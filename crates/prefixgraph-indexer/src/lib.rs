//! PrefixGraph indexer: walks the repo, parses files with tree-sitter,
//! emits content-addressed chunks/signatures/docs and graph edges, and
//! performs Merkle-style incremental invalidation.

pub mod index;
pub mod langs;
pub mod plugin;
pub mod walk;

pub use index::{dirty_files, index_repo, IndexOptions, IndexReport};
