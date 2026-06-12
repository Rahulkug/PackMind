//! PackMind planner: candidate gathering (lexical + graph walk),
//! token-budget min-cover with signature substitution, stable ordering,
//! and cache-aware rendering.

pub mod candidates;
pub mod plan;
pub mod render;

pub use candidates::{gather, Candidate};
pub use plan::{build_pack, PackRequest};
