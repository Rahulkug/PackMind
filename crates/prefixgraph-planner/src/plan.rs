//! Token-budget planning: greedy density selection with signature
//! substitution and containment, then cache-stable ordering.

use crate::candidates::Candidate;
use anyhow::Result;
use prefixgraph_core::model::{id_hex, Node, NodeKind};
use prefixgraph_core::pack::{ContextPack, Freshness, Layout, PackItem, Totals, Why, PACK_VERSION};
use prefixgraph_core::{tokens, Store};
use std::collections::{HashMap, HashSet};

pub struct PackRequest {
    pub query: String,
    pub token_budget: i64,
    pub include_content: bool,
    pub stale_files: i64,
    pub surface: String, // "cli" | "mcp" | "proxy"
}

struct Chosen {
    node: Node,
    why: Why,
    substituted: bool,
}

/// Greedy weighted selection (score/tokens density) with two code-aware
/// refinements:
/// 1. signature substitution — a non-anchor item that doesn't fit degrades
///    to its signature node instead of being dropped;
/// 2. containment — never spend budget twice on overlapping bytes of a file.
fn select(store: &Store, cands: &[Candidate], budget: i64) -> Vec<Chosen> {
    let mut order: Vec<&Candidate> = cands.iter().collect();
    order.sort_by(|a, b| {
        let da = a.score / a.node.tokens.max(1) as f64;
        let db = b.score / b.node.tokens.max(1) as f64;
        db.partial_cmp(&da)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.node.id.cmp(&b.node.id))
    });

    let mut chosen: Vec<Chosen> = Vec::new();
    let mut used = 0i64;
    let mut covered: HashMap<String, Vec<(i64, i64)>> = HashMap::new();

    let overlaps = |covered: &HashMap<String, Vec<(i64, i64)>>, n: &Node| -> bool {
        covered
            .get(&n.path)
            .map(|ranges| {
                ranges
                    .iter()
                    .any(|(s, e)| n.byte_start < *e && *s < n.byte_end && n.kind == NodeKind::AstChunk)
            })
            .unwrap_or(false)
    };

    for c in order {
        if overlaps(&covered, &c.node) {
            continue;
        }
        let t = c.node.tokens;
        if used + t <= budget {
            covered
                .entry(c.node.path.clone())
                .or_default()
                .push((c.node.byte_start, c.node.byte_end));
            used += t;
            chosen.push(Chosen {
                node: c.node.clone(),
                why: c.why.clone(),
                substituted: false,
            });
            continue;
        }
        // Signature substitution: anchors must appear in full; everything
        // else degrades gracefully (interfaces are most of the value at a
        // tenth of the tokens).
        if c.why.reason != "anchor" && c.node.kind == NodeKind::AstChunk {
            if let Some(symbol) = c.node.symbol.as_deref() {
                if let Some(sig) = store.signature_for(&c.node.path, symbol) {
                    if used + sig.tokens <= budget && !overlaps(&covered, &sig) {
                        used += sig.tokens;
                        chosen.push(Chosen {
                            node: sig,
                            why: c.why.clone(),
                            substituted: true,
                        });
                    }
                }
            }
        }
    }
    chosen
}

/// Cache-stable ordering: hot-set members first (hot-set order), then
/// ascending content hash. Deterministic across queries, sessions, users.
fn stable_order(chosen: &mut Vec<Chosen>, hot_ids: &[prefixgraph_core::model::NodeId]) {
    let hot_pos: HashMap<_, _> = hot_ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    chosen.sort_by(|a, b| {
        let ka = hot_pos.get(&a.node.id).copied();
        let kb = hot_pos.get(&b.node.id).copied();
        match (ka, kb) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.node.hash.cmp(&b.node.hash),
        }
    });
}

pub fn build_pack(store: &Store, req: &PackRequest) -> Result<ContextPack> {
    let cands = crate::candidates::gather(store, &req.query, &[], 600)?;
    let mut chosen = select(store, &cands, req.token_budget);
    let (hot_version, hot_ids) = store.hot_set()?;
    stable_order(&mut chosen, &hot_ids);

    let hot_member: HashSet<_> = hot_ids.iter().collect();
    let stable_prefix_items: Vec<String> = chosen
        .iter()
        .filter(|c| hot_member.contains(&c.node.id))
        .map(|c| id_hex(&c.node.id))
        .collect();

    let selected_tokens: i64 = chosen.iter().map(|c| c.node.tokens).sum();

    // Counterfactual: tokens of the whole files the items came from.
    let mut paths: Vec<&str> = chosen.iter().map(|c| c.node.path.as_str()).collect();
    paths.sort();
    paths.dedup();
    let mut raw_tokens = 0i64;
    for p in &paths {
        match store.file_node(p) {
            Some(f) => raw_tokens += f.tokens,
            None => raw_tokens += chosen
                .iter()
                .filter(|c| c.node.path == *p)
                .map(|c| c.node.tokens)
                .sum::<i64>(),
        }
    }
    let saved = (raw_tokens - selected_tokens).max(0);
    let saved_pct = if raw_tokens > 0 {
        (10_000.0 * saved as f64 / raw_tokens as f64).round() / 100.0
    } else {
        0.0
    };

    let items: Vec<PackItem> = chosen
        .iter()
        .map(|c| PackItem {
            item_type: if c.substituted {
                "signature".to_string()
            } else if c.node.role == "test" {
                "test".to_string()
            } else {
                c.node.kind.label().to_string()
            },
            path: c.node.path.clone(),
            symbol: c.node.symbol.clone(),
            lines: [c.node.line_start, c.node.line_end],
            tokens: c.node.tokens,
            node: id_hex(&c.node.id),
            content: if req.include_content {
                Some(c.node.content.clone())
            } else {
                None
            },
            why: c.why.clone(),
        })
        .collect();

    let pack = ContextPack {
        pack_version: PACK_VERSION.to_string(),
        pack_id: ulid::Ulid::new().to_string(),
        query: req.query.clone(),
        repo: store
            .root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".to_string()),
        head: store.meta_get("head_commit"),
        freshness: Freshness {
            state: if req.stale_files == 0 {
                "fresh".into()
            } else {
                "stale".into()
            },
            stale_files: req.stale_files,
            indexed_at: store.meta_get("last_indexed_at").unwrap_or_default(),
        },
        token_budget: req.token_budget,
        tokenizer: tokens::TOKENIZER_NAME.to_string(),
        token_estimate: !tokens::is_exact(),
        items,
        layout: Layout {
            stable_prefix_items,
            hot_set_version: hot_version,
        },
        totals: Totals {
            selected_tokens,
            estimated_raw_tokens: raw_tokens,
            saved_tokens: saved,
            saved_pct,
        },
    };

    store.record_pack(&pack, &req.surface)?;
    let ids: Vec<_> = chosen.iter().map(|c| c.node.id).collect();
    store.bump_stats(&ids)?;
    Ok(pack)
}
