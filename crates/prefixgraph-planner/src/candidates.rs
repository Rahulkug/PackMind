//! Candidate generation: anchors (paths/symbols in the query) + lexical
//! search + bounded 2-hop graph walk, scored deterministically.
//!
//! The score is decomposable by design — every component maps onto a pack
//! item's `why` field. No learned model in the core path.

use anyhow::Result;
use prefixgraph_core::model::{EdgeKind, Node, NodeId, NodeKind};
use prefixgraph_core::pack::Why;
use prefixgraph_core::Store;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub node: Node,
    pub score: f64,
    pub hops: u8,
    pub why: Why,
}

const WALK_KINDS: &[EdgeKind] = &[
    EdgeKind::Imports,
    EdgeKind::Calls,
    EdgeKind::Inherits,
    EdgeKind::Implements,
    EdgeKind::TestedBy,
    EdgeKind::MentionsDoc,
];

fn hop_decay(hops: u8) -> f64 {
    match hops {
        0 => 1.0,
        1 => 0.6,
        _ => 0.35,
    }
}

fn edge_reason(kind: EdgeKind, reverse: bool) -> &'static str {
    match (kind, reverse) {
        (EdgeKind::Imports, false) => "imports",
        (EdgeKind::Imports, true) => "imported_by",
        (EdgeKind::Calls, false) => "calls",
        (EdgeKind::Calls, true) => "called_by",
        (EdgeKind::Inherits, _) => "inherits",
        (EdgeKind::Implements, _) => "inherits",
        (EdgeKind::TestedBy, _) => "tested_by",
        (EdgeKind::MentionsDoc, _) => "doc_mention",
        (EdgeKind::Supersedes, _) => "supersedes",
    }
}

/// Tokens worth treating as potential anchors: path-like or identifier-like.
fn query_tokens(query: &str) -> (Vec<String>, Vec<String>) {
    let mut paths = Vec::new();
    let mut symbols = Vec::new();
    for raw in query.split([' ', '\t', '\n', ',', ';', '(', ')', '`', '"', '\'']) {
        let t = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.' && c != '_');
        if t.len() < 3 {
            continue;
        }
        let has_ext = t.rsplit('.').next().map(|e| e.len() <= 4).unwrap_or(false) && t.contains('.');
        if t.contains('/') || has_ext {
            paths.push(t.to_string());
        }
        if t.chars().all(|c| c.is_alphanumeric() || c == '_')
            && t.chars().any(|c| c.is_alphabetic())
        {
            symbols.push(t.to_string());
        }
    }
    (paths, symbols)
}

pub fn gather(
    store: &Store,
    query: &str,
    extra_anchor_paths: &[String],
    max_candidates: usize,
) -> Result<Vec<Candidate>> {
    let mut cands: HashMap<NodeId, Candidate> = HashMap::new();
    let (path_tokens, symbol_tokens) = query_tokens(query);

    // --- 1. Anchors: explicit paths ---
    let mut anchor_paths: Vec<String> = extra_anchor_paths.to_vec();
    for t in &path_tokens {
        let suffix = t.rsplit('/').next().unwrap_or(t);
        for p in store.paths_matching(suffix, 3)? {
            anchor_paths.push(p);
        }
    }
    anchor_paths.sort();
    anchor_paths.dedup();
    for p in &anchor_paths {
        for node in store.nodes_by_path(p, &[NodeKind::AstChunk])? {
            let why = Why {
                reason: "anchor".into(),
                score: None,
                detail: format!("file referenced in query: {p}"),
            };
            insert(&mut cands, node, 1.0, 0, why);
        }
    }

    // --- 1b. Anchors: symbols named in the query ---
    for s in &symbol_tokens {
        for node in store.find_symbol(s, 3)? {
            let why = Why {
                reason: "anchor".into(),
                score: None,
                detail: format!("symbol named in query: {s}"),
            };
            insert(&mut cands, node, 0.95, 0, why);
        }
    }

    // --- 2. Lexical search ---
    for (node, rank, term) in store.search_text(query, 32)? {
        // Signature nodes match lexically too (their text is a subset of
        // their chunk's), but whether a pack ships a signature instead of
        // the chunk is the planner's call (signature substitution), not
        // retrieval's. Resolve hits back to the chunk so a cheap signature
        // can't outrank and then shadow its own full chunk.
        let node = if node.kind == NodeKind::Signature {
            match node
                .symbol
                .as_deref()
                .and_then(|s| store.chunk_for(&node.path, s))
            {
                Some(chunk) => chunk,
                None => continue,
            }
        } else {
            node
        };
        let why = Why {
            reason: "search_hit".into(),
            score: Some((rank * 100.0).round() / 100.0),
            detail: format!("query term '{term}'"),
        };
        insert(&mut cands, node, rank, 0, why);
    }

    // --- 3. Bounded 2-hop graph walk ---
    let mut frontier: Vec<(NodeId, u8)> = cands.keys().map(|id| (*id, 0u8)).collect();
    let mut hop = 0u8;
    while hop < 2 && frontier.len() < 512 {
        hop += 1;
        let mut next = Vec::new();
        for (id, _) in &frontier {
            let mut neighbors: Vec<(EdgeKind, NodeId, bool)> = Vec::new();
            for (k, other, _w) in store.out_edges(id, WALK_KINDS)? {
                neighbors.push((k, other, false));
            }
            for (k, other, _w) in store.in_edges(id, WALK_KINDS)? {
                neighbors.push((k, other, true));
            }
            let from = store.get_node(id);
            for (kind, other, reverse) in neighbors {
                if cands.contains_key(&other) {
                    continue;
                }
                let Some(node) = store.get_node(&other) else {
                    continue;
                };
                if !node.valid {
                    continue;
                }
                let detail = match &from {
                    Some(f) => format!(
                        "{} edge from {}",
                        edge_reason(kind, reverse),
                        f.symbol.clone().unwrap_or_else(|| f.path.clone())
                    ),
                    None => edge_reason(kind, reverse).to_string(),
                };
                let why = Why {
                    reason: edge_reason(kind, reverse).into(),
                    score: None,
                    detail,
                };
                // FILE nodes are routing structure: expand to their chunks
                // rather than shipping whole files.
                if node.kind == NodeKind::File {
                    for chunk in store
                        .nodes_by_path(&node.path, &[NodeKind::AstChunk])?
                        .into_iter()
                        .take(8)
                    {
                        if !cands.contains_key(&chunk.id) {
                            next.push((chunk.id, hop));
                            insert(&mut cands, chunk, 0.5, hop, why.clone());
                        }
                    }
                } else {
                    next.push((other, hop));
                    insert(&mut cands, node, 0.5, hop, why);
                }
            }
        }
        frontier = next;
    }

    // --- 4. Final scoring ---
    let mut out: Vec<Candidate> = cands.into_values().collect();
    for c in &mut out {
        let text_score = if c.why.reason == "search_hit" {
            c.why.score.unwrap_or(0.5)
        } else if c.why.reason == "anchor" {
            1.0
        } else {
            0.0
        };
        let edge_prior = match c.why.reason.as_str() {
            "anchor" => 1.0,
            "tested_by" | "called_by" | "calls" => 0.8,
            "inherits" | "imports" | "imported_by" => 0.6,
            "doc_mention" => 0.4,
            _ => 0.5,
        };
        c.score = 0.40 * text_score
            + 0.25 * hop_decay(c.hops)
            + 0.15 * edge_prior
            + 0.10 * c.node.centrality
            + 0.10 * if c.node.role == "test" { 0.5 } else { 0.7 };
        c.why.score = Some((c.score * 100.0).round() / 100.0);
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.node.id.cmp(&b.node.id)) // deterministic tie-break
    });
    out.truncate(max_candidates);
    Ok(out)
}

fn insert(cands: &mut HashMap<NodeId, Candidate>, node: Node, score: f64, hops: u8, why: Why) {
    cands
        .entry(node.id)
        .and_modify(|existing| {
            if score > existing.score {
                existing.score = score;
                existing.hops = hops.min(existing.hops);
                // "anchor" is sticky: it grants full-content (no signature
                // substitution) treatment in the planner and must not be
                // downgraded by a later search hit on the same node.
                if existing.why.reason != "anchor" {
                    existing.why = why.clone();
                }
            }
        })
        .or_insert(Candidate {
            node,
            score,
            hops,
            why,
        });
}
