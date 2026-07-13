use crate::error::LoreError;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Compression format detected from file extension
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compression {
    None,
    Gzip,
    Xz,
    Bzip2,
    Zstd,
}

impl Compression {
    fn from_ext(ext: &str) -> Self {
        if ext.eq_ignore_ascii_case("gz") {
            Compression::Gzip
        } else if ext.eq_ignore_ascii_case("xz") {
            Compression::Xz
        } else if ext.eq_ignore_ascii_case("bz2") {
            Compression::Bzip2
        } else if ext.eq_ignore_ascii_case("zst") || ext.eq_ignore_ascii_case("zstd") {
            Compression::Zstd
        } else {
            Compression::None
        }
    }
}

/// Document format resolved from the (inner) extension. `Plain` covers
/// every extension without a dedicated reader — bytes pass through
/// as-is after decompression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Epub,
    Pdf,
    Office,
    LegacyDoc,
    Plain,
}

impl Format {
    fn from_ext(ext: &str) -> Self {
        if ext.eq_ignore_ascii_case("epub") {
            Format::Epub
        } else if ext.eq_ignore_ascii_case("pdf") {
            Format::Pdf
        } else if ext.eq_ignore_ascii_case("docx")
            || ext.eq_ignore_ascii_case("pptx")
            || ext.eq_ignore_ascii_case("xlsx")
            || ext.eq_ignore_ascii_case("odt")
        {
            Format::Office
        } else if ext.eq_ignore_ascii_case("doc") {
            Format::LegacyDoc
        } else {
            Format::Plain
        }
    }
}

/// Resolve extension chain: compression + format. Allocation-free.
/// Examples:
///   "manual.txt.gz" -> (Gzip, txt)
///   "spec.adoc" -> (None, adoc)
///   "notes.info.xz" -> (Xz, info)
/// Returns `None` for files without an extension.
fn resolve_extensions(path: &Path) -> Option<(Compression, Format)> {
    let file_name = path.file_name()?.to_str()?;

    let mut parts = file_name.rsplit('.');
    let last = parts.next()?; // rsplit always yields at least one part
    let prev = parts.next()?; // no second part -> no dot -> no extension

    let compression = Compression::from_ext(last);

    // With compression the format is the second-to-last extension —
    // but only if one exists ("file.gz" has format ext "gz", i.e. Plain).
    let format_ext = if compression != Compression::None && parts.next().is_some() {
        prev
    } else {
        last
    };

    Some((compression, Format::from_ext(format_ext)))
}

/// Detect compression format from file content using magic bytes
fn detect_compression(data: &[u8]) -> Compression {
    // Use infer to detect compression format from content
    if let Some(kind) = infer::get(data) {
        match kind.mime_type() {
            "application/gzip" => return Compression::Gzip,
            "application/x-xz" => return Compression::Xz,
            "application/x-bzip2" => return Compression::Bzip2,
            "application/zstd" => return Compression::Zstd,
            _ => {}
        }
    }

    Compression::None
}

/// Detect document format from content magic bytes. `None` when
/// inconclusive — plain-text formats have no magic bytes, so the
/// extension stays the tiebreak for those.
fn detect_format(data: &[u8]) -> Option<Format> {
    let kind = infer::get(data)?;
    match kind.mime_type() {
        "application/pdf" => Some(Format::Pdf),
        "application/epub+zip" => Some(Format::Epub),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        | "application/vnd.oasis.opendocument.text" => Some(Format::Office),
        "application/msword" => Some(Format::LegacyDoc),
        _ => None,
    }
}

/// Decompress bytes based on compression format. Takes ownership so
/// the `None` case passes the buffer through without copying.
fn decompress(data: Vec<u8>, compression: Compression) -> io::Result<Vec<u8>> {
    match compression {
        Compression::None => Ok(data),

        Compression::Gzip => {
            use flate2::read::GzDecoder;
            let mut decoder = GzDecoder::new(&data[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        }

        Compression::Xz => {
            use xz2::read::XzDecoder;
            let mut decoder = XzDecoder::new(&data[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        }

        Compression::Bzip2 => {
            use bzip2_rs::DecoderReader;
            let mut decoder = DecoderReader::new(&data[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        }

        Compression::Zstd => {
            use ruzstd::decoding::StreamingDecoder;
            let cursor = std::io::Cursor::new(&data[..]);
            let mut decoder = StreamingDecoder::new(cursor)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        }
    }
}



/// Extract raw XHTML/XML resource bytes from EPUB archives.
fn read_epub_bytes_plaintext(data: &[u8]) -> io::Result<Vec<u8>> {
    let cursor = std::io::Cursor::new(data);
    let mut doc = epub::doc::EpubDoc::from_reader(cursor)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let resource_ids: Vec<String> = doc.resources.keys().cloned().collect();
    let mut result = Vec::new();

    for resource_id in resource_ids {
        let Some(mime) = doc.get_resource_mime(&resource_id) else {
            continue;
        };

        let is_markup = matches!(
            mime.as_str(),
            "application/xhtml+xml" | "text/html" | "application/xml" | "text/xml"
        );

        if !is_markup {
            continue;
        }

        if let Some((content, _mime)) = doc.get_resource(&resource_id) {
            if !result.is_empty() {
                result.push(b'\n');
            }
            result.extend_from_slice(&content);
        }
    }

    Ok(result)
}

/// Extract raw XML bytes from Office documents (docx, pptx, xlsx, odt)
/// These are ZIP archives containing XML files.
fn read_office_bytes(data: &[u8]) -> io::Result<Vec<u8>> {
    use zip::ZipArchive;

    let cursor = std::io::Cursor::new(data);
    let mut archive =
        ZipArchive::new(cursor).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut result = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        if entry.name().ends_with(".xml") {
            if !result.is_empty() {
                result.push(b'\n');
            }
            entry.read_to_end(&mut result)?;
        }
    }

    Ok(result)
}

/// Returns the appropriate extractor for a path's extension, or `None`
/// if the file has no extension at all. Plain `Copy` value — no boxing.
pub fn extractor_for(path: &Path) -> Option<Extractor> {
    resolve_extensions(path).map(|(compression, format)| Extractor {
        compression,
        format,
    })
}

/// Unified extractor that handles compression and format dispatch.
/// Extraction is sync only — no async variant, to avoid maintaining
/// two code paths per format.
#[derive(Debug, Clone, Copy)]
pub struct Extractor {
    compression: Compression,
    format: Format,
}

impl Extractor {
    /// Extracts searchable content from the file as raw bytes.
    pub fn extract(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        // Read file bytes
        let raw_bytes = fs::read(path)?;

        // Detect actual compression from content (overrides extension-based detection)
        let actual_compression = detect_compression(&raw_bytes);
        let compression = if actual_compression != Compression::None {
            actual_compression
        } else {
            self.compression
        };

        // Decompress if needed (in memory, never to disk)
        let data = decompress(raw_bytes, compression)?;

        // Detect actual format from content (overrides extension-based
        // detection, same policy as compression). Extension is the
        // fallback for formats without magic bytes.
        let format = detect_format(&data).unwrap_or(self.format);

        // Dispatch to format-specific reader
        match format {
            // EPUB needs special handling
            Format::Epub => read_epub_bytes_plaintext(&data),

            // PDF extraction using lopdf (in-memory, no temp files).
            // Chosen over pdf-extract for its Result-based API —
            // malformed PDFs come back as errors, not panics.
            Format::Pdf => {
                let doc = lopdf::Document::load_mem(&data)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
                let text = doc
                    .extract_text(&pages)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(text.into_bytes())
            }

            // Office formats (ZIP archives containing XML)
            Format::Office => read_office_bytes(&data),

            // Unsupported legacy formats
            Format::LegacyDoc => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                LoreError::UnsupportedFormat(
                    "doc (legacy binary format) is not supported".to_string(),
                ),
            )),

            // All other formats - return bytes as-is
            Format::Plain => Ok(data),
        }
    }
}


