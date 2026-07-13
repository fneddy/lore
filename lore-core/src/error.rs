/// Errors that can occur while searching or extracting documentation.
#[derive(Debug, thiserror::Error)]
pub enum LoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no extractor for extension: {0}")]
    UnsupportedFormat(String),

    #[error("extraction failed: {0}")]
    Extraction(String),
}

// Match derives Clone, but std::io::Error is not Clone, so LoreError
// needs a hand-rolled Clone impl that preserves the message rather
// than the original error.
impl Clone for LoreError {
    fn clone(&self) -> Self {
        match self {
            LoreError::Io(e) => LoreError::Extraction(format!("io error: {e}")),
            LoreError::UnsupportedFormat(s) => LoreError::UnsupportedFormat(s.clone()),
            LoreError::Extraction(s) => LoreError::Extraction(s.clone()),
        }
    }
}
