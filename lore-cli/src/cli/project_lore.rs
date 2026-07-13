//! `project-lore` — wrapper around lore for project documentation search.
//! Provides sane defaults for searching project-specific documentation like
//! Rust docs, Doxygen output, and common documentation directories.

use clap::Parser;
use lore_cli::core::arguments::Cli;
use lore_cli::core::defaults::project_lore_defaults;
use lore_cli::core::search::run_search;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "project-lore",
    version,
    about = "Search project documentation (rustdoc, doxygen, docs/)"
)]
struct ProjectLoreCli {
    #[command(flatten)]
    base: Cli,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let parsed_cli = ProjectLoreCli::parse().base;
    
    // If user provided paths, use their CLI args directly
    // Otherwise, use project-lore defaults
    let cli = if !parsed_cli.paths.is_empty() 
        || !parsed_cli.exclude_path_patterns.is_empty()
        || !parsed_cli.exclude_extensions.is_empty()
        || !parsed_cli.exclude_match.is_empty()
        || parsed_cli.include_path {
        parsed_cli
    } else {
        project_lore_defaults(parsed_cli.query, parsed_cli.context.unwrap_or(0))
    };

    run_search(cli).await
}
