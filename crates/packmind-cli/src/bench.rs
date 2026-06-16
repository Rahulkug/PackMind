//! Reproducible, offline benchmarks (LLD §10.5). These produce the numbers
//! the README is allowed to claim: token savings vs the file-dump
//! counterfactual, and prefix stability under a replayed edit sequence.

use anyhow::{anyhow, Result};
use packmind_core::config::Config;
use packmind_core::Store;
use packmind_indexer::{index_repo, walk, IndexOptions};
use packmind_planner::plan::{build_pack, PackRequest};
use packmind_planner::render::envelope_for_node;
use serde_json::json;
use std::path::Path;

/// Data-producing core, shared by the CLI printer and `packmind demo`.
pub fn token_savings_report(
    root: &Path,
    budget: Option<i64>,
    queries_file: Option<&Path>,
) -> Result<serde_json::Value> {
    let store = Store::open_existing(root)?;
    let budget = budget.unwrap_or(Config::load(root)?.plan.budget);
    let queries: Vec<String> = match queries_file {
        Some(p) => std::fs::read_to_string(p)?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect(),
        // Deterministic, repo-derived task queries over the most central
        // symbols — every repo benches itself without fixture files.
        None => store
            .top_chunks(8)?
            .into_iter()
            .filter_map(|n| n.symbol)
            .flat_map(|s| {
                [
                    format!("Refactor {s}"),
                    format!("Where is {s} used and tested?"),
                    format!("Explain how {s} works"),
                ]
            })
            .collect(),
    };
    if queries.is_empty() {
        return Err(anyhow!("no queries to run (empty file or unindexed repo)"));
    }

    let mut rows = vec![];
    for q in &queries {
        let pack = build_pack(
            &store,
            &PackRequest {
                query: q.clone(),
                token_budget: budget,
                include_content: false,
                dirty_paths: vec![],
                mode: String::new(),
                surface: "bench".into(),
            },
        )?;
        rows.push(json!({
            "query": q,
            "items": pack.items.len(),
            "selected_tokens": pack.totals.selected_tokens,
            "raw_tokens": pack.totals.estimated_raw_tokens,
            "saved_pct": pack.totals.saved_pct,
        }));
    }
    let mut saved: Vec<f64> = rows
        .iter()
        .filter_map(|r| r["saved_pct"].as_f64())
        .collect();
    saved.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = saved[saved.len() / 2];
    let mean = saved.iter().sum::<f64>() / saved.len() as f64;

    Ok(json!({
        "benchmark": "token-savings",
        "budget": budget,
        "packs": rows.len(),
        "median_saved_pct": median,
        "mean_saved_pct": (mean * 100.0).round() / 100.0,
        "results": rows,
    }))
}

pub fn token_savings(
    root: &Path,
    budget: Option<i64>,
    queries_file: Option<&Path>,
    json_out: bool,
) -> Result<()> {
    let summary = token_savings_report(root, budget, queries_file)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        let rows = summary["results"].as_array().cloned().unwrap_or_default();
        println!(
            "token-savings · budget {} · {} packs",
            summary["budget"], summary["packs"]
        );
        for r in &rows {
            println!(
                "  {:>5.1}%  {:>5} of {:>6} tok  {:>2} items  {}",
                r["saved_pct"].as_f64().unwrap_or(0.0),
                r["selected_tokens"],
                r["raw_tokens"],
                r["items"],
                r["query"].as_str().unwrap_or("")
            );
        }
        println!(
            "median saved: {:.1}% · mean: {:.1}%",
            summary["median_saved_pct"].as_f64().unwrap_or(0.0),
            summary["mean_saved_pct"].as_f64().unwrap_or(0.0)
        );
    }
    Ok(())
}

/// Replay a scripted edit sequence on a temp copy of the repo and measure
/// what survives: chunk preservation per edit and hot-prefix byte stability.
/// The user's working tree is never touched.
pub fn cache_stability_report(root: &Path) -> Result<serde_json::Value> {
    let src = root.canonicalize()?;
    let dir = tempfile::tempdir()?;
    for f in walk::repo_files(&src) {
        let rel = f.strip_prefix(&src)?;
        if rel.starts_with(packmind_core::STATE_DIR) {
            continue;
        }
        let to = dir.path().join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&f, &to)?;
    }

    let mut store = Store::open(dir.path())?;
    index_repo(&mut store, &IndexOptions::default())?;
    let baseline_prefix = hot_prefix_bytes(&store)?;

    let mut steps = vec![];
    let mut run_step = |store: &mut Store, name: &str| -> Result<()> {
        let report = index_repo(store, &IndexOptions::default())?;
        let prefix = hot_prefix_bytes(store)?;
        steps.push(json!({
            "step": name,
            "chunks_preserved": report.chunks_preserved,
            "chunks_staled": report.chunks_staled,
            "chunks_new": report.chunks_new,
            "preservation_pct": report.cache_stability(),
            "hot_prefix_stable": prefix == baseline_prefix,
        }));
        Ok(())
    };

    // 1. Append a comment to the busiest source file — the canonical
    //    "my cache should survive this" edit.
    if let Some((rel_path, comment)) = comment_target(&store)? {
        let p = dir.path().join(&rel_path);
        let mut text = std::fs::read_to_string(&p)?;
        text.push_str(&format!("\n{comment} packmind-bench: appended comment\n"));
        std::fs::write(&p, text)?;
        run_step(&mut store, "append trailing comment")?;
    }

    // 2. Add a new doc file (no AST chunks -> hot set must not move).
    let note = dir.path().join("packmind_bench_note.md");
    std::fs::write(&note, "# packmind bench\n\ntemporary note file.\n")?;
    run_step(&mut store, "add new doc file")?;

    // 3. Delete it again.
    std::fs::remove_file(&note)?;
    run_step(&mut store, "delete the added file")?;

    let prefix_stable_steps = steps
        .iter()
        .filter(|s| s["hot_prefix_stable"].as_bool() == Some(true))
        .count();
    let min_preservation = steps
        .iter()
        .filter_map(|s| s["preservation_pct"].as_f64())
        .fold(100.0_f64, f64::min);

    Ok(json!({
        "benchmark": "cache-stability",
        "steps": steps,
        "hot_prefix_stable_steps": format!("{prefix_stable_steps}/{}", steps.len()),
        "min_chunk_preservation_pct": min_preservation,
    }))
}

pub fn cache_stability(root: &Path, budget: Option<i64>, json_out: bool) -> Result<()> {
    let _ = budget; // packs are not built here; reserved for future steps
    let summary = cache_stability_report(root)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    let steps = summary["steps"].as_array().cloned().unwrap_or_default();
    println!("cache-stability · replayed {} edits", steps.len());
    for s in &steps {
        println!(
            "  {:<28} preserved {:>5.1}% ({} staled, {} new) · hot prefix {}",
            s["step"].as_str().unwrap_or(""),
            s["preservation_pct"].as_f64().unwrap_or(0.0),
            s["chunks_staled"],
            s["chunks_new"],
            if s["hot_prefix_stable"].as_bool() == Some(true) {
                "stable"
            } else {
                "CHANGED"
            }
        );
    }
    println!(
        "hot prefix stable in {} steps · min preservation {:.1}%",
        summary["hot_prefix_stable_steps"].as_str().unwrap_or(""),
        summary["min_chunk_preservation_pct"].as_f64().unwrap_or(0.0)
    );
    Ok(())
}

/// Rendered bytes of the current hot-set prefix, in hot-set order.
fn hot_prefix_bytes(store: &Store) -> Result<String> {
    let (_, ids) = store.hot_set()?;
    let mut out = String::new();
    for id in &ids {
        if let Some(n) = store.get_node(id) {
            if n.valid {
                out.push_str(&envelope_for_node(&n));
            }
        }
    }
    Ok(out)
}

/// The highest-centrality file with a known comment syntax.
fn comment_target(store: &Store) -> Result<Option<(String, &'static str)>> {
    for n in store.top_chunks(20)? {
        let ext = n.path.rsplit('.').next().unwrap_or("");
        let comment = match ext {
            "py" => Some("#"),
            "ts" | "tsx" | "js" | "jsx" | "java" => Some("//"),
            _ => None,
        };
        if let Some(c) = comment {
            return Ok(Some((n.path, c)));
        }
    }
    Ok(None)
}
