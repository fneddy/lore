use crate::match_set::MatchSet;
use crate::params::SearchParams;
use crate::search::Search;
use std::path::PathBuf;

/// Builds a `Search`. All methods are consuming (`mut self -> Self`)
/// so calls chain naturally; call order never affects the result.
pub struct SearchBuilder {
    paths: Vec<PathBuf>,
    excluded_paths: Vec<PathBuf>,
    match_set: Option<MatchSet>,
    exclude_patterns: Vec<String>,
    extensions: Vec<String>,
    excluded_extensions: Vec<String>,
    use_builtins: bool,
    include_path_in_search: bool,
    walk_yield_interval: usize,
}

impl Default for SearchBuilder {
    fn default() -> Self {
        Self {
            paths: vec![],
            excluded_paths: vec![],
            match_set: None,
            exclude_patterns: vec![],
            extensions: vec![],
            excluded_extensions: vec![],
            use_builtins: true,
            include_path_in_search: false,
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

    /// Exclude a path prefix from the walk entirely.
    pub fn exclude_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.excluded_paths.push(path.into());
        self
    }

    /// Sets the match expression. See `MatchSet` for OR/AND grouping.
    pub fn matching(mut self, set: MatchSet) -> Self {
        self.match_set = Some(set);
        self
    }

    /// Excludes files whose searched text contains this substring.
    /// Always requires extraction, regardless of what else is set.
    pub fn exclude_match(mut self, pattern: impl Into<String>) -> Self {
        self.exclude_patterns.push(pattern.into());
        self
    }

    /// Restrict to this extension. Repeatable — acts as an allowlist.
    pub fn extension(mut self, ext: impl Into<String>) -> Self {
        self.extensions.push(ext.into());
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

    /// How many files the walker pushes into the backlog before
    /// cooperatively yielding via `tokio::task::yield_now()`. Default
    /// 1024, clamped to a minimum of 1.
    pub fn walk_yield_interval(mut self, interval: usize) -> Self {
        self.walk_yield_interval = interval.max(1);
        self
    }

    /// Freeze into an immutable, cheaply-cloneable `Search`.
    pub fn build(self) -> Search {
        let params = SearchParams {
            paths: self.paths,
            excluded_paths: self.excluded_paths,
            match_set: self.match_set,
            exclude_patterns: self.exclude_patterns,
            extensions: self.extensions,
            excluded_extensions: self.excluded_extensions,
            use_builtins: self.use_builtins,
            include_path_in_search: self.include_path_in_search,
            walk_yield_interval: self.walk_yield_interval,
        };
        Search::new(params)
    }
}
