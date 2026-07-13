use crate::error::DocfError;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Extracts searchable plain text from a file. One implementation per
/// format, dispatched by extension via `extractor_for`. Sync only —
/// no async variant, to avoid maintaining two code paths per format.
pub trait Extractable: Send + Sync {
    fn extract(&self, path: &Path) -> std::io::Result<String>;
}

/// Returns the appropriate extractor for a path's extension, or `None`
/// if the extension isn't recognized at all (as opposed to recognized
/// but unimplemented — see `PdfExtractor`/`OfficeExtractor`).
pub fn extractor_for(path: &Path) -> Option<Box<dyn Extractable>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "html" | "htm" => Some(Box::new(HtmlExtractor)),
        "pdf" => Some(Box::new(PdfExtractor)),
        "docx" | "pptx" | "xlsx" | "epub" | "odt" => Some(Box::new(OfficeExtractor)),
        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => Some(Box::new(ManExtractor)),
        "" => None,
        _ => Some(Box::new(PlainTextExtractor)),
    }
}

/// Zero-cost wrapper around `fs::read_to_string` for already-plain-text
/// formats (.md, .txt, .rs, .toml, ...).
pub struct PlainTextExtractor;

impl Extractable for PlainTextExtractor {
    fn extract(&self, path: &Path) -> std::io::Result<String> {
        fs::read_to_string(path)
    }
}

/// Strips HTML tags via `html2text`. Whole-document parse, not
/// streaming — see the crate-level known-limitation note.
pub struct HtmlExtractor;

impl Extractable for HtmlExtractor {
    fn extract(&self, path: &Path) -> std::io::Result<String> {
        let bytes = fs::read(path)?;
        Ok(html2text::from_read(bytes.as_slice(), 120))
    }
}

/// Shells out to `man --Tutf8 -l <path>` to render troff source to
/// plain text. No pure-Rust troff parser exists, so this is a
/// deliberate, narrow exception to "no subprocesses" elsewhere in the
/// extraction layer.
pub struct ManExtractor;

impl Extractable for ManExtractor {
    fn extract(&self, path: &Path) -> std::io::Result<String> {
        let output = Command::new("man")
            .arg("--Tutf8")
            .arg("-l")
            .arg(path)
            .output()?;

        if !output.status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "man exited with status {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        String::from_utf8(output.stdout)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

/// Stub. A real implementation would use `pdf-extract` (page-by-page
/// via `poppler-rs` is preferable — see design discussion). Not wired
/// up in this build; see the note in `Cargo.toml`.
pub struct PdfExtractor;

impl Extractable for PdfExtractor {
    fn extract(&self, _path: &Path) -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            DocfError::UnsupportedFormat("pdf (extractor not linked in this build)".into()),
        ))
    }
}

/// Stub. A real implementation would use `dotext` (docx/pptx/xlsx/epub
/// all implement `Read` directly). Not wired up in this build.
pub struct OfficeExtractor;

impl Extractable for OfficeExtractor {
    fn extract(&self, path: &Path) -> std::io::Result<String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("office")
            .to_string();
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            DocfError::UnsupportedFormat(format!("{ext} (extractor not linked in this build)")),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_with_ext(dir: &tempfile::TempDir, name: &str, contents: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let mut f = fs::File::create(&path).unwrap();
        write!(f, "{contents}").unwrap();
        path
    }

    #[test]
    fn plain_text_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = temp_with_ext(&dir, "notes.md", "# hello\nworld");
        let extractor = extractor_for(&p).unwrap();
        let text = extractor.extract(&p).unwrap();
        assert_eq!(text, "# hello\nworld");
    }

    #[test]
    fn html_strips_tags() {
        let dir = tempfile::tempdir().unwrap();
        let p = temp_with_ext(&dir, "page.html", "<html><body><p>hello world</p></body></html>");
        let extractor = extractor_for(&p).unwrap();
        let text = extractor.extract(&p).unwrap();
        assert!(text.contains("hello world"));
        assert!(!text.contains("<p>"));
    }

    #[test]
    fn no_extension_returns_none() {
        assert!(extractor_for(Path::new("Makefile")).is_none());
    }

    #[test]
    fn pdf_is_unsupported_stub() {
        let extractor = extractor_for(Path::new("foo.pdf")).unwrap();
        assert!(extractor.extract(Path::new("foo.pdf")).is_err());
    }
}
