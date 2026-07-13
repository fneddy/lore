//! `lore` — exposes the complete `lore-core` public API to the
//! terminal, via `clap`. Every `SearchBuilder` method has a
//! corresponding flag; the only thing here that isn't a 1:1 mapping
//! to a builder method is `--workers`, which governs how many
//! `Search` clones this CLI spawns — concurrency is caller-owned by
//! design in `lore-core`, so a "worker count" flag necessarily lives
//! here, not in the library.
//!
//! `builtin_sources()` itself is not exposed directly (it's a
//! library-internal registry, not a CLI-shaped operation) — its
//! effect is reachable via `--no-builtins`.

use clap::Parser;
use lore_cli::core::arguments::Cli;
use lore_cli::core::search::run_search;
use std::process::ExitCode;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    run_search(cli).await
}
