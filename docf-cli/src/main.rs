//! `docf` — minimal documentation finder. MVP CLI over `docf-core`.
//!
//! Usage: docf [QUERY]... [-p PATH]...
//!
//! Multiple QUERY terms are ORed together (each becomes its own group
//! in the underlying MatchSet). Multiple -p flags add extra scan
//! roots on top of the library's built-in sources.

use docf_core::{MatchSet, SearchBuilder};
use futures_util::StreamExt;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const WORKERS: usize = 8; // the CLI's own fixed choice; the library imposes none

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let mut builder = SearchBuilder::new();
    let mut query_terms: Vec<String> = vec![];

    let mut args = lexopt::Parser::from_env();
    loop {
        let arg = match args.next() {
            Ok(Some(a)) => a,
            Ok(None) => break,
            Err(e) => {
                eprintln!("docf: {e}");
                return ExitCode::from(2);
            }
        };

        use lexopt::Arg::*;
        match arg {
            Value(v) => match v.into_string() {
                Ok(s) => query_terms.push(s),
                Err(_) => {
                    eprintln!("docf: invalid UTF-8 in query argument");
                    return ExitCode::from(2);
                }
            },
            Short('p') | Long("path") => match args.value() {
                Ok(v) => builder = builder.add_path(v),
                Err(e) => {
                    eprintln!("docf: -p requires a path argument: {e}");
                    return ExitCode::from(2);
                }
            },
            Short('h') | Long("help") => {
                print_help();
                return ExitCode::SUCCESS;
            }
            Short('V') | Long("version") => {
                println!("docf {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("docf: unrecognized argument: {arg:?}");
                return ExitCode::from(2);
            }
        }
    }

    if !query_terms.is_empty() {
        let mut set = MatchSet::new();
        for (i, term) in query_terms.into_iter().enumerate() {
            if i > 0 {
                set = set.or();
            }
            set = set.add(term);
        }
        builder = builder.matching(set);
    }

    let search = builder.build();
    let found = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::with_capacity(WORKERS);

    for _ in 0..WORKERS {
        let search = search.clone();
        let found = found.clone();
        handles.push(tokio::spawn(async move {
            let mut stream = search.run();
            while let Some(m) = stream.next().await {
                match &m.extracted {
                    Ok(()) => {
                        println!("{}", m.path.display());
                        found.store(true, Ordering::Relaxed);
                    }
                    Err(e) => {
                        eprintln!("docf: {}: {e}", m.path.display());
                    }
                }
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    if found.load(Ordering::Relaxed) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn print_help() {
    println!(
        "docf {}
minimal documentation finder

USAGE:
    docf [QUERY]... [-p PATH]...

ARGS:
    QUERY    Substring(s) to search for. Multiple QUERY terms are ORed
             together. Matched against extracted file content (and
             against the path too, for files cheap enough to check
             without extraction).

OPTIONS:
    -p, --path <PATH>   Add an extra scan root, on top of built-in
                         sources (cargo registry, man pages, project
                         files). Repeatable.
    -h, --help          Print this help and exit.
    -V, --version       Print version and exit.",
        env!("CARGO_PKG_VERSION")
    );
}
