//! PackMind CLI — the Level-1 adoption surface.

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use packmind_core::Store;
use packmind_indexer::{dirty_files, index_repo, IndexOptions};
use packmind_planner::plan::{build_pack, PackRequest};
use packmind_planner::render;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "packmind",
    version,
    about = "AST-aware context and prompt-cache optimization for AI coding agents",
    long_about = "PackMind builds an incremental, AST-aware graph of your repository and \
produces compact, explainable, cache-stable context packs for any LLM or coding agent.\n\
Local-first: your code never leaves your machine."
)]
struct Cli {
    /// Repo root (defaults to the nearest ancestor with .packmind, else cwd)
    #[arg(long, global = true)]
    repo: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create .packmind/ state dir and gitignore entry
    Init {
        /// Repo root (default: current directory)
        path: Option<PathBuf>,
    },
    /// Index (or incrementally re-index) the repository
    Index {
        /// Repo root (default: nearest indexed ancestor, else cwd)
        path: Option<PathBuf>,
        /// Re-index everything, ignoring the resume fast path
        #[arg(long)]
        force: bool,
    },
    /// Show index freshness, counts, and cache stability
    Status,
    /// Search the code graph
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Build a context pack for a task or question
    Pack {
        query: String,
        #[arg(long, default_value_t = 12000)]
        budget: i64,
        #[arg(long)]
        json: bool,
        /// Render mode: plain | anthropic | openai
        #[arg(long)]
        render: Option<String>,
        /// Omit item contents (ids + explains only)
        #[arg(long)]
        no_content: bool,
    },
    /// Like `pack`, with a human explanation of every inclusion
    AskContext {
        query: String,
        #[arg(long, default_value_t = 12000)]
        budget: i64,
    },
    /// Who calls this symbol
    Callers { symbol: String },
    /// Which tests cover this file or symbol
    Tests { target: String },
    /// What depends on this file or symbol (reverse closure)
    Impact {
        target: String,
        #[arg(long, default_value_t = 3)]
        depth: usize,
    },
    /// Replay a recorded pack's explains
    Why { pack_id: String },
    /// Serve PackMind tools over MCP (stdio)
    Mcp,
    /// Drop stale nodes older than --days and vacuum
    Gc {
        #[arg(long, default_value_t = 30)]
        days: i64,
    },
}

fn find_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.canonicalize()?);
    }
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.clone();
    loop {
        if dir.join(packmind_core::STATE_DIR).is_dir() {
            return Ok(dir);
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => return Ok(cwd),
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(match e.to_string().contains("no PackMind index") {
            true => 2,
            false => 1,
        });
    }
}

fn run(cli: Cli) -> Result<()> {
    let root = find_root(cli.repo)?;
    match cli.command {
        Command::Init { path } => {
            let root = match path {
                Some(p) => p.canonicalize()?,
                None => root,
            };
            Store::open(&root)?;
            let gi = root.join(".gitignore");
            let entry = format!("{}/", packmind_core::STATE_DIR);
            let current = std::fs::read_to_string(&gi).unwrap_or_default();
            if !current.lines().any(|l| l.trim() == entry) {
                std::fs::write(&gi, format!("{current}{entry}\n"))?;
            }
            println!(
                "initialized {}/{}",
                root.display(),
                packmind_core::STATE_DIR
            );
            println!("next: packmind index .");
        }
        Command::Index { path, force } => {
            let root = match path {
                Some(p) => p.canonicalize()?,
                None => root,
            };
            let mut store = Store::open(&root)?;
            let report = index_repo(&mut store, &IndexOptions { force })?;
            println!(
                "indexed {} files ({} unchanged, {} deleted) in {:.1}s",
                report.files_indexed,
                report.files_unchanged,
                report.files_deleted,
                report.duration_ms as f64 / 1000.0
            );
            println!(
                "chunks: {} new · {} preserved · {} invalidated · cache stability: {:.1}%",
                report.chunks_new,
                report.chunks_preserved,
                report.chunks_staled,
                report.cache_stability()
            );
            println!(
                "edges: +{} · hot set v{}",
                report.edges_added, report.hot_set_version
            );
            if !report.skipped.is_empty() {
                println!(
                    "skipped {} files (see `packmind status`)",
                    report.skipped.len()
                );
            }
        }
        Command::Status => {
            let store = Store::open_existing(&root)?;
            let counts = store.counts()?;
            let dirty = dirty_files(&store)?;
            println!(
                "repo: {} · head: {}",
                root.display(),
                store.meta_get("head_commit").unwrap_or_else(|| "-".into())
            );
            println!(
                "files: {} indexed, {} skipped · chunks: {} · signatures: {} · docs: {} · edges: {}",
                counts.files, counts.skipped_files, counts.chunks,
                counts.signatures, counts.docs, counts.edges
            );
            if let Some(r) = store.meta_get("last_index_report") {
                if let Ok(rep) = serde_json::from_str::<packmind_indexer::IndexReport>(&r) {
                    println!(
                        "last index: {} chunks invalidated · {} preserved · cache stability: {:.1}%",
                        rep.chunks_staled,
                        rep.chunks_preserved,
                        rep.cache_stability()
                    );
                }
            }
            if dirty.is_empty() {
                println!("freshness: fresh");
            } else {
                println!(
                    "freshness: stale ({} files changed) — run: packmind index .",
                    dirty.len()
                );
                for f in dirty.iter().take(10) {
                    println!("  changed: {f}");
                }
            }
        }
        Command::Search { query, limit, json } => {
            let store = Store::open_existing(&root)?;
            let result = packmind_mcp::tools::search_code(&store, &query, limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if let Some(hits) = result.get("hits").and_then(|h| h.as_array()) {
                if hits.is_empty() {
                    println!("no results");
                }
                for h in hits {
                    println!(
                        "{:5.2}  {}::{}  [{}:{}-{}]  ({})",
                        h["score"].as_f64().unwrap_or(0.0),
                        h["path"].as_str().unwrap_or(""),
                        h["symbol"].as_str().unwrap_or("-"),
                        h["kind"].as_str().unwrap_or(""),
                        h["lines"][0],
                        h["lines"][1],
                        h["why"].as_str().unwrap_or("")
                    );
                }
            }
        }
        Command::Pack {
            query,
            budget,
            json,
            render: render_mode,
            no_content,
        } => {
            let store = Store::open_existing(&root)?;
            let stale = dirty_files(&store)?.len() as i64;
            let pack = build_pack(
                &store,
                &PackRequest {
                    query,
                    token_budget: budget,
                    include_content: !no_content || render_mode.is_some(),
                    stale_files: stale,
                    surface: "cli".into(),
                },
            )?;
            match render_mode.as_deref() {
                Some("plain") => println!("{}", render::render_plain(&pack)),
                Some("anthropic") => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&render::render_anthropic(&pack))?
                    )
                }
                Some("openai") => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&render::render_openai(&pack))?
                    )
                }
                Some(other) => {
                    return Err(anyhow!(
                        "unknown render mode '{other}' (plain|anthropic|openai)"
                    ))
                }
                None if json => println!("{}", serde_json::to_string_pretty(&pack)?),
                None => print_pack_summary(&pack),
            }
        }
        Command::AskContext { query, budget } => {
            let store = Store::open_existing(&root)?;
            let stale = dirty_files(&store)?.len() as i64;
            let pack = build_pack(
                &store,
                &PackRequest {
                    query: query.clone(),
                    token_budget: budget,
                    include_content: false,
                    stale_files: stale,
                    surface: "cli".into(),
                },
            )?;
            print_pack_summary(&pack);
        }
        Command::Callers { symbol } => {
            let store = Store::open_existing(&root)?;
            let result = packmind_mcp::tools::find_callers(&store, &symbol)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Tests { target } => {
            let store = Store::open_existing(&root)?;
            let result = packmind_mcp::tools::find_tests(&store, &target)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Impact { target, depth } => {
            let store = Store::open_existing(&root)?;
            let result = packmind_mcp::tools::impact_analysis(&store, &target, depth)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Why { pack_id } => {
            let store = Store::open_existing(&root)?;
            match store.get_pack(&pack_id) {
                Some(json) => {
                    let v: serde_json::Value = serde_json::from_str(&json)?;
                    println!("{}", serde_json::to_string_pretty(&v)?);
                }
                None => return Err(anyhow!("pack '{pack_id}' not found")),
            }
        }
        Command::Mcp => {
            packmind_mcp::serve_stdio(&root)?;
        }
        Command::Gc { days } => {
            let store = Store::open_existing(&root)?;
            let (nodes, edges) = store.gc(days)?;
            println!("gc: removed {nodes} stale nodes, {edges} orphan edges");
        }
    }
    Ok(())
}

fn print_pack_summary(pack: &packmind_core::pack::ContextPack) {
    println!(
        "Selected {} items, {} tokens.  Raw equivalent: ~{} tokens.  Saved: {:.1}%.",
        pack.items.len(),
        pack.totals.selected_tokens,
        pack.totals.estimated_raw_tokens,
        pack.totals.saved_pct
    );
    println!(
        "pack {} · freshness: {}{} · hot set v{}",
        pack.pack_id,
        pack.freshness.state,
        if pack.freshness.stale_files > 0 {
            format!(" ({} files)", pack.freshness.stale_files)
        } else {
            String::new()
        },
        pack.layout.hot_set_version
    );
    println!("\nIncluded:");
    for item in &pack.items {
        println!(
            "- {}::{}  [{} {}-{}, {} tok]  ({}{})",
            item.path,
            item.symbol.as_deref().unwrap_or("-"),
            item.item_type,
            item.lines[0],
            item.lines[1],
            item.tokens,
            item.why.reason,
            item.why
                .score
                .map(|s| format!(" {s:.2}"))
                .unwrap_or_default()
        );
        println!("    why: {}", item.why.detail);
    }
}
