//! `docf-core` — minimal documentation search library.
//!
//! Build a `Search` via `SearchBuilder`, then `.run()` it (async,
//! `Stream<Item = Match>`). Clone the resulting `Search` across as
//! many spawned tasks as you want concurrency for; the library
//! coordinates them via a shared work-stealing backlog and never
//! spawns anything or picks a worker count itself.
//!
//! ```no_run
//! use docf_core::{SearchBuilder, MatchSet};
//! use futures_util::StreamExt;
//!
//! # async fn example() {
//! let search = SearchBuilder::new()
//!     .add_path("~/docs")
//!     .matching(MatchSet::new().add("hashmap"))
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
mod match_;
mod match_set;
mod params;
mod pipeline;
mod search;

pub use builder::SearchBuilder;
pub use error::DocfError;
pub use extract::{extractor_for, Extractable};
pub use match_::{Match, SourceKind};
pub use match_set::MatchSet;
pub use search::Search;
