//! MCP server implementation for lore
//!
//! Resources and prompts modules were removed after reevaluating them
//! against agent turn/token efficiency rather than completeness — see
//! CHANGES.md. If a future resource proves itself needed (e.g. the
//! single-resource catalog described in
//! concept-walk-buffer-structured-resources.md, once there's evidence
//! agents actually need it), reintroduce this module structure with a
//! `resources.rs` at that point rather than restoring the wider set that
//! was removed here.

pub mod devhelp2;
pub mod gir;
pub mod handler;
pub mod tools;

pub use handler::LoreHandler;
