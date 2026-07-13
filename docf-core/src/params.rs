use crate::match_set::MatchSet;
use std::path::PathBuf;

/// Frozen search configuration. Never constructed directly by
/// consumers — build one via `SearchBuilder`.
pub(crate) struct SearchParams {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) excluded_paths: Vec<PathBuf>,
    pub(crate) match_set: Option<MatchSet>,
    pub(crate) exclude_patterns: Vec<String>,
    pub(crate) extensions: Vec<String>,
    pub(crate) excluded_extensions: Vec<String>,
    pub(crate) use_builtins: bool,
    pub(crate) include_path_in_search: bool,
    pub(crate) walk_yield_interval: usize,
}
