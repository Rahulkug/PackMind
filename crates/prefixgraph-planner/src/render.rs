//! Cache-aware rendering: pack -> prompt text/blocks with provider cache hints.
//!
//! Layout (PG-ENV-1):
//!   [stable prefix: hot-set items, hot-set order]   <- cache-stable across queries
//!   [query items: ascending content hash]           <- stable per selection
//! Envelope attributes derive from node records only — no timestamps, no user
//! data — so envelope bytes are identical across users by construction.

use prefixgraph_core::pack::{ContextPack, PackItem};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn envelope(item: &PackItem) -> String {
    let content = item.content.as_deref().unwrap_or("");
    format!(
        "<pg:ctx id=\"{}\" path=\"{}\" kind=\"{}\" lines=\"{}-{}\"{}>\n{}</pg:ctx>\n",
        &item.node[..12.min(item.node.len())],
        item.path,
        item.item_type,
        item.lines[0],
        item.lines[1],
        item.symbol
            .as_deref()
            .map(|s| format!(" symbol=\"{s}\""))
            .unwrap_or_default(),
        content
    )
}

fn split_blocks(pack: &ContextPack) -> (String, String) {
    let stable: std::collections::HashSet<&str> = pack
        .layout
        .stable_prefix_items
        .iter()
        .map(|s| s.as_str())
        .collect();
    let mut prefix = String::new();
    let mut query_block = String::new();
    for item in &pack.items {
        let env = envelope(item);
        if stable.contains(item.node.as_str()) {
            prefix.push_str(&env);
        } else {
            query_block.push_str(&env);
        }
    }
    (prefix, query_block)
}

/// Plain text rendering — for piping into any agent (Aider, custom scripts)
/// and for local vLLM servers where byte stability *is* the cache mechanism.
pub fn render_plain(pack: &ContextPack) -> String {
    let (prefix, query_block) = split_blocks(pack);
    format!(
        "<!-- PrefixGraph context pack {} ({} tokens, saved {:.1}%) -->\n{}{}",
        pack.pack_id, pack.totals.selected_tokens, pack.totals.saved_pct, prefix, query_block
    )
}

/// Anthropic Messages API system blocks with cache_control breakpoints:
/// stable prefix gets a 1h-TTL breakpoint, query block a 5m (default) one.
pub fn render_anthropic(pack: &ContextPack) -> Value {
    let (prefix, query_block) = split_blocks(pack);
    let mut blocks = vec![];
    if !prefix.is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": prefix,
            "cache_control": {"type": "ephemeral", "ttl": "1h"}
        }));
    }
    if !query_block.is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": query_block,
            "cache_control": {"type": "ephemeral"}
        }));
    }
    json!({ "system": blocks })
}

/// OpenAI-compatible: one system message + a stable prompt_cache_key derived
/// from the envelope version, hot-set version, and stable item ids — shared
/// by every user of the same repo snapshot.
pub fn render_openai(pack: &ContextPack) -> Value {
    let (prefix, query_block) = split_blocks(pack);
    let mut h = Sha256::new();
    h.update(prefixgraph_core::ENVELOPE_VERSION.as_bytes());
    h.update(pack.layout.hot_set_version.to_be_bytes());
    for id in &pack.layout.stable_prefix_items {
        h.update(id.as_bytes());
    }
    let key = hex::encode(h.finalize());
    json!({
        "messages": [{"role": "system", "content": format!("{prefix}{query_block}")}],
        "prompt_cache_key": format!("pg-{}", &key[..32])
    })
}
