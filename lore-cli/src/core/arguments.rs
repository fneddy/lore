//! CLI argument parsing shared across all lore binaries.

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Cli {
    /// Search terms — each term is ORed (matches any term).
    pub query: Vec<String>,

    /// Add a scan root, on top of built-in sources. Repeatable.
    #[arg(short = 'p', long = "path")]
    pub paths: Vec<PathBuf>,

    /// Exclude paths containing this substring (e.g., "translation", "temp"). Repeatable.
    #[arg(short = 'P', long = "exclude-path-pattern")]
    pub exclude_path_patterns: Vec<String>,

    /// Exclude files whose searched text contains this substring.
    /// Always requires extraction. Repeatable.
    #[arg(short = 'X', long = "exclude-match")]
    pub exclude_match: Vec<String>,

    /// Exclude this extension. Repeatable.
    #[arg(short = 'T', long = "exclude-extension")]
    pub exclude_extensions: Vec<String>,

    /// Fold each file's path into its searched text, so a pattern can
    /// hit either the path or the body without being checked
    /// separately.
    #[arg(short = 'i', long = "include-path", default_value_t = false)]
    pub include_path: bool,

    /// How many files the walker pushes into the backlog before
    /// cooperatively yielding. Lower values improve responsiveness on
    /// single-threaded runtimes at a small throughput cost.
    #[arg(long = "walk-yield-interval", default_value_t = 1024)]
    pub walk_yield_interval: usize,

    /// How many concurrent Search clones this CLI spawns. Not a
    /// lore-core property — concurrency is caller-owned by design;
    /// this is purely the CLI's own choice of how many tasks to run.
    #[arg(short = 'j', long = "workers", default_value_t = num_cpus::get().saturating_sub(2).max(1))]
    pub workers: usize,

    /// Show n lines of context around each match. When set, extracts
    /// and displays the matched content with surrounding lines.
    /// Defaults to 3 if -c is passed without a value, 0 if not passed.
    #[arg(short = 'c', long = "context", default_missing_value = "3", num_args = 0..=1, require_equals = false)]
    pub context: Option<usize>,

    /// Stream results as they arrive instead of collecting and sorting by relevance.
    /// By default, results are collected and sorted by number of matched terms.
    #[arg(long = "stream", default_value_t = false)]
    pub stream: bool,
}


