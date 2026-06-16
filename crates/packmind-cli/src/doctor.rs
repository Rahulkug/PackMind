//! `packmind doctor`: make setup failures self-explanatory. Every check
//! prints a ✓ / ⚠ / ✗ line and the exact command that fixes it.

use anyhow::Result;
use packmind_core::config::Config;
use packmind_core::Store;
use packmind_indexer::dirty_files;
use std::path::Path;

pub fn run(root: &Path) -> Result<i32> {
    let mut warnings = 0;
    let ok = |msg: &str| println!("  ✓ {msg}");
    println!("packmind {} · repo {}", env!("CARGO_PKG_VERSION"), root.display());

    if root.join(".git").exists() {
        ok("git repository detected");
    } else {
        warnings += 1;
        println!("  ⚠ not a git repository — freshness uses file mtimes only");
    }

    let state = root.join(packmind_core::STATE_DIR);
    if !state.is_dir() {
        println!("  ✗ no {} directory — run: packmind init {}", packmind_core::STATE_DIR, root.display());
        return Ok(2);
    }
    ok(&format!("{} state directory exists", packmind_core::STATE_DIR));

    let store = match Store::open_existing(root) {
        Ok(s) => s,
        Err(e) => {
            println!("  ✗ index does not open: {e}");
            return Ok(2);
        }
    };
    let counts = store.counts()?;
    if counts.chunks == 0 {
        println!("  ✗ index is empty — run: packmind index {}", root.display());
        return Ok(2);
    }
    ok(&format!(
        "index: {} files, {} chunks, {} edges, {} docs",
        counts.files, counts.chunks, counts.edges, counts.docs
    ));
    if counts.skipped_files > 0 {
        warnings += 1;
        println!(
            "  ⚠ {} files skipped (non-UTF-8/oversized/unparsed) — see: packmind status",
            counts.skipped_files
        );
    }

    if store.fts_enabled {
        ok("full-text search: FTS5 enabled");
    } else {
        warnings += 1;
        println!("  ⚠ SQLite lacks FTS5 — using slower LIKE fallback search");
    }

    if packmind_core::tokens::is_exact() {
        ok("token counts: exact (o200k_base)");
    } else {
        warnings += 1;
        println!("  ⚠ tokenizer unavailable — counts are chars/4 estimates");
    }

    match Config::load(root) {
        Ok(_) => ok("config: valid (or defaults)"),
        Err(e) => {
            warnings += 1;
            println!("  ⚠ config problem: {e}");
        }
    }

    let dirty = dirty_files(&store)?;
    if dirty.is_empty() {
        ok("freshness: index matches the working tree");
    } else {
        warnings += 1;
        println!(
            "  ⚠ index is stale: {} files changed — run: packmind index {}",
            dirty.len(),
            root.display()
        );
    }

    println!(
        "\nMCP setup:  claude mcp add packmind -- {} --repo {} mcp",
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "packmind".into()),
        root.display()
    );
    if warnings == 0 {
        println!("all checks passed");
    } else {
        println!("{warnings} warning(s) — everything still works, see above");
    }
    Ok(0)
}
