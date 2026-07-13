//! Shared modules for lore CLI binaries.

pub mod core;
pub mod mcp;

// Re-export commonly used items at crate root for backwards compatibility
pub use core::arguments;
pub use core::defaults;
pub use core::search;
pub use core::update;