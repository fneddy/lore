use crate::error::LoreError;
use crate::extract::extractor_for;
use aho_corasick::AhoCorasick;
use crossbeam_deque::{Injector, Steal};
use futures_core::Stream;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

/// A single search result: a file that satisfied the search criteria.
#[derive(Debug, Clone)]
pub struct Match {
    pub path: PathBuf,
    /// `Ok` if extraction (when needed) succeeded and the file matched.
    /// `Err` if the file passed path filters but extraction failed —
    /// surfaced rather than silently dropped.
    pub extracted: Result<(), LoreError>,
    /// Byte offsets into the searched text for matched terms.
    pub offsets: Vec<usize>,
    /// Query terms that matched in this file (for sorting by match count).
    pub matched_terms: Vec<String>,
    /// The extracted bytes, carried along only when `keep_content()`
    /// was set on the builder — lets consumers display context without
    /// extracting the file a second time. `None` otherwise.
    pub content: Option<Vec<u8>>,
}

/// Frozen search configuration. Never constructed directly by
/// consumers — build one via `SearchBuilder`.
pub(crate) struct SearchParams {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) patterns: Vec<String>,
    pub(crate) exclude_patterns: Vec<String>,
    pub(crate) excluded_extensions: Vec<String>,
    pub(crate) include_path_in_search: bool,
    pub(crate) keep_content: bool,
    pub(crate) walk_yield_interval: usize,
}

/// Shared, lock-free coordination state for a `Search`. Filled by
/// whichever clone claims the walker role; drained concurrently by
/// every clone (including the walker, once its walk completes).
struct WorkState {
    injector: Injector<PathBuf>,
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

/// A configured, ready-to-run search. Cheap to clone (three `Arc`
/// bumps) — clone it across as many spawned tasks as you want
/// concurrency for; the library coordinates them safely and does not
/// itself spawn anything or choose a concurrency level.
#[derive(Clone)]
pub struct Search {
    params: Arc<SearchParams>,
    work: Arc<WorkState>,
    /// Pre-built AhoCorasick automaton for pattern matching (case-insensitive)
    pattern_matcher: Arc<Option<AhoCorasick>>,
    /// Pre-built AhoCorasick automaton for exclusion matching (case-insensitive)
    exclude_matcher: Arc<Option<AhoCorasick>>,
    /// Pre-built AhoCorasick automaton for path exclusion matching (case-insensitive)
    path_exclude_matcher: Arc<Option<AhoCorasick>>,
}

impl Search {
    pub(crate) fn new(
        params: SearchParams,
        pattern_matcher: Option<AhoCorasick>,
        exclude_matcher: Option<AhoCorasick>,
        path_exclude_matcher: Option<AhoCorasick>,
    ) -> Self {
        Self {
            params: Arc::new(params),
            work: Arc::new(WorkState::new()),
            pattern_matcher: Arc::new(pattern_matcher),
            exclude_matcher: Arc::new(exclude_matcher),
            path_exclude_matcher: Arc::new(path_exclude_matcher),
        }
    }

    /// Resolves every scan root: built-in sources (unless
    /// `no_builtins()` was set) plus every `add_path()` call.
    fn resolve_roots(&self) -> Vec<PathBuf> {
        self.params.paths.clone()
    }

    /// Excluded-path-pattern check. Applied to every walked entry —
    /// including directories, so matching subtrees are pruned without
    /// being descended into. Sound because the matcher is a substring
    /// match on the full path: a matching directory implies every
    /// descendant path matches too.
    fn passes_path_exclusion(&self, path: &Path) -> bool {
        match *self.path_exclude_matcher {
            Some(ref matcher) => !matcher.is_match(path.as_os_str().as_encoded_bytes()),
            None => true,
        }
    }

    /// Extension denylist check. Cheap — no extraction, no allocation;
    /// `excluded_extensions` is pre-normalized (dot-stripped) at build
    /// time.
    fn passes_extension_filter(&self, path: &Path) -> bool {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        !self.params
            .excluded_extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Concatenates path + extracted content when `include_path_in_search`
    /// is set, so a single pattern can hit either half without being
    /// checked separately. Borrows the extracted bytes as-is otherwise —
    /// no copy in the common case.
    fn build_searchable_bytes<'a>(&self, path: &Path, extracted: &'a [u8]) -> Cow<'a, [u8]> {
        if self.params.include_path_in_search {
            let path_str = path.to_string_lossy();
            let mut result = Vec::with_capacity(path_str.len() + 1 + extracted.len());
            result.extend_from_slice(path_str.as_bytes());
            result.push(b'\n');
            result.extend_from_slice(extracted);
            Cow::Owned(result)
        } else {
            Cow::Borrowed(extracted)
        }
    }

    /// Simple OR matching: returns offsets and matched terms if ANY pattern matches.
    /// Returns `None` if excluded or if no patterns match.
    /// Uses pre-built AhoCorasick matchers for efficiency.
    fn document_matches(&self, text: &[u8]) -> Option<(Vec<usize>, Vec<String>)> {
        // Check exclusions first using pre-built matcher
        if let Some(ref exclude_matcher) = *self.exclude_matcher {
            if exclude_matcher.is_match(text) {
                return None;
            }
        }

        // If no patterns, return empty match
        if self.params.patterns.is_empty() {
            return Some((vec![], vec![]));
        }

        // Use pre-built pattern matcher (always present when patterns is non-empty)
        let pattern_matcher = self.pattern_matcher.as_ref().as_ref()?;

        let mut offsets = vec![];
        // Small fixed-size bitset over pattern indices — no hashing.
        let mut matched = vec![false; self.params.patterns.len()];

        // Non-overlapping (leftmost-first) matching: each text position
        // counts at most once, which is what the relevance sort and
        // context display want.
        for m in pattern_matcher.find_iter(text) {
            matched[m.pattern().as_usize()] = true;
            offsets.push(m.start());
        }

        if offsets.is_empty() {
            return None;
        }

        // Collect matched terms using original pattern strings
        let matched_terms: Vec<String> = matched
            .iter()
            .enumerate()
            .filter(|(_, &hit)| hit)
            .map(|(idx, _)| self.params.patterns[idx].clone())
            .collect();

        Some((offsets, matched_terms))
    }

    /// The core per-file decision: does this file pass path filters, and
    /// (if extraction is needed) does its searchable text satisfy the
    /// match set? Returns `None` for files that simply don't match or aren't
    /// extractable; returns `Some(Match)` with `extracted: Err(..)` for
    /// files that matched path filters but failed extraction, so failures
    /// are surfaced rather than silently dropped.
    fn file_matches(&self, path: &Path) -> Option<Match> {
        let needs_extraction = self.params.include_path_in_search
            || !self.params.exclude_patterns.is_empty()
            || !self.params.patterns.is_empty();

        if !needs_extraction {
            // No content filter at all — every path-filtered file matches.
            return Some(Match {
                path: path.to_owned(),
                extracted: Ok(()),
                offsets: vec![],
                matched_terms: vec![],
                content: None,
            });
        }

        let extractor = extractor_for(path)?;

        match extractor.extract(path) {
            Ok(extracted) => {
                // Scope `bytes` so its borrow of `extracted` ends
                // before `extracted` is (optionally) moved into the Match.
                let matched = {
                    let bytes = self.build_searchable_bytes(path, &extracted);
                    self.document_matches(&bytes)
                };
                matched.map(|(offsets, matched_terms)| Match {
                    path: path.to_owned(),
                    extracted: Ok(()),
                    offsets,
                    matched_terms,
                    content: self.params.keep_content.then_some(extracted),
                })
            }
            Err(e) => Some(Match {
                path: path.to_owned(),
                extracted: Err(LoreError::from(e)),
                offsets: vec![],
                matched_terms: vec![],
                content: None,
            }),
        }
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
            // Leader election only — the injector does its own
            // synchronization, so no ordering stronger than Acquire is
            // needed on success (Relaxed on failure: losers learn
            // nothing they act on).
            let is_walker = self.work
                .walker_claimed
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok();

            if is_walker {
                let roots = self.resolve_roots();
                let mut since_yield = 0usize;

                for root in roots {
                    let walker = WalkDir::new(&root)
                        .into_iter()
                        // Prunes excluded subtrees at the directory
                        // level — the walker never descends into them.
                        .filter_entry(|e| self.passes_path_exclusion(e.path()));
                    for entry in walker.filter_map(|e| e.ok()) {
                        if !entry.file_type().is_file() {
                            continue;
                        }
                        let path = entry.into_path();
                        if !self.passes_extension_filter(&path) {
                            continue;
                        }

                        // Relaxed is enough: every increment is
                        // sequenced before the Release store of
                        // `walking_done`, so consumers that observe
                        // `walking_done == true` (Acquire) see all of
                        // them. See the termination check below.
                        self.work.in_flight.fetch_add(1, Ordering::Relaxed);
                        self.work.injector.push(path);

                        since_yield += 1;
                        if since_yield >= self.params.walk_yield_interval {
                            since_yield = 0;
                            tokio::task::yield_now().await;
                        }
                    }
                }
                self.work.walking_done.store(true, Ordering::Release);
            }

            // Termination: break only once the walk is over AND every
            // pushed item has been claimed. Ordering argument: the
            // Acquire load of `walking_done` synchronizes with the
            // walker's Release store, making every `in_flight`
            // increment visible; decrements observed "late" merely
            // cause another loop iteration, never a missed item. So
            // Relaxed suffices for the counter itself.
            loop {
                match self.work.injector.steal() {
                    Steal::Success(path) => {
                        self.work.in_flight.fetch_sub(1, Ordering::Relaxed);
                        if let Some(m) = self.file_matches(&path) {
                            yield m;
                        }
                    }
                    Steal::Empty
                        if self.work.walking_done.load(Ordering::Acquire)
                            && self.work.in_flight.load(Ordering::Relaxed) == 0 =>
                    {
                        break;
                    }
                    _ => tokio::task::yield_now().await,
                }
            }
        })
    }
}
