//! Search building and execution functionality.

use crate::arguments::Cli;
use futures_util::StreamExt;
use lore_core::SearchBuilder;
use serde::Serialize;
use std::path::PathBuf;
use std::process::ExitCode;

use tokio::sync::mpsc;

pub fn build_search(cli: &Cli) -> lore_core::Search {
    let mut builder = SearchBuilder::new();

    for p in &cli.paths {
        builder = builder.add_path(p.clone());
    }
    for p in &cli.exclude_path_patterns {
        builder = builder.exclude_path_pattern(p.clone());
    }
    for x in &cli.exclude_match {
        builder = builder.exclude_match(x.clone());
    }
    for e in &cli.exclude_extensions {
        builder = builder.exclude_extension(e.clone());
    }
    if cli.include_path {
        builder = builder.include_path_in_search(true);
    }
    // Context display needs the extracted content; carrying it on the
    // Match avoids extracting every displayed file a second time.
    if cli.context.unwrap_or(0) > 0 {
        builder = builder.keep_content(true);
    }
    builder = builder.walk_yield_interval(cli.walk_yield_interval);

    // All query terms are ORed
    if !cli.query.is_empty() {
        builder = builder.patterns(cli.query.clone());
    }

    builder.build()
}

/// Result of a search match
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub path: PathBuf,
    #[serde(skip_serializing)]
    pub offsets: Vec<usize>,
    pub filesize: u64,
    pub matched_terms: Vec<String>,
    /// Extracted content, present only when the search was built with
    /// `keep_content` (i.e. context display is on).
    #[serde(skip_serializing)]
    pub content: Option<Vec<u8>>,
}

/// Error during search
#[derive(Debug, Clone, Serialize)]
pub struct SearchError {
    pub path: PathBuf,
    pub error: String,
}

/// Message from search stream
#[derive(Debug, Clone)]
pub enum SearchMessage {
    Match(SearchResult),
    Error(SearchError),
}

/// Runs a search and returns a channel receiver for results.
/// This is the common search runner used by all binaries.
/// Each binary can consume the channel differently (print, format as string, etc.)
pub async fn run_search_stream(cli: &Cli) -> mpsc::Receiver<SearchMessage> {
    let workers = cli.workers.max(1);
    let search = build_search(cli);

    // Create a channel for results
    // Buffer size of 100 to avoid blocking workers too much
    let (tx, rx) = mpsc::channel::<SearchMessage>(100);

    // Spawn worker tasks
    for _ in 0..workers {
        let search = search.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut stream = search.run();
            while let Some(m) = stream.next().await {
                let msg = match m.extracted {
                    Ok(()) => {
                        let filesize = std::fs::metadata(&m.path)
                            .map(|meta| meta.len())
                            .unwrap_or(0);
                        SearchMessage::Match(SearchResult {
                            path: m.path,
                            offsets: m.offsets,
                            filesize,
                            matched_terms: m.matched_terms,
                            content: m.content,
                        })
                    }
                    Err(e) => SearchMessage::Error(SearchError {
                        path: m.path,
                        error: e.to_string(),
                    }),
                };
                if tx.send(msg).await.is_err() {
                    break; // Receiver dropped
                }
            }
        });
    }

    // Drop the original sender so receiver knows when all workers are done
    drop(tx);

    rx
}

/// Displays a match with optional context lines around each offset.
fn display_match(result: &SearchResult, context: usize) {
    let match_count = result.offsets.len();
    let unique_terms = result.matched_terms.len();
    let terms_display = if result.matched_terms.is_empty() {
        String::from("none")
    } else {
        result.matched_terms.join(",")
    };
    
    // Single line output: path | matches | terms | size
    println!("{} | matches:{} terms:{} [{}] size:{}", 
             result.path.display(), match_count, unique_terms, terms_display, result.filesize);
    
    if context == 0 || result.offsets.is_empty() {
        return;
    }

    // Content is carried on the result when context display is on —
    // no second extraction.
    let Some(content) = &result.content else {
        return;
    };

    // Convert bytes to string for display
    let text = String::from_utf8_lossy(content);
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return;
    }

    // Precompute the byte offset each line starts at, once; each match
    // offset then maps to its line with a binary search instead of a
    // scan from the top of the file.
    let mut line_starts = Vec::with_capacity(lines.len());
    let mut byte_pos = 0usize;
    for line in &lines {
        line_starts.push(byte_pos);
        byte_pos += line.len() + 1; // +1 for newline
    }

    let mut displayed_ranges = std::collections::HashSet::new();

    for &offset in &result.offsets {
        // Find which line this offset is on
        let line_num = match line_starts.binary_search(&offset) {
            Ok(i) => i,                       // offset is exactly a line start
            Err(i) => i.saturating_sub(1),    // offset falls inside the previous line
        };

        // Calculate context range
        let start = line_num.saturating_sub(context);
        let end = (line_num + context + 1).min(lines.len());

        // Skip if we've already displayed this range
        if !displayed_ranges.insert((start, end)) {
            continue;
        }

        // Display the context
        for i in start..end {
            let marker = if i == line_num { ">" } else { " " };
            println!("{} {:4} | {}", marker, i + 1, lines[i]);
        }
        if end < lines.len() {
            println!("  ----");
        }
    }
}

/// Restore the default SIGPIPE disposition. Rust ignores SIGPIPE at
/// startup, which turns `lore ... | head` into a "failed printing to
/// stdout: Broken pipe" panic once the reader exits; with the default
/// disposition the process just terminates silently, like grep.
fn reset_sigpipe() {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// CLI-specific search runner that prints results to stdout.
/// Used by lore, system-lore, and project-lore binaries.
/// (Not by mcp-lore — dying on SIGPIPE is the right behavior for a
/// printing CLI, and only for a printing CLI.)
pub async fn run_search(cli: Cli) -> ExitCode {
    reset_sigpipe();
    let context = cli.context.unwrap_or(0);
    let stream_mode = cli.stream;
    let mut found = false;

    let mut rx = run_search_stream(&cli).await;

    if stream_mode {
        // Stream mode: print results as they arrive
        while let Some(msg) = rx.recv().await {
            match msg {
                SearchMessage::Match(result) => {
                    found = true;
                    display_match(&result, context);
                }
                SearchMessage::Error(err) => {
                    eprintln!("lore: {}: {}", err.path.display(), err.error);
                }
            }
        }
    } else {
        // Collect mode: gather all results, sort by relevance, then print
        let mut results = Vec::new();
        let mut errors = Vec::new();

        while let Some(msg) = rx.recv().await {
            match msg {
                SearchMessage::Match(result) => {
                    results.push(result);
                }
                SearchMessage::Error(err) => {
                    errors.push(err);
                }
            }
        }

        // Sort by relevance: 
        // 1. Number of unique terms matched (more terms = more relevant)
        // 2. Total match count (more matches = more relevant)
        // Example: "terms:2, matches:100" ranks higher than "terms:1, matches:500"
        results.sort_by(|a, b| {
            b.matched_terms.len()
                .cmp(&a.matched_terms.len())
                .then_with(|| b.offsets.len().cmp(&a.offsets.len()))
        });

        // Print sorted results
        for result in results {
            found = true;
            display_match(&result, context);
        }

        // Print errors at the end
        for err in errors {
            eprintln!("lore: {}: {}", err.path.display(), err.error);
        }
    }

    if found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
