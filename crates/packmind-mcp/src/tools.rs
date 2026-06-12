//! Tool definitions and dispatch. Descriptions are written for agent
//! consumption and are part of the public API (frozen per minor version).

use anyhow::{anyhow, Result};
use packmind_core::model::{id_from_hex, id_hex, EdgeKind, NodeKind};
use packmind_core::Store;
use packmind_planner::plan::{build_pack, PackRequest};
use serde_json::{json, Value};

pub fn definitions() -> Value {
    json!([
        {
            "name": "search_code",
            "description": "Search this repository's code graph (symbols, code text, docs). Returns ranked hits with paths, line ranges, and why each matched. Prefer this over reading many files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search terms, e.g. 'payment validation fx rate'"},
                    "limit": {"type": "integer", "default": 10, "minimum": 1, "maximum": 50}
                },
                "required": ["query"]
            }
        },
        {
            "name": "explain_symbol",
            "description": "Explain a function/class by name: its signature, defining file, and direct relations (callers, callees, tests, docs).",
            "inputSchema": {
                "type": "object",
                "properties": {"symbol": {"type": "string", "description": "Symbol name, e.g. 'PaymentValidator'"}},
                "required": ["symbol"]
            }
        },
        {
            "name": "find_callers",
            "description": "Find code that calls the given function/class (reverse call edges).",
            "inputSchema": {
                "type": "object",
                "properties": {"symbol": {"type": "string"}},
                "required": ["symbol"]
            }
        },
        {
            "name": "find_tests",
            "description": "Find tests covering the given file or symbol (TESTED_BY edges).",
            "inputSchema": {
                "type": "object",
                "properties": {"file_or_symbol": {"type": "string"}},
                "required": ["file_or_symbol"]
            }
        },
        {
            "name": "build_context_pack",
            "description": "Build a token-budgeted, explained context pack for a coding task or question about this repository. Returns the most relevant code chunks, signatures, tests and docs, each with the reason it was included, plus token-savings totals. Prefer this over reading many files one by one.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "The task or question, e.g. 'Refactor PaymentValidator to use FxRateService'"},
                    "token_budget": {"type": "integer", "default": 12000, "minimum": 500},
                    "include_content": {"type": "boolean", "default": true}
                },
                "required": ["query"]
            }
        },
        {
            "name": "changed_since",
            "description": "Report which files changed on disk since the last index, and how many graph chunks that invalidates vs preserves.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "impact_analysis",
            "description": "What depends on this file or symbol: reverse closure over import/call/test edges, grouped by distance. Use before changing shared code.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_or_symbol": {"type": "string"},
                    "depth": {"type": "integer", "default": 3, "minimum": 1, "maximum": 5}
                },
                "required": ["file_or_symbol"]
            }
        },
        {
            "name": "get_content",
            "description": "Fetch the exact (normalized) content of graph nodes by id, e.g. ids returned in a context pack.",
            "inputSchema": {
                "type": "object",
                "properties": {"node_ids": {"type": "array", "items": {"type": "string"}}},
                "required": ["node_ids"]
            }
        }
    ])
}

pub fn dispatch(store: &Store, name: &str, args: &Value) -> Result<Value> {
    let s = |key: &str| -> Option<String> {
        args.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    let i = |key: &str, default: i64| -> i64 {
        args.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
    };
    match name {
        "search_code" => {
            let query = s("query").ok_or_else(|| anyhow!("missing 'query'"))?;
            search_code(store, &query, i("limit", 10) as usize)
        }
        "explain_symbol" => {
            let symbol = s("symbol").ok_or_else(|| anyhow!("missing 'symbol'"))?;
            explain_symbol(store, &symbol)
        }
        "find_callers" => {
            let symbol = s("symbol").ok_or_else(|| anyhow!("missing 'symbol'"))?;
            find_callers(store, &symbol)
        }
        "find_tests" => {
            let target = s("file_or_symbol").ok_or_else(|| anyhow!("missing 'file_or_symbol'"))?;
            find_tests(store, &target)
        }
        "build_context_pack" => {
            let query = s("query").ok_or_else(|| anyhow!("missing 'query'"))?;
            let stale = packmind_indexer::dirty_files(store)?.len() as i64;
            let pack = build_pack(
                store,
                &PackRequest {
                    query,
                    token_budget: i("token_budget", 12_000),
                    include_content: args
                        .get("include_content")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                    stale_files: stale,
                    surface: "mcp".to_string(),
                },
            )?;
            Ok(serde_json::to_value(&pack)?)
        }
        "changed_since" => changed_since(store),
        "impact_analysis" => {
            let target = s("file_or_symbol").ok_or_else(|| anyhow!("missing 'file_or_symbol'"))?;
            impact_analysis(store, &target, i("depth", 3) as usize)
        }
        "get_content" => {
            let ids = args
                .get("node_ids")
                .and_then(|v| v.as_array())
                .ok_or_else(|| anyhow!("missing 'node_ids'"))?;
            let mut out = vec![];
            for v in ids {
                if let Some(id) = v.as_str().and_then(id_from_hex) {
                    if let Some(node) = store.get_node(&id) {
                        out.push(json!({
                            "node": id_hex(&node.id), "path": node.path,
                            "symbol": node.symbol, "content": node.content
                        }));
                    }
                }
            }
            Ok(json!({"nodes": out}))
        }
        other => Err(anyhow!(
            "unknown tool '{other}' — see tools/list for available tools"
        )),
    }
}

pub fn search_code(store: &Store, query: &str, limit: usize) -> Result<Value> {
    let cands = packmind_planner::gather(store, query, &[], limit.max(10))?;
    let hits: Vec<Value> = cands
        .iter()
        .take(limit)
        .map(|c| {
            let snippet: String = c
                .node
                .content
                .lines()
                .take(4)
                .collect::<Vec<_>>()
                .join("\n");
            json!({
                "path": c.node.path, "symbol": c.node.symbol,
                "kind": c.node.kind.label(),
                "lines": [c.node.line_start, c.node.line_end],
                "score": c.why.score, "why": c.why.reason,
                "node": id_hex(&c.node.id), "snippet": snippet
            })
        })
        .collect();
    Ok(json!({"hits": hits}))
}

fn resolve_symbol_node(store: &Store, symbol: &str) -> Result<packmind_core::model::Node> {
    store
        .find_symbol(symbol, 1)?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("symbol '{symbol}' not found — try search_code first"))
}

pub fn explain_symbol(store: &Store, symbol: &str) -> Result<Value> {
    let node = resolve_symbol_node(store, symbol)?;
    let sig = node
        .symbol
        .as_deref()
        .and_then(|s| store.signature_for(&node.path, s));
    let rel = |edges: Vec<(EdgeKind, packmind_core::model::NodeId, f64)>| -> Vec<Value> {
        edges
            .into_iter()
            .filter_map(|(k, id, _)| {
                let n = store.get_node(&id)?;
                if !n.valid {
                    return None;
                }
                Some(
                    json!({"relation": k.label(), "path": n.path, "symbol": n.symbol,
                            "lines": [n.line_start, n.line_end]}),
                )
            })
            .collect()
    };
    let all = [
        EdgeKind::Imports,
        EdgeKind::Calls,
        EdgeKind::Inherits,
        EdgeKind::Implements,
        EdgeKind::TestedBy,
        EdgeKind::MentionsDoc,
    ];
    Ok(json!({
        "symbol": node.symbol, "path": node.path,
        "lines": [node.line_start, node.line_end],
        "signature": sig.map(|s| s.content).unwrap_or_else(|| node.content.lines().take(3).collect::<Vec<_>>().join("\n")),
        "outgoing": rel(store.out_edges(&node.id, &all)?),
        "incoming": rel(store.in_edges(&node.id, &all)?),
        "node": id_hex(&node.id)
    }))
}

pub fn find_callers(store: &Store, symbol: &str) -> Result<Value> {
    let node = resolve_symbol_node(store, symbol)?;
    let callers: Vec<Value> = store
        .in_edges(&node.id, &[EdgeKind::Calls])?
        .into_iter()
        .filter_map(|(_, id, w)| {
            let n = store.get_node(&id)?;
            if !n.valid {
                return None;
            }
            Some(json!({"path": n.path, "symbol": n.symbol,
                        "lines": [n.line_start, n.line_end], "confidence": w}))
        })
        .collect();
    Ok(json!({"symbol": symbol, "callers": callers}))
}

pub fn find_tests(store: &Store, target: &str) -> Result<Value> {
    // Resolve a symbol, or every chunk of a matching file.
    let nodes = if target.contains('/') || target.contains('.') {
        let suffix = target.rsplit('/').next().unwrap_or(target);
        let mut nodes = vec![];
        for p in store.paths_matching(suffix, 3)? {
            nodes.extend(store.nodes_by_path(&p, &[NodeKind::AstChunk])?);
        }
        nodes
    } else {
        store.find_symbol(target, 5)?
    };
    if nodes.is_empty() {
        return Err(anyhow!("'{target}' not found — try search_code first"));
    }
    let mut tests = vec![];
    for n in &nodes {
        for (_, id, _) in store.out_edges(&n.id, &[EdgeKind::TestedBy])? {
            if let Some(t) = store.get_node(&id) {
                if t.valid {
                    tests.push(json!({"path": t.path, "symbol": t.symbol,
                                      "lines": [t.line_start, t.line_end],
                                      "covers": n.symbol}));
                }
            }
        }
    }
    Ok(json!({"target": target, "tests": tests}))
}

pub fn changed_since(store: &Store) -> Result<Value> {
    let dirty = packmind_indexer::dirty_files(store)?;
    let report: Value = store
        .meta_get("last_index_report")
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(json!({}));
    Ok(json!({
        "changed_files": dirty,
        "last_index": report,
        "hint": if dirty.is_empty() { "index is fresh" } else { "run: packmind index ." }
    }))
}

pub fn impact_analysis(store: &Store, target: &str, depth: usize) -> Result<Value> {
    let seeds = if target.contains('/') || target.contains('.') {
        let suffix = target.rsplit('/').next().unwrap_or(target);
        let mut nodes = vec![];
        for p in store.paths_matching(suffix, 3)? {
            nodes.extend(store.nodes_by_path(&p, &[NodeKind::AstChunk])?);
            if let Some(f) = store.file_node(&p) {
                nodes.push(f);
            }
        }
        nodes
    } else {
        store.find_symbol(target, 5)?
    };
    if seeds.is_empty() {
        return Err(anyhow!("'{target}' not found"));
    }
    let kinds = [
        EdgeKind::Calls,
        EdgeKind::Imports,
        EdgeKind::Inherits,
        EdgeKind::Implements,
        EdgeKind::TestedBy,
    ];
    let mut visited: std::collections::HashSet<_> = seeds.iter().map(|n| n.id).collect();
    let mut frontier: Vec<_> = seeds.iter().map(|n| n.id).collect();
    let mut levels = vec![];
    for d in 1..=depth {
        let mut next = vec![];
        let mut level_items = vec![];
        for id in &frontier {
            for (k, other, _) in store.in_edges(id, &kinds)? {
                if visited.insert(other) {
                    if let Some(n) = store.get_node(&other) {
                        if n.valid {
                            level_items.push(json!({"path": n.path, "symbol": n.symbol,
                                                    "via": k.label()}));
                            next.push(other);
                        }
                    }
                }
            }
        }
        if level_items.is_empty() {
            break;
        }
        levels.push(json!({"distance": d, "count": level_items.len(), "items": level_items}));
        frontier = next;
    }
    Ok(json!({"target": target, "impacted": levels}))
}
