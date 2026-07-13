use crate::match_::{Match, SourceKind};
use crate::params::SearchParams;
use crate::pipeline::{file_matches, passes_path_filters};
use crate::source::builtin_sources;
use crossbeam_deque::{Injector, Steal};
use futures_core::Stream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

/// Shared, lock-free coordination state for a `Search`. Filled by
/// whichever clone claims the walker role; drained concurrently by
/// every clone (including the walker, once its walk completes).
struct WorkState {
    injector: Injector<(PathBuf, SourceKind)>,
    in_flight: AtomicUsize,
    walking_done: AtomicBool,
    walker_claimed: AtomicBool,
}

impl WorkState {
    fn new() -> Self {
        Self {
            injector: Injector::new(),
            in_flight: AtomicUsize::new(0),
            walking_done: AtomicBool::new(false),
            walker_claimed: AtomicBool::new(false),
        }
    }
}

/// A configured, ready-to-run search. Cheap to clone (two `Arc`
/// bumps) — clone it across as many spawned tasks as you want
/// concurrency for; the library coordinates them safely and does not
/// itself spawn anything or choose a concurrency level.
#[derive(Clone)]
pub struct Search(Arc<SearchParams>, Arc<WorkState>);

impl Search {
    pub(crate) fn new(params: SearchParams) -> Self {
        Self(Arc::new(params), Arc::new(WorkState::new()))
    }

    /// Resolves every scan root: built-in sources (unless
    /// `no_builtins()` was set) plus every `add_path()` call, each
    /// tagged with its `SourceKind`.
    fn resolve_roots(&self) -> Vec<(PathBuf, SourceKind)> {
        let params = &self.0;
        let mut roots = vec![];

        if params.use_builtins {
            for source in builtin_sources() {
                let kind = source.kind();
                for root in source.scan() {
                    roots.push((root, kind));
                }
            }
        }

        for p in &params.paths {
            roots.push((p.clone(), SourceKind::UserPath));
        }

        roots
    }

    /// The only entry point. Fully async; spawns no tasks and decides
    /// no concurrency level itself — the library only provides safe,
    /// dynamically load-balancing coordination for however many
    /// callers choose to run concurrently.
    ///
    /// Await-in-a-loop, like any other async iterator — no channel to
    /// construct or pass in. Call this from as many spawned task
    /// clones as desired. Whichever clone's stream is polled first
    /// claims the walker role and pushes discovered files into a
    /// shared backlog as it walks; every clone's stream, including the
    /// walker's, concurrently steals from that backlog and yields
    /// matches as items are requested. A single un-spawned call is
    /// valid and behaves like a plain sequential iterator.
    ///
    /// The walker periodically yields control (every
    /// `walk_yield_interval` files, default 1024) via
    /// `tokio::task::yield_now()`, so even on a single-threaded
    /// runtime other clones get a chance to drain a partially-filled
    /// backlog rather than waiting for the full walk.
    pub fn run(&self) -> std::pin::Pin<Box<dyn Stream<Item = Match> + Send + '_>> {
        Box::pin(async_stream::stream! {
            let params = self.0.clone();
            let work = self.1.clone();

            let is_walker = work
                .walker_claimed
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok();

            if is_walker {
                let roots = self.resolve_roots();
                let mut since_yield = 0usize;

                for (root, kind) in roots {
                    for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
                        if !entry.file_type().is_file() {
                            continue;
                        }
                        let path = entry.into_path();
                        if !passes_path_filters(&path, &params) {
                            continue;
                        }

                        work.in_flight.fetch_add(1, Ordering::SeqCst);
                        work.injector.push((path, kind));

                        since_yield += 1;
                        if since_yield >= params.walk_yield_interval {
                            since_yield = 0;
                            tokio::task::yield_now().await;
                        }
                    }
                }
                work.walking_done.store(true, Ordering::SeqCst);
            }

            loop {
                match work.injector.steal() {
                    Steal::Success((path, kind)) => {
                        work.in_flight.fetch_sub(1, Ordering::SeqCst);
                        if let Some(m) = file_matches(&path, kind, &params) {
                            yield m;
                        }
                    }
                    Steal::Empty
                        if work.walking_done.load(Ordering::SeqCst)
                            && work.in_flight.load(Ordering::SeqCst) == 0 =>
                    {
                        break;
                    }
                    _ => tokio::task::yield_now().await,
                }
            }
        })
    }
}
