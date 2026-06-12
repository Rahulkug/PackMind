//! PackMind planner: candidate gathering (lexical + graph walk),
//! token-budget min-cover with signature substitution, stable ordering,
//! task-mode profiles, and cache-aware rendering.

pub mod candidates;
pub mod plan;
pub mod profile;
pub mod render;
pub mod report;

pub use candidates::{gather, Candidate};
pub use plan::{build_pack, PackRequest};
pub use profile::{Mode, Profile, MODE_NAMES};
