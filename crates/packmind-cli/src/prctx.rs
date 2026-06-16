//! `packmind pr-context`: PR-shaped context — what changed, what it
//! touches, and a suggested review pack. Useful with or without an agent.

use anyhow::{anyhow, Result};
use packmind_core::model::{EdgeKind, NodeKind};
use packmind_core::Store;
use packmind_indexer::dirty_files;
use packmind_planner::plan::{build_pack, PackRequest};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub fn run(root: &Path, since: Option<&str>, budget: i64, json_out: bool) -> Result<()> {
    let store = Store::open_existing(root)?;

    // Changed files: a git range when given, else working tree vs index.
    let (source, changed): (String, Vec<String>) = match since {
        Some(rev) => {
            let out = std::process::Command::new("git")
                .args(["-C", &root.to_string_lossy(), "diff", "--name-only", rev])
                .output()
                .map_err(|e| anyhow!("git not available: {e}"))?;
            if !out.status.success() {
                return Err(anyhow!(
                    "git diff --name-only {rev} failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            let files = String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::to_string)
                .collect();
            (format!("git diff {rev}"), files)
        }
        None => ("working tree vs last index".into(), dirty_files(&store)?),
    };
    if changed.is_empty() {
        println!("no changed files ({source}) — nothing to review");
        return Ok(());
    }

    // Changed symbols: indexed chunks living in the changed files.
    let mut changed_symbols = vec![];
    let mut seeds = vec![];
    for path in &changed {
        for n in store.nodes_by_path(path, &[NodeKind::AstChunk])? {
            if let Some(s) = &n.symbol {
                changed_symbols.push(format!("{path}::{s}"));
            }
            seeds.push(n);
        }
    }

    // Impact: who calls/imports/tests the changed chunks.
    let kinds = [EdgeKind::Calls, EdgeKind::Imports, EdgeKind::Inherits, EdgeKind::TestedBy];
    let mut impacted: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for seed in &seeds {
        for (k, other, _) in store.in_edges(&seed.id, &kinds)? {
            if let Some(n) = store.get_node(&other) {
                if n.valid && !changed.contains(&n.path) {
                    let entry = format!(
                        "{}::{}",
                        n.path,
                        n.symbol.as_deref().unwrap_or("-")
                    );
                    let bucket = impacted.entry(k.label().to_string()).or_default();
                    if !bucket.contains(&entry) {
                        bucket.push(entry);
                    }
                }
            }
        }
        // tests covering the change are TESTED_BY out-edges from the subject
        for (_, other, _) in store.out_edges(&seed.id, &[EdgeKind::TestedBy])? {
            if let Some(n) = store.get_node(&other) {
                if n.valid {
                    let entry = format!("{}::{}", n.path, n.symbol.as_deref().unwrap_or("-"));
                    let bucket = impacted.entry("tested_by".into()).or_default();
                    if !bucket.contains(&entry) {
                        bucket.push(entry);
                    }
                }
            }
        }
    }

    // Suggested review pack: pr mode anchors the changed files.
    let names: Vec<&str> = changed_symbols
        .iter()
        .filter_map(|s| s.rsplit("::").next())
        .take(5)
        .collect();
    let query = if names.is_empty() {
        format!("review changes to {}", changed.join(", "))
    } else {
        format!("review changes to {}", names.join(", "))
    };
    let pack = build_pack(
        &store,
        &PackRequest {
            query,
            token_budget: budget,
            include_content: false,
            dirty_paths: changed.clone(),
            mode: "pr".into(),
            surface: "cli".into(),
        },
    )?;

    if json_out {
        let result: Value = json!({
            "source": source,
            "changed_files": changed,
            "changed_symbols": changed_symbols,
            "impacted": impacted,
            "suggested_pack": serde_json::to_value(&pack)?,
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("PR context ({source})\n");
    println!("Changed files ({}):", changed.len());
    for f in &changed {
        println!("  {f}");
    }
    if !changed_symbols.is_empty() {
        println!("\nChanged symbols ({}):", changed_symbols.len());
        for s in &changed_symbols {
            println!("  {s}");
        }
    }
    if impacted.is_empty() {
        println!("\nImpacted: none found in the graph");
    } else {
        println!("\nImpacted:");
        for (rel, items) in &impacted {
            println!("  {rel} ({}):", items.len());
            for i in items.iter().take(8) {
                println!("    {i}");
            }
        }
    }
    let tests = pack
        .items
        .iter()
        .filter(|i| i.item_type == "test" || i.why.reason == "tested_by")
        .count();
    println!(
        "\nSuggested review pack: {} items · {} tok · {} test items · pack {}",
        pack.items.len(),
        pack.totals.selected_tokens,
        tests,
        pack.pack_id
    );
    println!("  replay it:  packmind why {}", pack.pack_id);
    println!(
        "  full pack:  packmind pack \"{}\" --mode pr --budget {budget} --json",
        pack.query
    );
    Ok(())
}
