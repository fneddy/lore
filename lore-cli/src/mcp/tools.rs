//! Query execution and file content extraction tools

use super::devhelp2;
use super::gir;
use super::handler::QueryParams;
use crate::core::defaults::{project_lore_defaults, system_lore_defaults};
use crate::core::search::{run_search_stream, SearchMessage, SearchResult};
use serde::Serialize;
use std::io::Read;
use std::path::PathBuf;

// ============================================================================
// Query Execution
// ============================================================================

/// Execute a system documentation query.
/// Returns (matches, total_count, has_more) for pagination support.
pub async fn execute_query_system(params: QueryParams) -> Result<(Vec<SearchResult>, usize, bool), String> {
    log::info!("execute_query_system start");
    
    // Validate query terms for spaces
    if !params.allow_spaces_in_terms {
        for term in &params.query {
            if term.contains(' ') {
                let error_msg = format!(
                    "Query term '{}' contains spaces. Either split into separate terms or set allowSpacesInTerms=true to search for exact phrases.",
                    term
                );
                log::warn!("{}", error_msg);
                return Err(error_msg);
            }
        }
    }
    
    let max_results = params.max_results;
    let page = params.page;
    let offset = page.saturating_mul(max_results);
    let context = params.context;
    let query = params.query.clone();
    let mut cli = system_lore_defaults(query, context);

    configure_cli(&mut cli, params);

    log::debug!(
        "execute_query_system configured paths={}, workers={}, include_path={}, max_results={}, page={}, offset={}",
        cli.paths.len(),
        cli.workers,
        cli.include_path,
        max_results,
        page,
        offset
    );

    let (results, total_count, has_more) = execute_search(&cli, max_results, offset).await;
    log::info!(
        "execute_query_system done with {} results (total={}, has_more={})",
        results.len(),
        total_count,
        has_more
    );
    Ok((results, total_count, has_more))
}

/// Execute a project documentation query.
/// Returns (matches, total_count, has_more) for pagination support.
pub async fn execute_query_project(params: QueryParams) -> Result<(Vec<SearchResult>, usize, bool), String> {
    log::info!("execute_query_project start");
    
    // Validate query terms for spaces
    if !params.allow_spaces_in_terms {
        for term in &params.query {
            if term.contains(' ') {
                let error_msg = format!(
                    "Query term '{}' contains spaces. Either split into separate terms or set allowSpacesInTerms=true to search for exact phrases.",
                    term
                );
                log::warn!("{}", error_msg);
                return Err(error_msg);
            }
        }
    }
    
    let max_results = params.max_results;
    let page = params.page;
    let offset = page.saturating_mul(max_results);
    let context = params.context;
    let query = params.query.clone();
    let mut cli = project_lore_defaults(query, context);

    configure_cli(&mut cli, params);

    log::debug!(
        "execute_query_project configured paths={}, workers={}, include_path={}, max_results={}, page={}, offset={}",
        cli.paths.len(),
        cli.workers,
        cli.include_path,
        max_results,
        page,
        offset
    );

    let (results, total_count, has_more) = execute_search(&cli, max_results, offset).await;
    log::info!(
        "execute_query_project done with {} results (total={}, has_more={})",
        results.len(),
        total_count,
        has_more
    );
    Ok((results, total_count, has_more))
}

/// Configure CLI arguments from query parameters
fn configure_cli(cli: &mut crate::core::arguments::Cli, params: QueryParams) {
    if !params.paths.is_empty() {
        cli.paths = params.paths;
    }
    if !params.exclude_path_patterns.is_empty() {
        cli.exclude_path_patterns = params.exclude_path_patterns;
    }
    if !params.exclude_match.is_empty() {
        cli.exclude_match = params.exclude_match;
    }
    if !params.exclude_extensions.is_empty() {
        cli.exclude_extensions = params.exclude_extensions;
    }
    if params.include_path {
        cli.include_path = true;
    }

    // Use multiple workers like the CLI does
    cli.workers = num_cpus::get().saturating_sub(2).max(1);
}

/// Execute search and collect results with pagination support.
///
/// Collects ALL matching results, sorts by relevance, then returns a paginated slice.
/// This allows users to access complete result sets through multiple queries with
/// different offset values.
///
/// Results are sorted by relevance:
/// 1. Number of unique query terms matched (more terms = higher priority)
/// 2. Total match count (more matches = higher priority)
///
/// Returns: (paginated_results, total_count, has_more)
async fn execute_search(
    cli: &crate::core::arguments::Cli,
    max_results: usize,
    offset: usize,
) -> (Vec<SearchResult>, usize, bool) {
    log::info!(
        "execute_search start: query_terms={}, paths={}, max_results={}, offset={}, workers={}",
        cli.query.len(),
        cli.paths.len(),
        max_results,
        offset,
        cli.workers
    );
    let mut results = Vec::new();
    let mut rx = run_search_stream(cli).await;

    // Collect all results (we need to sort them and support pagination)
    while let Some(msg) = rx.recv().await {
        match msg {
            SearchMessage::Match(mut result) => {
                if let Ok(metadata) = std::fs::metadata(&result.path) {
                    result.filesize = metadata.len();
                }
                log::debug!("execute_search match: {}", result.path.display());
                results.push(result);
            }
            SearchMessage::Error(err) => {
                log::warn!(
                    "execute_search skipped error on {}: {}",
                    err.path.display(),
                    err.error
                );
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

    let total_count = results.len();
    
    // Apply pagination: slice results based on offset and max_results
    let end = offset.saturating_add(max_results).min(total_count);
    let paginated = if offset < total_count {
        results[offset..end].to_vec()
    } else {
        Vec::new()
    };
    
    let has_more = end < total_count;

    log::info!(
        "execute_search done: total={}, returned={} (offset={}, has_more={})",
        total_count,
        paginated.len(),
        offset,
        has_more
    );
    (paginated, total_count, has_more)
}

// ============================================================================
// File Content Extraction
// ============================================================================

/// Result of a `show` extraction: the (possibly sliced) content plus the
/// full extracted length, so callers can detect truncation/pagination needs.
pub struct ExtractedShow {
    pub content: String,
    pub total_bytes: usize,
}

/// Show extracted content from a documentation file
pub fn show(path: PathBuf, start: usize, length: usize) -> ExtractedShow {
    log::info!(
        "show start for {} (start={}, length={})",
        path.display(),
        start,
        length
    );

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    let result = match ext.as_str() {
        "html" | "htm" | "page" => extract_html(&path),
        "epub" => extract_epub(&path),
        "docx" | "pptx" | "xlsx" | "odt" => extract_office(&path),
        "devhelp2" => extract_devhelp2(&path),
        "gir" => extract_gir(&path),
        "gz" => {
            // Check if it's a devhelp2.gz or gir.gz file
            if path.to_string_lossy().ends_with(".devhelp2.gz") {
                extract_devhelp2(&path)
            } else if path.to_string_lossy().ends_with(".gir.gz") {
                extract_gir(&path)
            } else {
                extract_man(&path)
            }
        }
        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => extract_man(&path),
        _ => extract_plain(&path),
    };

    // Apply start/length slicing
    let original_len = result.len();
    let end = start.saturating_add(length).min(result.len());
    let sliced = result.get(start..end).unwrap_or(&result).to_vec();

    log::info!(
        "show done for {} (extracted {} bytes, returned {} bytes)",
        path.display(),
        original_len,
        sliced.len()
    );

    ExtractedShow {
        content: String::from_utf8_lossy(&sliced).into_owned(),
        total_bytes: original_len,
    }
}

/// Extract HTML content with html2text formatting
fn extract_html(path: &PathBuf) -> Vec<u8> {
    match std::fs::read(path) {
        Ok(data) => match html2text::from_read(data.as_slice(), 120) {
            Ok(content) => {
                log::info!(
                    "extract_html extracted {} bytes from {}",
                    content.len(),
                    path.display()
                );
                content.into_bytes()
            }
            Err(e) => {
                log::warn!("extract_html failed for {}: {}", path.display(), e);
                format!("Error extracting HTML: {}", e).into_bytes()
            }
        },
        Err(e) => {
            log::warn!("extract_html failed to read {}: {}", path.display(), e);
            format!("Error reading file: {}", e).into_bytes()
        }
    }
}

/// Extract EPUB content with html2text formatting
fn extract_epub(path: &PathBuf) -> Vec<u8> {
    match std::fs::read(path) {
        Ok(data) => {
            let cursor = std::io::Cursor::new(data);
            match epub::doc::EpubDoc::from_reader(cursor) {
                Ok(mut doc) => {
                    let text = (0..doc.spine.len())
                        .filter_map(|i| {
                            doc.set_current_chapter(i);
                            doc.get_current_str().and_then(|(content, _mime)| {
                                html2text::from_read(content.as_bytes(), 120).ok()
                            })
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    log::info!(
                        "extract_epub extracted {} bytes from {}",
                        text.len(),
                        path.display()
                    );
                    text.into_bytes()
                }
                Err(e) => {
                    log::warn!("extract_epub failed for {}: {}", path.display(), e);
                    format!("Error extracting EPUB: {}", e).into_bytes()
                }
            }
        }
        Err(e) => {
            log::warn!("extract_epub failed to read {}: {}", path.display(), e);
            format!("Error reading file: {}", e).into_bytes()
        }
    }
}

/// Extract Office document content (docx, pptx, xlsx, odt)
fn extract_office(path: &PathBuf) -> Vec<u8> {
    match std::fs::read(path) {
        Ok(data) => {
            use zip::ZipArchive;

            let cursor = std::io::Cursor::new(data);
            match ZipArchive::new(cursor) {
                Ok(mut archive) => {
                    let mut result = Vec::new();

                    for i in 0..archive.len() {
                        if let Ok(mut entry) = archive.by_index(i) {
                            if entry.name().ends_with(".xml") {
                                if !result.is_empty() {
                                    result.push(b'\n');
                                }
                                let _ = entry.read_to_end(&mut result);
                            }
                        }
                    }

                    log::info!(
                        "extract_office extracted {} bytes from {}",
                        result.len(),
                        path.display()
                    );
                    result
                }
                Err(e) => {
                    log::warn!(
                        "extract_office failed to open archive {}: {}",
                        path.display(),
                        e
                    );
                    format!("Error extracting Office document: {}", e).into_bytes()
                }
            }
        }
        Err(e) => {
            log::warn!("extract_office failed to read {}: {}", path.display(), e);
            format!("Error reading file: {}", e).into_bytes()
        }
    }
}

/// Extract and render man page content
fn extract_man(path: &PathBuf) -> Vec<u8> {
    // First get the decompressed content using lore_core extractor
    match lore_core::extractor_for(path) {
        Some(extractor) => match extractor.extract(path) {
            Ok(content) => {
                // Try to render using groff
                match std::process::Command::new("groff")
                    .args(["-man", "-Tutf8"])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    Ok(mut child) => {
                        // Write the man page content to groff's stdin
                        if let Some(mut stdin) = child.stdin.take() {
                            use std::io::Write;
                            let _ = stdin.write_all(&content);
                        }

                        // Read the rendered output
                        match child.wait_with_output() {
                            Ok(output) if output.status.success() => {
                                log::info!(
                                    "extract_man rendered {} bytes from {}",
                                    output.stdout.len(),
                                    path.display()
                                );
                                output.stdout
                            }
                            _ => {
                                log::warn!(
                                    "extract_man groff failed for {}, returning raw content",
                                    path.display()
                                );
                                content
                            }
                        }
                    }
                    Err(_) => {
                        log::warn!(
                            "extract_man groff not available for {}, returning raw content",
                            path.display()
                        );
                        content
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "extract_man extraction failed for {}: {}",
                    path.display(),
                    e
                );
                format!("Error extracting man page: {}", e).into_bytes()
            }
        },
        None => {
            log::warn!("extract_man no extractor for {}", path.display());
            format!("No extractor available for file: {}", path.display()).into_bytes()
        }
    }
}

/// Extract plain text content using lore_core extractors
fn extract_plain(path: &PathBuf) -> Vec<u8> {
    match lore_core::extractor_for(path) {
        Some(extractor) => match extractor.extract(path) {
            Ok(content) => {
                log::info!(
                    "extract_plain extracted {} bytes from {}",
                    content.len(),
                    path.display()
                );
                content
            }
            Err(e) => {
                log::warn!("extract_plain failed for {}: {}", path.display(), e);
                format!("Error extracting file: {}", e).into_bytes()
            }
        },
        None => {
            log::warn!("extract_plain no extractor for {}", path.display());
            format!("No extractor available for file: {}", path.display()).into_bytes()
        }
    }
}

/// Extract and format devhelp2 documentation
fn extract_devhelp2(path: &PathBuf) -> Vec<u8> {
    // Use lore-core to read and decompress the file
    match lore_core::extractor_for(path) {
        Some(extractor) => match extractor.extract(path) {
            Ok(data) => match devhelp2::parse_devhelp(&data) {
                Ok(book) => {
                    let mut output = String::new();

                    // Get the base directory for resolving relative links
                    let base_dir = path
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    // Add book header
                    output.push_str(&format!("# {}\n\n", book.title));
                    output.push_str(&format!("**Name:** {}\n", book.name));
                    output.push_str(&format!("**Version:** {}\n", book.version));
                    output.push_str(&format!("**Language:** {}\n", book.language));
                    output.push_str(&format!("**Link:** {}/{}\n\n", base_dir, book.link));

                    // Add chapters section
                    if !book.chapters().is_empty() {
                        output.push_str("## Chapters\n\n");
                        for chapter in book.chapters() {
                            format_chapter(&mut output, chapter, 0, &base_dir);
                        }
                        output.push_str("\n");
                    }

                    // Add keywords/functions section
                    if !book.keywords().is_empty() {
                        output.push_str("## Keywords/Functions\n\n");
                        for keyword in book.keywords() {
                            output.push_str(&format!(
                                "- **{}** ({}): {}/{}\n",
                                keyword.name, keyword.kind, base_dir, keyword.link
                            ));
                        }
                    }

                    log::info!(
                        "extract_devhelp2 extracted {} bytes from {}",
                        output.len(),
                        path.display()
                    );
                    output.into_bytes()
                }
                Err(e) => {
                    log::warn!("extract_devhelp2 parse failed for {}: {}", path.display(), e);
                    format!("Error parsing devhelp2 file: {}", e).into_bytes()
                }
            },
            Err(e) => {
                log::warn!(
                    "extract_devhelp2 extraction failed for {}: {}",
                    path.display(),
                    e
                );
                format!("Error extracting devhelp2 file: {}", e).into_bytes()
            }
        },
        None => {
            log::warn!("extract_devhelp2 no extractor for {}", path.display());
            format!("No extractor available for file: {}", path.display()).into_bytes()
        }
    }
}

/// Helper function to format chapters recursively
fn format_chapter(output: &mut String, chapter: &devhelp2::Chapter, depth: usize, base_dir: &str) {
    let indent = "  ".repeat(depth);
    output.push_str(&format!(
        "{}- **{}**: {}/{}\n",
        indent, chapter.name, base_dir, chapter.link
    ));

    for child in &chapter.children {
        format_chapter(output, child, depth + 1, base_dir);
    }
}

/// Extract and format GIR (GObject Introspection) documentation
fn extract_gir(path: &PathBuf) -> Vec<u8> {
    // Use lore-core to read and decompress the file
    match lore_core::extractor_for(path) {
        Some(extractor) => match extractor.extract(path) {
            Ok(data) => match gir::render_gir(&data) {
                Ok(rendered) => {
                    log::info!(
                        "extract_gir extracted {} bytes from {}",
                        rendered.len(),
                        path.display()
                    );
                    rendered
                }
                Err(e) => {
                    log::warn!("extract_gir render failed for {}: {}", path.display(), e);
                    format!("Error rendering GIR file: {}", e).into_bytes()
                }
            },
            Err(e) => {
                log::warn!(
                    "extract_gir extraction failed for {}: {}",
                    path.display(),
                    e
                );
                format!("Error extracting GIR file: {}", e).into_bytes()
            }
        },
        None => {
            log::warn!("extract_gir no extractor for {}", path.display());
            format!("No extractor available for file: {}", path.display()).into_bytes()
        }
    }
}

// ============================================================================
// Supporting helpers for show(metadataOnly) and update()
//
// `list-sources` and standalone `stat` were removed as dedicated MCP
// tools (see handler.rs's module doc comment for why) — `stat` survives
// here as a plain function, now called only from inside the `show`
// handler when metadataOnly is requested, rather than exposed on its own.
// ============================================================================

/// Filesystem metadata for a path, without extracting its content.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatResult {
    pub path: String,
    pub exists: bool,
    pub size_bytes: u64,
    pub extension: Option<String>,
    pub modified_unix: Option<u64>,
    pub is_dir: bool,
}

/// Stat a path without extracting/parsing it. Used internally by the
/// `show` tool's metadataOnly mode.
pub fn stat(path: PathBuf) -> StatResult {
    match std::fs::metadata(&path) {
        Ok(meta) => StatResult {
            path: path.display().to_string(),
            exists: true,
            size_bytes: meta.len(),
            extension: path.extension().and_then(|e| e.to_str()).map(String::from),
            modified_unix: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            is_dir: meta.is_dir(),
        },
        Err(e) => {
            log::debug!(
                "stat: {} does not exist or is inaccessible: {}",
                path.display(),
                e
            );
            StatResult {
                path: path.display().to_string(),
                exists: false,
                size_bytes: 0,
                extension: None,
                modified_unix: None,
                is_dir: false,
            }
        }
    }
}

/// One heading/signature line extracted from an HTML doc page.
#[derive(Debug, Clone, Serialize)]
pub struct OutlineEntry {
    /// "h1" | "h2" | "h3" | "code" | "error"
    pub level: String,
    pub text: String,
}

/// Extract h1-h3 headings and inline `<code>` spans from an HTML file, in
/// document order. Deliberately dependency-free: a hand-rolled scan rather
/// than a full HTML parser. This is fine for well-formed rustdoc/doxygen
/// output but is not robust against malformed or deeply nested markup — if
/// that becomes a problem, swap this for the `scraper` crate.
pub fn outline(path: PathBuf) -> Vec<OutlineEntry> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if !matches!(ext.as_str(), "html" | "htm") {
        return vec![OutlineEntry {
            level: "error".to_string(),
            text: format!("outline only supports .html/.htm files, got .{}", ext),
        }];
    }

    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(e) => {
            return vec![OutlineEntry {
                level: "error".to_string(),
                text: format!("failed to read {}: {}", path.display(), e),
            }]
        }
    };

    strip_boilerplate_sections(extract_headings(&data))
}

/// Rustdoc sections that carry no page-specific information — every
/// type gets the same auto-trait and blanket-impl listings, so they're
/// pure noise in an outline.
fn is_boilerplate_section(text: &str) -> bool {
    text.starts_with("Auto Trait Implementations")
        || text.starts_with("Blanket Implementations")
}

/// Drop boilerplate section headings and everything under them: from a
/// matching h1/h2 up to (exclusive) the next h1/h2, so the h3 impls and
/// code spans inside those sections go too.
fn strip_boilerplate_sections(entries: Vec<OutlineEntry>) -> Vec<OutlineEntry> {
    let mut out = Vec::with_capacity(entries.len());
    let mut skipping = false;

    for entry in entries {
        if matches!(entry.level.as_str(), "h1" | "h2") {
            skipping = is_boilerplate_section(&entry.text);
        }
        if !skipping {
            out.push(entry);
        }
    }

    out
}

fn extract_headings(html: &str) -> Vec<OutlineEntry> {
    const TAGS: [&str; 4] = ["h1", "h2", "h3", "code"];
    let mut out = vec![];
    let mut i = 0usize;

    while let Some(rel) = html[i..].find('<') {
        let pos = i + rel;
        let mut matched = false;

        for tag in TAGS {
            let open = format!("<{}", tag);
            if html[pos..].starts_with(&open) {
                if let Some(gt_rel) = html[pos..].find('>') {
                    let content_start = pos + gt_rel + 1;
                    let close_tag = format!("</{}>", tag);
                    if let Some(close_rel) = html[content_start..].find(&close_tag) {
                        let raw = &html[content_start..content_start + close_rel];
                        let clean = html2text::from_read(raw.as_bytes(), 200)
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        if !clean.is_empty() {
                            out.push(OutlineEntry {
                                level: tag.to_string(),
                                text: clean,
                            });
                        }
                        i = content_start + close_rel + close_tag.len();
                        matched = true;
                        break;
                    }
                }
            }
        }

        if !matched {
            i = pos + 1;
        }
    }

    out
}

/// One crate/module discovered under a rustdoc output root.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleEntry {
    pub crate_name: String,
    pub path: String,
}

/// List crate directories with rustdoc output (each containing an
/// index.html) under `root`, defaulting to "target/doc".
pub fn list_modules(root: Option<PathBuf>) -> Vec<ModuleEntry> {
    let root = root.unwrap_or_else(|| PathBuf::from("target/doc"));
    let mut out = vec![];

    let entries = match std::fs::read_dir(&root) {
        Ok(e) => e,
        Err(e) => {
            log::debug!("list_modules: cannot read {}: {}", root.display(), e);
            return out;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let index = path.join("index.html");
        if index.exists() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            out.push(ModuleEntry {
                crate_name: name,
                path: index.display().to_string(),
            });
        }
    }

    out.sort_by(|a, b| a.crate_name.cmp(&b.crate_name));
    out
}

/// One-line summary of target/doc's current state: whether it exists, how
/// many crates have generated docs, and when it was last modified.
///
/// This used to be its own `lore://index/stats` resource; it's now called
/// directly from the `update` tool and appended to that tool's response
/// instead, since checking freshness only matters right after a refresh —
/// a separate always-available resource for it wasn't earning its keep.
pub fn target_doc_summary() -> String {
    let root = std::path::Path::new("target/doc");
    if !root.exists() {
        return "Index snapshot: no target/doc directory found.".to_string();
    }

    let crate_count = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir() && e.path().join("index.html").exists())
                .count()
        })
        .unwrap_or(0);

    let modified_unix = std::fs::metadata(root)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    match modified_unix {
        Some(ts) => format!(
            "Index snapshot: {} documented crates under target/doc (last modified unix={})",
            crate_count, ts
        ),
        None => format!(
            "Index snapshot: {} documented crates under target/doc",
            crate_count
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_strips_rustdoc_boilerplate_sections() {
        let html = r#"
            <h1>Struct SearchBuilder</h1>
            <h2>Implementations</h2>
            <h3>impl SearchBuilder</h3>
            <code>pub fn build(self) -> Search</code>
            <h2>Trait Implementations</h2>
            <h3>impl Default for SearchBuilder</h3>
            <h2>Auto Trait Implementations</h2>
            <h3>impl Send for SearchBuilder</h3>
            <code>Send</code>
            <h2>Blanket Implementations</h2>
            <h3>impl&lt;T&gt; Any for T</h3>
            <code>TypeId</code>
        "#;

        let entries = strip_boilerplate_sections(extract_headings(html));
        let texts: Vec<&str> = entries.iter().map(|e| e.text.as_str()).collect();

        // Real sections survive, in order.
        assert_eq!(
            texts,
            [
                "Struct SearchBuilder",
                "Implementations",
                "impl SearchBuilder",
                "pub fn build(self) -> Search",
                "Trait Implementations",
                "impl Default for SearchBuilder",
            ]
        );
    }
}
