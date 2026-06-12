//! The context pack: PackMind's universal contract (docs/context-pack-format.md).
//! Every integration surface consumes or produces this object.

use serde::Serialize;

pub const PACK_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize)]
pub struct ContextPack {
    pub pack_version: String,
    pub pack_id: String,
    pub query: String,
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    pub freshness: Freshness,
    pub token_budget: i64,
    pub tokenizer: String,
    pub token_estimate: bool,
    pub items: Vec<PackItem>,
    pub layout: Layout,
    pub totals: Totals,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub lines: [i64; 2],
    pub tokens: i64,
    pub node: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub why: Why,
}

/// Mandatory per-item explanation — explainability is part of the contract.
/// Reasons: anchor | search_hit | imports | imported_by | calls | called_by |
/// inherits | tested_by | doc_mention | hot_set
#[derive(Debug, Clone, Serialize)]
pub struct Why {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Freshness {
    pub state: String, // "fresh" | "stale"
    pub stale_files: i64,
    pub indexed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Layout {
    /// Node ids (hex) that belong to the repo's stable prefix (hot set) and
    /// are rendered first, in hot-set order. Consumers must not reorder items
    /// if they want prefix-cache stability.
    pub stable_prefix_items: Vec<String>,
    pub hot_set_version: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Totals {
    pub selected_tokens: i64,
    /// Tokens of the whole files the items came from — the "dumping files"
    /// counterfactual every savings number is measured against.
    pub estimated_raw_tokens: i64,
    pub saved_tokens: i64,
    pub saved_pct: f64,
}
