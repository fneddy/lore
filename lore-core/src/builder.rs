use crate::search::{Search, SearchParams};
use aho_corasick::AhoCorasick;
use std::path::PathBuf;

/// Builds a `Search`. All methods are consuming (`mut self -> Self`)
/// so calls chain naturally; call order never affects the result.
pub struct SearchBuilder {
    paths: Vec<PathBuf>,
    excluded_path_patterns: Vec<String>,
    patterns: Vec<String>,
    exclude_patterns: Vec<String>,
    excluded_extensions: Vec<String>,
    use_builtins: bool,
    include_path_in_search: bool,
    keep_content: bool,
    walk_yield_interval: usize,
}

impl Default for SearchBuilder {
    fn default() -> Self {
        Self {
            paths: vec![],
            excluded_path_patterns: vec![],
            patterns: vec![],
            exclude_patterns: vec![],
            excluded_extensions: vec![],
            use_builtins: true,
            include_path_in_search: false,
            keep_content: false,
            walk_yield_interval: 1024,
        }
    }
}

impl SearchBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a scan root, on top of built-in sources.
    pub fn add_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.paths.push(path.into());
        self
    }

    /// Exclude paths matching a glob pattern (e.g., "**/translation/**", "*translation*").
    pub fn exclude_path_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.excluded_path_patterns.push(pattern.into());
        self
    }

    /// Add a search pattern. All patterns are ORed together.
    pub fn add_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.patterns.push(pattern.into());
        self
    }

    /// Set all search patterns at once. All patterns are ORed together.
    pub fn patterns(mut self, patterns: Vec<String>) -> Self {
        self.patterns = patterns;
        self
    }

    /// Excludes files whose searched text contains this substring.
    /// Always requires extraction, regardless of what else is set.
    pub fn exclude_match(mut self, pattern: impl Into<String>) -> Self {
        self.exclude_patterns.push(pattern.into());
        self
    }

    /// Exclude this extension. Repeatable.
    pub fn exclude_extension(mut self, ext: impl Into<String>) -> Self {
        self.excluded_extensions.push(ext.into());
        self
    }

    /// Skip built-in sources (cargo, man, project). Only affects
    /// sources, not extractors — every supported file type is still
    /// extractable regardless of this flag.
    pub fn no_builtins(mut self) -> Self {
        self.use_builtins = false;
        self
    }

    /// If set, the file's path is prepended to its extracted content
    /// before matching, so a pattern can hit either the path or the
    /// body text without being checked separately. Off by default.
    pub fn include_path_in_search(mut self, include: bool) -> Self {
        self.include_path_in_search = include;
        self
    }

    /// If set, each `Match` carries the extracted content bytes in its
    /// `content` field, so consumers that display context don't have
    /// to extract the file a second time. Off by default — leaving it
    /// off keeps matches small when content isn't needed.
    pub fn keep_content(mut self, keep: bool) -> Self {
        self.keep_content = keep;
        self
    }

    /// How many files the walker pushes into the backlog before
    /// cooperatively yielding via `tokio::task::yield_now()`. Default
    /// 1024, clamped to a minimum of 1.
    pub fn walk_yield_interval(mut self, interval: usize) -> Self {
        self.walk_yield_interval = interval.max(1);
        self
    }

    /// Freeze into an immutable, cheaply-cloneable `Search`.
    pub fn build(self) -> Search {
        // Build pattern matcher once if patterns exist
        let pattern_matcher = if !self.patterns.is_empty() {
            AhoCorasick::builder()
                .ascii_case_insensitive(true)
                .build(&self.patterns)
                .ok()
        } else {
            None
        };

        // Build exclude matcher once if exclude patterns exist
        let exclude_matcher = if !self.exclude_patterns.is_empty() {
            AhoCorasick::builder()
                .ascii_case_insensitive(true)
                .build(&self.exclude_patterns)
                .ok()
        } else {
            None
        };

        // Build path exclude matcher once if path patterns exist
        let path_exclude_matcher = if !self.excluded_path_patterns.is_empty() {
            AhoCorasick::builder()
                .ascii_case_insensitive(true)
                .build(&self.excluded_path_patterns)
                .ok()
        } else {
            None
        };

        // Normalize once here so the per-file filter compares without
        // allocating: strip any leading dot ("md" and ".md" are
        // equivalent); comparison itself is case-insensitive.
        let excluded_extensions = self
            .excluded_extensions
            .into_iter()
            .map(|e| e.trim_start_matches('.').to_string())
            .collect();

        let params = SearchParams {
            paths: self.paths,
            patterns: self.patterns,
            exclude_patterns: self.exclude_patterns,
            excluded_extensions,
            include_path_in_search: self.include_path_in_search,
            keep_content: self.keep_content,
            walk_yield_interval: self.walk_yield_interval,
        };
        Search::new(params, pattern_matcher, exclude_matcher, path_exclude_matcher)
    }
}
