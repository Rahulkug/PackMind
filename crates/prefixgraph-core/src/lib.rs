//! PrefixGraph core: data model, content addressing (PG-NORM-1), token counting,
//! the SQLite store, and the context pack contract.

pub mod hash;
pub mod model;
pub mod norm;
pub mod pack;
pub mod store;
pub mod tokens;

pub use model::{EdgeKind, Node, NodeId, NodeKind};
pub use store::Store;

/// Version string baked into every content hash preimage. Changing normalization
/// rules requires bumping this, which invalidates (re-derives) every node id.
pub const NORM_VERSION: &str = "PG-NORM-1";

/// Envelope format version for rendered context items.
pub const ENVELOPE_VERSION: &str = "PG-ENV-1";

pub const SCHEMA_VERSION: i64 = 1;

/// Name of the state directory created at the repo root.
pub const STATE_DIR: &str = ".prefixgraph";
pub const DB_FILE: &str = "index.db";
