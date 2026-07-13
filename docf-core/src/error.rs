use std::fmt;

/// Errors that can occur while searching or extracting documentation.
#[derive(Debug, thiserror::Error)]
pub enum DocfError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no extractor for extension: {0}")]
    UnsupportedFormat(String),

    #[error("extraction failed: {0}")]
    Extraction(String),
}

// Match derives Clone, but std::io::Error is not Clone, so DocfError
// needs a hand-rolled Clone impl that preserves the message rather
// than the original error.
impl Clone for DocfError {
    fn clone(&self) -> Self {
        match self {
            DocfError::Io(e) => DocfError::Extraction(format!("io error: {e}")),
            DocfError::UnsupportedFormat(s) => DocfError::UnsupportedFormat(s.clone()),
            DocfError::Extraction(s) => DocfError::Extraction(s.clone()),
        }
    }
}

impl DocfError {
    pub(crate) fn extraction(msg: impl fmt::Display) -> Self {
        DocfError::Extraction(msg.to_string())
    }
}
