use crate::error::DocfError;
use std::path::PathBuf;

/// A single search result: a file that satisfied the search criteria.
#[derive(Debug, Clone)]
pub struct Match {
    pub path: PathBuf,
    pub source: SourceKind,
    /// `Ok` if extraction (when needed) succeeded and the file matched.
    /// `Err` if the file passed path filters but extraction failed —
    /// surfaced rather than silently dropped.
    pub extracted: Result<(), DocfError>,
    /// Byte offsets into the searched text (path+content, or content
    /// only — see `include_path_in_search`) for every satisfied term,
    /// from every fully-satisfied OR-group, not just the first.
    pub offsets: Vec<usize>,
}

/// Which built-in (or user-supplied) source a match came from.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Cargo,
    Man,
    Project,
    UserPath,
}
