//! `lore-core` — minimal documentation search library.
//!
//! Build a `Search` via `SearchBuilder`, then `.run()` it (async,
//! `Stream<Item = Match>`). Clone the resulting `Search` across as
//! many spawned tasks as you want concurrency for; the library
//! coordinates them via a shared work-stealing backlog and never
//! spawns anything or picks a worker count itself.
//!
//! ```no_run
//! use lore_core::SearchBuilder;
//! use futures_util::StreamExt;
//!
//! # async fn example() {
//! let search = SearchBuilder::new()
//!     .add_path("~/docs")
//!     .add_pattern("hashmap")
//!     .build();
//!
//! let mut stream = search.run();
//! while let Some(m) = stream.next().await {
//!     println!("{}", m.path.display());
//! }
//! # }
//! ```

mod builder;
mod error;
mod extract;
mod search;

pub use builder::SearchBuilder;
pub use error::LoreError;
pub use extract::{extractor_for, Extractor};
pub use search::{Match, Search};
