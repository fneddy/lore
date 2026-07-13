//! MCP server handler and tool implementations
//!
//! Design note: this file was revised to optimize specifically for agent
//! turn/token efficiency, not for maximum tool coverage. Concretely, that
//! meant removing `stat` and `list-sources` as standalone tools (folded
//! into `show` and deleted, respectively — see docstrings below for why),
//! and removing the resources/prompts capability entirely: MCP resources
//! still cost a full round-trip in most current clients (no savings over
//! a tool call), and MCP prompts are typically user-triggered rather than
//! something an autonomous agent loop invokes on itself, so neither
//! earned its complexity cost against the stated goal. See CHANGES.md for
//! the full reasoning.

use super::tools::{
    execute_query_project, execute_query_system, list_modules, outline, show, stat,
    target_doc_summary,
};
use crate::core::update::update_documentation;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::schemars::JsonSchema;
use rmcp::{schemars, tool, tool_router};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for query-system and query-project tools
///
/// These parameters control how documentation is searched and filtered.
/// All query terms are ORed together (matches any term).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryParams {
    /// Search terms to query (OR logic - matches any term)
    ///
    /// Pass as JSON array: ["async", "tokio"] finds docs containing "async" OR "tokio"
    ///
    /// Common patterns:
    /// - Single term: ["from_read"] - finds specific function
    /// - Multiple terms: ["async", "tokio", "futures"] - finds any of these (OR logic)
    /// - All terms match: Files containing ALL terms rank higher in results
    ///
    /// IMPORTANT: Each term should be a single word without spaces.
    /// If you need phrases with spaces, set allowSpacesInTerms to true.
    pub query: Vec<String>,

    /// Allow spaces in query terms (search for exact phrases)
    ///
    /// Default: false. When false, query terms containing spaces will return an error
    /// with a helpful message suggesting to split terms or enable this flag.
    ///
    /// Examples:
    /// - allowSpacesInTerms: false (default) + query: ["async tokio"] → ERROR
    /// - allowSpacesInTerms: false + query: ["async", "tokio"] → OK (searches for "async" OR "tokio")
    /// - allowSpacesInTerms: true + query: ["async tokio"] → OK (searches for exact phrase "async tokio")
    #[serde(default)]
    pub allow_spaces_in_terms: bool,

    /// Additional scan roots beyond built-in sources
    ///
    /// ⚠️ RARELY NEEDED: The tool already searches appropriate default locations.
    /// Only set this if you need to search ADDITIONAL custom paths beyond defaults.
    ///
    /// Default behavior (when omitted or empty):
    /// - query-system: Searches /usr/share/doc, man pages, system libraries
    /// - query-project: Searches target/doc, docs/, README files
    ///
    /// Example use case: Search a custom documentation directory
    /// {"paths": ["/opt/custom-docs"], ...}
    ///
    /// ⚠️ DO NOT set this to override defaults - it ADDS to them, not replaces them.
    #[serde(default)]
    pub paths: Vec<PathBuf>,

    /// Exclude paths containing these substrings
    ///
    /// Use to filter out unwanted directories or file patterns.
    /// Examples: ["target", "node_modules", ".git"]
    #[serde(default)]
    pub exclude_path_patterns: Vec<String>,

    /// Exclude files whose content contains these substrings
    ///
    /// Use to filter out files with specific text patterns.
    /// Example: ["deprecated", "internal use only"]
    #[serde(default)]
    pub exclude_match: Vec<String>,

    /// Exclude files with these extensions
    ///
    /// Use to filter out specific file types.
    /// Examples: ["test", "tmp", "bak"]
    #[serde(default)]
    pub exclude_extensions: Vec<String>,

    /// Include file paths in the searched text
    ///
    /// When true, file paths are searchable, useful for finding where code is located.
    /// Set to true when you need to understand code organization.
    #[serde(default)]
    pub include_path: bool,

    /// Number of context lines to show around each match
    ///
    /// Default: 3 lines. Increase (5-10) for complex code, decrease (0-1) for quick scanning.
    #[serde(default = "default_context")]
    pub context: usize,

    /// Maximum number of results to return per page
    ///
    /// Default: 10. Use with `page` to paginate through large result sets.
    /// If the response's `hasMore` field is true, call again with page
    /// incremented by 1 to get the next page.
    #[serde(default = "default_max_results")]
    pub max_results: usize,

    /// Page number for result pagination (0-based)
    ///
    /// Default: 0 (first page). Use to paginate through large result sets.
    /// Example: page=0 gets first maxResults, page=1 gets next maxResults, etc.
    #[serde(default)]
    pub page: usize,
}

fn default_context() -> usize {
    log::debug!("using default context value");
    3
}

fn default_max_results() -> usize {
    log::debug!("using default max_results value");
    100
}

/// Parameters for show tool
///
/// Specifies which documentation file to extract and display.
/// The path should be an exact file path obtained from query-project or query-system results.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShowParams {
    /// Path to the file to extract and show
    ///
    /// Must be a valid file path to documentation. Common examples:
    /// - Rust docs: "target/doc/crate_name/index.html"
    /// - Type docs: "target/doc/crate_name/struct.TypeName.html"
    /// - Man pages: "/usr/share/man/man2/socket.2.gz"
    /// - Local docs: "docs/api/reference.html"
    ///
    /// The tool automatically handles:
    /// - HTML rendering (via html2text for readable formatting)
    /// - EPUB extraction and formatting
    /// - PDF text extraction
    /// - Compressed man pages (gzip)
    /// - Plain text files
    ///
    /// Tip: for HTML files, try the `outline` tool first — it returns just
    /// headings and code signatures, which is usually enough to decide
    /// whether the full `show` extraction (which can be large) is worth it.
    pub path: PathBuf,

    /// Starting byte offset for content extraction
    ///
    /// Content extraction begins at this byte position.
    /// Useful for paginating through large documentation files.
    /// Use 0 to start from the beginning. Ignored when metadataOnly is true.
    #[serde(default)]
    pub start: usize,

    /// Maximum number of bytes to extract
    ///
    /// Limits the extracted content to this many bytes.
    /// If the response's `hasMore` field is true, call again with
    /// `start` advanced by `returnedBytes` to continue reading.
    /// Ignored when metadataOnly is true.
    #[serde(default)]
    pub length: usize,

    /// If true, skip extraction entirely and return only filesystem
    /// metadata (size, extension, last-modified time) for `path`.
    ///
    /// Use this to decide whether a full extraction is worth the cost
    /// (e.g. skip a 50MB PDF) before spending a real `show` call on it —
    /// this replaces what used to be a separate `stat` tool; folding it
    /// into `show` means there's one tool to reach for when you need
    /// anything about a specific file, not two to choose between.
    #[serde(default)]
    pub metadata_only: bool,
}

/// Parameters for outline tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutlineParams {
    /// Path to an HTML/HTM documentation file (e.g. a rustdoc page)
    pub path: PathBuf,
}

/// Parameters for list-modules tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListModulesParams {
    /// Root directory to scan for per-crate rustdoc output.
    /// Defaults to "target/doc" if omitted.
    #[serde(default)]
    pub root: Option<PathBuf>,
}

// ============================================================================
// Response Envelope Types
// ============================================================================
//
// These intentionally derive only Serialize: JsonSchema is only needed on
// tool *input* params (rmcp uses it to build the input schema), and
// QueryResult wraps SearchResult from core::search, which itself only
// derives Serialize — adding JsonSchema/Deserialize here would fail to
// compile without also adding it there, and neither is needed for an
// output-only type.

/// Response envelope for query-system / query-project
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub matches: Vec<crate::core::search::SearchResult>,
    /// Total number of matches found (across all pages)
    pub total_count: usize,
    /// True if more results exist beyond the current page.
    /// When true, call again with offset increased by maxResults.
    pub has_more: bool,
}

/// Response envelope for show. When `metadataOnly` was requested, `content`
/// is empty, `hasMore` is false, and `sizeBytes`/`modifiedUnix`/`isDir` are
/// populated; otherwise those three are `None` and the extraction fields
/// are populated as before.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowResult {
    pub content: String,
    pub start: usize,
    pub returned_bytes: usize,
    pub total_bytes: usize,
    pub has_more: bool,
    pub size_bytes: Option<u64>,
    pub modified_unix: Option<u64>,
    pub is_dir: Option<bool>,
}

// ============================================================================
// Handler Implementation
// ============================================================================

/// Handler for lore MCP tools
#[derive(Clone)]
pub struct LoreHandler {
    pub(crate) tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

impl LoreHandler {
    pub fn new() -> Self {
        log::info!("constructing LoreHandler");
        let handler = Self {
            tool_router: Self::tool_router(),
        };
        log::info!(
            "LoreHandler constructed with tools: {:?}",
            handler
                .tool_router
                .list_all()
                .iter()
                .map(|t| t.name.as_ref())
                .collect::<Vec<_>>()
        );
        handler
    }

    /// Get access to the tool router for logging/debugging
    pub fn get_tool_router(&self) -> &rmcp::handler::server::router::tool::ToolRouter<Self> {
        &self.tool_router
    }
}

#[tool_router]
impl LoreHandler {
    /// **[PRIMARY for system wide library/API docs]** Query system documentation (man pages, /usr/share/doc, etc.)
    ///
    /// Search through system-level documentation including:
    /// - Man pages
    /// - /usr/share/doc directories
    /// - System library documentation
    /// - Standard library references
    ///
    /// USAGE EXAMPLES:
    /// ```json
    /// // Single term search
    /// {"query": ["socket"], "maxResults": 10}
    ///
    /// // Multiple terms (OR logic - finds docs with ANY term)
    /// {"query": ["gtk", "menu", "menuitem"], "maxResults": 20}
    ///
    /// // With pagination
    /// {"query": ["async"], "maxResults": 100, "page": 0}
    /// {"query": ["async"], "maxResults": 100, "page": 1}
    /// ```
    ///
    /// IMPORTANT: query must be a JSON array of strings, NOT a single string with spaces.
    /// ❌ WRONG: {"query": "gtk menu menuitem"}
    /// ✅ CORRECT: {"query": ["gtk", "menu", "menuitem"]}
    ///
    /// Returns: `{ matches: [...], totalCount: N, hasMore: bool }`
    /// - Results sorted by relevance (files matching more terms rank higher)
    /// - Use page parameter to access additional results when hasMore is true
    ///
    /// Best practices:
    /// - Start with maxResults=10-20, increase if needed
    /// - Use page parameter for pagination (page=0, 1, 2, ...)
    /// - Filter with excludeExtensions or excludePathPatterns to reduce noise
    #[tool(name = "query-system")]
    async fn query_system(&self, params: Parameters<QueryParams>) -> String {
        log::info!("query-system called with query: {:?}", params.0.query);
        match execute_query_system(params.0).await {
            Ok((matches, total_count, has_more)) => {
                log::info!(
                    "query-system returned {} results (total={}, has_more={})",
                    matches.len(),
                    total_count,
                    has_more
                );
                let result = QueryResult { 
                    matches, 
                    total_count,
                    has_more 
                };
                serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| r#"{"matches":[],"totalCount":0,"hasMore":false}"#.to_string())
            }
            Err(error_msg) => {
                log::error!("query-system validation error: {}", error_msg);
                error_msg
            }
        }
    }

    /// **[PRIMARY for project library/API docs]** Query project documentation (rustdoc, doxygen, docs/)
    ///
    /// Search through project-specific documentation including:
    /// - Rustdoc (generated Rust documentation)
    /// - Doxygen documentation
    /// - Local docs/ directories
    /// - Project README files
    ///
    /// USAGE EXAMPLES:
    /// ```json
    /// // Single term search
    /// {"query": ["async"], "maxResults": 10}
    ///
    /// // Multiple terms (OR logic - finds docs with ANY term)
    /// {"query": ["async", "tokio", "futures"], "maxResults": 20}
    ///
    /// // With pagination
    /// {"query": ["Search"], "maxResults": 100, "page": 0}
    /// {"query": ["Search"], "maxResults": 100, "page": 1}
    ///
    /// // With filters
    /// {"query": ["async"], "excludePathPatterns": ["test"], "maxResults": 20}
    /// ```
    ///
    /// IMPORTANT: query must be a JSON array of strings, NOT a single string with spaces.
    /// ❌ WRONG: {"query": "async tokio"}
    /// ✅ CORRECT: {"query": ["async", "tokio"]}
    ///
    /// Returns: `{ matches: [...], totalCount: N, hasMore: bool }`
    /// - Results sorted by relevance (files matching more terms rank higher)
    /// - Use page parameter to access additional results when hasMore is true
    ///
    /// Workflow tips:
    /// - Use `list-modules` first to see what crates/modules exist
    /// - Start broad to find modules, then narrow to specific functions
    /// - Use `outline` on an HTML hit before `show` if you just need structure
    ///   (headings, signatures) rather than full prose
    /// - Combine with excludeExtensions to filter out test files
    /// - Set includePath=true to understand code location
    #[tool(name = "query-project")]
    async fn query_project(&self, params: Parameters<QueryParams>) -> String {
        log::info!("query-project called with query: {:?}", params.0.query);
        match execute_query_project(params.0).await {
            Ok((matches, total_count, has_more)) => {
                log::info!(
                    "query-project returned {} results (total={}, has_more={})",
                    matches.len(),
                    total_count,
                    has_more
                );
                let result = QueryResult { 
                    matches, 
                    total_count,
                    has_more 
                };
                serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| r#"{"matches":[],"totalCount":0,"hasMore":false}"#.to_string())
            }
            Err(error_msg) => {
                log::error!("query-project validation error: {}", error_msg);
                error_msg
            }
        }
    }

    /// Update project documentation (run cargo doc, doxygen, etc.)
    ///
    /// Regenerate project documentation by running:
    /// - cargo doc for Rust projects
    /// - doxygen for C/C++ projects
    /// - Other configured documentation generators
    ///
    /// Use this tool after making code changes to ensure documentation is current.
    /// Run before querying to get the latest API documentation and code examples.
    ///
    /// The response includes a one-line index snapshot (crate count under
    /// target/doc, last-modified time) appended after the generator's own
    /// status message, so you can tell in the same call whether the refresh
    /// actually produced anything new — no separate lookup needed.
    ///
    /// Workflow:
    /// 1. Make code changes
    /// 2. Run update to regenerate docs
    /// 3. Use query-project to verify documentation is correct
    #[tool]
    async fn update(&self) -> String {
        log::info!("update called");
        let result = update_documentation();
        let snapshot = target_doc_summary();
        log::info!("update completed");
        format!("{result}\n\n{snapshot}")
    }

    /// Show extracted content from a file, or just its metadata
    ///
    /// Display the full extracted and formatted content from a specific documentation file.
    /// Supports multiple formats:
    /// - HTML/HTM files (rendered as formatted text via html2text)
    /// - EPUB files (extracted and formatted)
    /// - PDF files (text extraction)
    /// - Man pages (decompressed and formatted)
    /// - Plain text files
    ///
    /// Use this tool after query-project or query-system to view complete documentation.
    /// The path parameter should be an exact file path from query results.
    ///
    /// Set `metadataOnly=true` to skip extraction entirely and get just size,
    /// extension, last-modified time, and whether the path is a directory —
    /// useful for deciding if a full extraction is worth the cost (e.g. skip
    /// a 50MB PDF) before paying for it.
    ///
    /// Returns a JSON object: `{ content, start, returnedBytes, totalBytes,
    /// hasMore, sizeBytes, modifiedUnix, isDir }`. `sizeBytes`/`modifiedUnix`/
    /// `isDir` are only populated when metadataOnly was set; otherwise check
    /// `hasMore` — if true, the file wasn't fully returned, call again with
    /// `start` advanced by `returnedBytes` to continue reading.
    ///
    /// Workflow:
    /// 1. Use query-project or query-system to find relevant documentation
    /// 2. Identify the specific file path from results
    /// 3. For HTML files, consider `outline` first if you only need structure
    /// 4. Use show with that path to view full content, paginating via hasMore
    ///
    /// Example paths:
    /// - target/doc/crate_name/index.html (Rust crate overview)
    /// - target/doc/crate_name/struct.TypeName.html (Type documentation)
    /// - /usr/share/man/man2/socket.2.gz (System man page)
    #[tool]
    async fn show(&self, params: Parameters<ShowParams>) -> String {
        log::info!(
            "show called for path: {:?}, start: {:?}, length: {:?}, metadata_only: {:?}",
            params.0.path,
            params.0.start,
            params.0.length,
            params.0.metadata_only
        );

        if params.0.metadata_only {
            let meta = stat(params.0.path);
            let result = ShowResult {
                content: String::new(),
                start: 0,
                returned_bytes: 0,
                total_bytes: meta.size_bytes as usize,
                has_more: false,
                size_bytes: Some(meta.size_bytes),
                modified_unix: meta.modified_unix,
                is_dir: Some(meta.is_dir),
            };
            log::info!("show completed (metadata_only)");
            return serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string());
        }

        let start = params.0.start;
        let extracted = show(params.0.path, start, params.0.length);
        let returned_bytes = extracted.content.len();
        let has_more = start.saturating_add(returned_bytes) < extracted.total_bytes;
        let result = ShowResult {
            content: extracted.content,
            start,
            returned_bytes,
            total_bytes: extracted.total_bytes,
            has_more,
            size_bytes: None,
            modified_unix: None,
            is_dir: None,
        };
        log::info!("show completed (has_more={})", has_more);
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
    }

    /// Extract just headings and code signatures from an HTML documentation page
    ///
    /// Returns a lightweight structural outline (h1/h2/h3 headings plus inline
    /// `<code>` spans) instead of the full rendered prose that `show` would
    /// return. Intended for large rustdoc/doxygen pages where you want to know
    /// "what's on this page" before paying the cost of a full extraction.
    ///
    /// Rustdoc boilerplate sections that appear identically on every page
    /// ("Auto Trait Implementations", "Blanket Implementations") are
    /// filtered out, including everything under them.
    ///
    /// Only supports .html/.htm files; other formats return an error entry.
    #[tool]
    async fn outline(&self, params: Parameters<OutlineParams>) -> String {
        log::info!("outline called for path: {:?}", params.0.path);
        let entries = outline(params.0.path);
        serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string())
    }

    /// List crates/modules with generated rustdoc, without searching their content
    ///
    /// Scans a rustdoc output root (default "target/doc") for per-crate
    /// directories containing an index.html, and returns their names and index
    /// paths. Use this to answer "what's in this codebase" before you know
    /// which crate name to search for with query-project.
    #[tool(name = "list-modules")]
    async fn list_modules_tool(&self, params: Parameters<ListModulesParams>) -> String {
        log::info!("list-modules called with root: {:?}", params.0.root);
        let modules = list_modules(params.0.root);
        serde_json::to_string_pretty(&modules).unwrap_or_else(|_| "[]".to_string())
    }
}

#[rmcp::tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for LoreHandler {
    fn get_info(&self) -> ServerInfo {
        log::info!("building MCP server info");
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Lore MCP server provides documentation search, inspection, and management tools:\n\
            \n\
            Search:\n\
            • query-project: Search project documentation (rustdoc, doxygen, docs/)\n\
            • query-system: Search system documentation (man pages, /usr/share/doc)\n\
            Both return { matches, truncated } sorted by relevance (unique terms matched,\n\
            then total match count). Narrow the query rather than raising maxResults when\n\
            truncated is true.\n\
            \n\
            Orient (cheaper than a broad search):\n\
            • list-modules: See which crates have rustdoc without searching them\n\
            • outline: Get just headings/signatures from an HTML doc page\n\
            \n\
            Read:\n\
            • show: Display extracted content from a documentation file, paginated\n\
            via start/hasMore. Set metadataOnly=true to get just size/mtime\n\
            without paying for extraction.\n\
            \n\
            Maintain:\n\
            • update: Regenerate project documentation (cargo doc, doxygen) and\n\
            report an index snapshot (crate count, last-modified) in the same\n\
            response.\n\
            \n\
            Recommended flow: list-modules to scope → query-project/query-system\n\
            to search (check truncated before raising maxResults) → outline or\n\
            show to read (check hasMore before assuming you're done).\n\
            ",
        )
    }
}
