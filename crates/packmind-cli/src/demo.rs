//! `packmind demo`: one command from repo to interactive proof. Indexes the
//! repo (incrementally), builds a pack matrix across modes and budgets, runs
//! cache-report and both benchmarks, and renders everything into a single
//! self-contained HTML file. The template ships inside the binary.

use anyhow::Result;
use packmind_core::config::Config;
use packmind_core::model::{id_hex, EdgeKind, NodeKind};
use packmind_core::Store;
use packmind_indexer::{dirty_files, index_repo, IndexOptions};
use packmind_planner::plan::{build_pack, PackRequest};
use packmind_planner::report::cache_report;
use serde_json::{json, Value};
use std::path::Path;

const TEMPLATE: &str = include_str!("../../../demo/template.html");

pub fn run(root: &Path, out: Option<&Path>, open: bool) -> Result<()> {
    let root = root.canonicalize()?;
    println!("[1/5] indexing {}", root.display());
    let mut store = Store::open(&root)?;
    index_repo(&mut store, &IndexOptions::default())?;

    // Repo-derived pack matrix: same demo story on any repository.
    let top: Vec<String> = store
        .top_chunks(2)?
        .into_iter()
        .filter_map(|n| n.symbol)
        .collect();
    let s0 = top.first().cloned().unwrap_or_else(|| "main".into());
    let s1 = top.get(1).cloned().unwrap_or_else(|| s0.clone());
    let matrix: Vec<(String, &str, i64)> = vec![
        (format!("Refactor {s0}"), "default", 1000),
        (format!("Refactor {s0}"), "default", 4000),
        (format!("Refactor {s0}"), "refactor", 2000),
        (format!("fix a bug in {s0}"), "bugfix", 2000),
        (format!("review {s0} for security issues"), "security", 2000),
        (format!("write tests for {s1}"), "test", 2000),
        ("explain the architecture of this repository".into(), "architecture", 2000),
        ("explain the architecture of this repository".into(), "default", 2000),
    ];

    println!("[2/5] building {} context packs (modes x budgets)", matrix.len());
    let dirty = dirty_files(&store)?;
    let mut packs = vec![];
    for (query, mode, budget) in &matrix {
        let pack = build_pack(
            &store,
            &PackRequest {
                query: query.clone(),
                token_budget: *budget,
                include_content: true,
                dirty_paths: dirty.clone(),
                mode: mode.to_string(),
                surface: "demo".into(),
            },
        )?;
        packs.push(json!({
            "label": format!("{query}  ·  mode: {mode}  ·  budget {budget}"),
            "pack": serde_json::to_value(&pack)?,
        }));
    }

    println!("[3/5] cache-report");
    let cache = cache_report(&store)?;

    println!("[4/5] benchmarks (token-savings + cache-stability edit replay)");
    let savings = crate::bench::token_savings_report(&root, Some(2000), None)?;
    let stability = crate::bench::cache_stability_report(&root)?;

    println!("[5/5] rendering HTML");
    let counts = store.counts()?;
    let (hot_version, _) = store.hot_set()?;
    let nodes: Vec<Value> = store
        .valid_nodes(&[NodeKind::File, NodeKind::AstChunk, NodeKind::DocChunk])?
        .into_iter()
        .map(|n| {
            json!({
                "id": id_hex(&n.id), "kind": n.kind.as_i64(), "path": n.path,
                "symbol": n.symbol, "role": n.role, "tokens": n.tokens,
                "centrality": n.centrality, "lines": [n.line_start, n.line_end],
            })
        })
        .collect();
    let edges: Vec<Value> = store
        .all_edges()?
        .into_iter()
        .filter(|(_, k, _)| *k != EdgeKind::Supersedes)
        .map(|(s, k, d)| json!({"s": id_hex(&s), "t": id_hex(&d), "kind": k.label()}))
        .collect();

    let data = json!({
        "generated_at": utc_now(),
        "version": env!("CARGO_PKG_VERSION"),
        "repo": {
            "name": root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            "files": counts.files, "chunks": counts.chunks,
            "signatures": counts.signatures, "docs": counts.docs,
            "edges": counts.edges, "hot_set_version": hot_version,
        },
        "nodes": nodes,
        "edges": edges,
        "packs": packs,
        "cache_report": cache,
        "bench_savings": savings,
        "bench_stability": stability,
    });

    let out_path = out
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| root.join("packmind-demo.html"));
    let html = TEMPLATE.replace("__PACKMIND_DATA__", &serde_json::to_string(&data)?);
    std::fs::write(&out_path, &html)?;
    println!(
        "\nwrote {} ({} KB) — self-contained, works offline",
        out_path.display(),
        html.len() / 1024
    );
    if open {
        #[cfg(target_os = "macos")]
        let opener = "open";
        #[cfg(not(target_os = "macos"))]
        let opener = "xdg-open";
        let _ = std::process::Command::new(opener).arg(&out_path).spawn();
    } else {
        println!("open it:  open {}", out_path.display());
    }
    let _ = Config::load(&root); // surface config typos early, non-fatal here
    Ok(())
}

/// Minimal UTC timestamp (yyyy-mm-dd hh:mm) without a date dependency.
fn utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        y, m, d, rem / 3600, (rem % 3600) / 60
    )
}
