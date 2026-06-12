//! Index orchestrator: full + resumable indexing, Merkle-style incremental
//! invalidation, edge resolution, centrality, and hot-set construction.

use crate::langs::plugin_for;
use crate::plugin::{parse, FileScan};
use crate::walk;
use anyhow::Result;
use packmind_core::hash::{content_hash, node_id, sha256_hex};
use packmind_core::model::{EdgeKind, Node, NodeId, NodeKind};
use packmind_core::store::FileRow;
use packmind_core::{norm::pm_norm_1, tokens, Store};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Default, Clone)]
pub struct IndexOptions {
    pub force: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IndexReport {
    pub files_seen: usize,
    pub files_indexed: usize,
    pub files_unchanged: usize,
    pub files_deleted: usize,
    pub chunks_new: usize,
    pub chunks_preserved: usize,
    pub chunks_staled: usize,
    pub edges_added: usize,
    pub duration_ms: u128,
    pub skipped: Vec<(String, String)>,
    pub hot_set_version: i64,
}

impl IndexReport {
    pub fn cache_stability(&self) -> f64 {
        let total = self.chunks_preserved + self.chunks_staled;
        if total == 0 {
            100.0
        } else {
            100.0 * self.chunks_preserved as f64 / total as f64
        }
    }
}

struct PendingEdges {
    chunk: NodeId,
    path: String,
    calls: Vec<String>,
    bases: Vec<String>,
    impls: Vec<String>,
    is_test: bool,
}

struct PendingImports {
    file_node: NodeId,
    path: String,
    modules: Vec<String>,
    lang: String,
}

struct PendingDoc {
    doc: NodeId,
    content: String,
}

pub fn index_repo(store: &mut Store, opts: &IndexOptions) -> Result<IndexReport> {
    let started = Instant::now();
    let root = store.root.clone();
    let mut report = IndexReport::default();
    let mut pending_edges: Vec<PendingEdges> = Vec::new();
    let mut pending_imports: Vec<PendingImports> = Vec::new();
    let mut pending_docs: Vec<PendingDoc> = Vec::new();

    let files = walk::repo_files(&root);
    report.files_seen = files.len();
    let mut seen_paths: HashSet<String> = HashSet::new();

    for abs in &files {
        let Some(rel) = walk::rel_path(&root, abs) else {
            continue;
        };
        seen_paths.insert(rel.clone());

        let Ok(meta) = std::fs::metadata(abs) else {
            continue;
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let size = meta.len() as i64;

        let existing = store.get_file(&rel);
        if !opts.force {
            if let Some(row) = &existing {
                if row.mtime == mtime && row.size == size && row.skipped.is_none() {
                    report.files_unchanged += 1;
                    continue;
                }
            }
        }

        let Ok(raw) = std::fs::read(abs) else {
            continue;
        };
        if raw.contains(&0u8) {
            record_skip(store, &rel, "binary", mtime, size, &mut report)?;
            continue;
        }
        let Some(norm) = pm_norm_1(&raw) else {
            record_skip(store, &rel, "non-utf8", mtime, size, &mut report)?;
            continue;
        };
        let sha = sha256_hex(norm.as_bytes());
        if !opts.force {
            if let Some(row) = &existing {
                if row.content_sha.as_deref() == Some(sha.as_str()) && row.skipped.is_none() {
                    // touched but unchanged: refresh mtime only
                    let mut row = row.clone();
                    row.mtime = mtime;
                    row.size = size;
                    store.upsert_file(&row)?;
                    report.files_unchanged += 1;
                    continue;
                }
            }
        }

        index_file(
            store,
            &rel,
            &norm,
            mtime,
            size,
            &sha,
            existing.as_ref(),
            &mut report,
            &mut pending_edges,
            &mut pending_imports,
            &mut pending_docs,
        )?;
        report.files_indexed += 1;
    }

    // Deleted files: rows in the DB that no longer exist on disk.
    for row in store.all_files()? {
        if !seen_paths.contains(&row.path) {
            for (_, _, id) in &row.merkle {
                if store.mark_stale(id, &row.path)? {
                    report.chunks_staled += 1;
                }
            }
            for kind in [NodeKind::File, NodeKind::Signature, NodeKind::DocChunk] {
                store.stale_others(&row.path, kind, &HashSet::new())?;
            }
            store.delete_file(&row.path)?;
            report.files_deleted += 1;
        }
    }

    report.edges_added += resolve_edges(store, &pending_edges, &pending_imports, &pending_docs)?;
    recompute_centrality(store)?;
    report.hot_set_version = rebuild_hot_set(store)?;

    if let Some(head) = git_head(&root) {
        store.meta_set("head_commit", &head)?;
    }
    report.duration_ms = started.elapsed().as_millis();
    store.meta_set("last_index_report", &serde_json::to_string(&report)?)?;
    store.meta_set(
        "last_indexed_at",
        &format!("{:?}", std::time::SystemTime::now()),
    )?;
    Ok(report)
}

fn record_skip(
    store: &Store,
    rel: &str,
    reason: &str,
    mtime: i64,
    size: i64,
    report: &mut IndexReport,
) -> Result<()> {
    store.upsert_file(&FileRow {
        path: rel.to_string(),
        file_node: None,
        merkle: vec![],
        mtime,
        size,
        content_sha: None,
        skipped: Some(reason.to_string()),
    })?;
    report.skipped.push((rel.to_string(), reason.to_string()));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn index_file(
    store: &mut Store,
    rel: &str,
    norm: &str,
    mtime: i64,
    size: i64,
    sha: &str,
    existing: Option<&FileRow>,
    report: &mut IndexReport,
    pending_edges: &mut Vec<PendingEdges>,
    pending_imports: &mut Vec<PendingImports>,
    pending_docs: &mut Vec<PendingDoc>,
) -> Result<()> {
    let plugin = plugin_for(rel);
    let lang = plugin.map(|p| p.id().to_string()).unwrap_or_else(|| {
        if rel.ends_with(".md") || rel.ends_with(".markdown") {
            "markdown".into()
        } else {
            "text".into()
        }
    });

    let old_ids: HashSet<NodeId> = existing
        .map(|r| r.merkle.iter().map(|(_, _, id)| *id).collect())
        .unwrap_or_default();

    let scan: FileScan = match plugin {
        Some(p) => match parse(&p.language(), norm) {
            Some(tree) => p.scan(&tree, norm),
            None => FileScan::default(), // parse failure -> FILE-level fallback
        },
        None => FileScan::default(),
    };
    let is_test = plugin.map(|p| p.is_test_path(rel)).unwrap_or(false);
    let role = if is_test { "test" } else { "" };

    let mut new_children: Vec<(i64, i64, NodeId)> = Vec::new();
    let mut keep_sigs: HashSet<NodeId> = HashSet::new();
    let mut keep_docs: HashSet<NodeId> = HashSet::new();

    // --- AST chunks + signatures ---
    for d in &scan.decls {
        let Some(content) = norm.get(d.byte_start..d.byte_end) else {
            continue;
        };
        let hash = content_hash(NodeKind::AstChunk, &lang, content);
        let id = node_id(&hash);
        let preserved = old_ids.contains(&id);

        let chunk = Node {
            id,
            hash,
            kind: NodeKind::AstChunk,
            path: rel.to_string(),
            symbol: Some(d.name.clone()),
            lang: Some(lang.clone()),
            role: role.to_string(),
            byte_start: d.byte_start as i64,
            byte_end: d.byte_end as i64,
            line_start: d.line_start as i64,
            line_end: d.line_end as i64,
            tokens: tokens::count(content),
            content: content.to_string(),
            valid: true,
            centrality: 0.0,
        };
        let was_new = store.upsert_node(&chunk)?;
        if was_new {
            store.fts_replace(&chunk)?;
            report.chunks_new += 1;
        } else if preserved {
            report.chunks_preserved += 1;
        }
        new_children.push((d.byte_start as i64, d.byte_end as i64, id));

        // Signature node (same path/symbol, kind=3).
        let sig_hash = content_hash(NodeKind::Signature, &lang, &d.signature);
        let sig_id = node_id(&sig_hash);
        let sig = Node {
            id: sig_id,
            hash: sig_hash,
            kind: NodeKind::Signature,
            path: rel.to_string(),
            symbol: Some(d.name.clone()),
            lang: Some(lang.clone()),
            role: role.to_string(),
            byte_start: d.byte_start as i64,
            byte_end: d.byte_end as i64,
            line_start: d.line_start as i64,
            line_end: d.line_end as i64,
            tokens: tokens::count(&d.signature),
            content: d.signature.clone(),
            valid: true,
            centrality: 0.0,
        };
        if store.upsert_node(&sig)? {
            store.fts_replace(&sig)?;
        }
        keep_sigs.insert(sig_id);

        pending_edges.push(PendingEdges {
            chunk: id,
            path: rel.to_string(),
            calls: d.calls.clone(),
            bases: d.bases.clone(),
            impls: d.impls.clone(),
            is_test,
        });
    }

    // --- Markdown doc chunks ---
    if lang == "markdown" {
        for (heading, body, line_start, line_end) in split_markdown(norm) {
            if body.len() < 30 {
                continue;
            }
            let hash = content_hash(NodeKind::DocChunk, &lang, &body);
            let id = node_id(&hash);
            let doc = Node {
                id,
                hash,
                kind: NodeKind::DocChunk,
                path: rel.to_string(),
                symbol: Some(heading.clone()),
                lang: Some(lang.clone()),
                role: String::new(),
                byte_start: 0,
                byte_end: body.len() as i64,
                line_start: line_start as i64,
                line_end: line_end as i64,
                tokens: tokens::count(&body),
                content: body.clone(),
                valid: true,
                centrality: 0.0,
            };
            if store.upsert_node(&doc)? {
                store.fts_replace(&doc)?;
            }
            keep_docs.insert(id);
            pending_docs.push(PendingDoc {
                doc: id,
                content: body,
            });
        }
    }

    // --- FILE node ---
    let file_hash = content_hash(NodeKind::File, &lang, norm);
    let file_id = node_id(&file_hash);
    let line_count = norm.lines().count() as i64;
    let file_node = Node {
        id: file_id,
        hash: file_hash,
        kind: NodeKind::File,
        path: rel.to_string(),
        symbol: None,
        lang: Some(lang.clone()),
        role: role.to_string(),
        byte_start: 0,
        byte_end: norm.len() as i64,
        line_start: 1,
        line_end: line_count.max(1),
        tokens: tokens::count(norm),
        content: norm.to_string(),
        valid: true,
        centrality: 0.0,
    };
    store.upsert_node(&file_node)?;
    let mut keep_file = HashSet::new();
    keep_file.insert(file_id);

    if !scan.imports.is_empty() {
        pending_imports.push(PendingImports {
            file_node: file_id,
            path: rel.to_string(),
            modules: scan.imports,
            lang: lang.clone(),
        });
    }

    // --- Invalidation: old chunks not re-derived are stale ---
    let new_ids: HashSet<NodeId> = new_children.iter().map(|(_, _, id)| *id).collect();
    let staled: Vec<NodeId> = old_ids.difference(&new_ids).copied().collect();
    for old in &staled {
        if store.mark_stale(old, rel)? {
            report.chunks_staled += 1;
        }
    }
    // Unambiguous replacement -> SUPERSEDES bookkeeping edge.
    let added: Vec<NodeId> = new_ids.difference(&old_ids).copied().collect();
    if staled.len() == 1 && added.len() == 1 {
        store.add_edge(&added[0], EdgeKind::Supersedes, &staled[0], 1.0)?;
    }
    store.stale_others(rel, NodeKind::Signature, &keep_sigs)?;
    store.stale_others(rel, NodeKind::DocChunk, &keep_docs)?;
    store.stale_others(rel, NodeKind::File, &keep_file)?;

    store.upsert_file(&FileRow {
        path: rel.to_string(),
        file_node: Some(file_id),
        merkle: new_children,
        mtime,
        size,
        content_sha: Some(sha.to_string()),
        skipped: None,
    })?;
    Ok(())
}

/// Split a markdown document into heading-bounded sections.
/// Returns (heading, section text incl. heading, line_start, line_end).
fn split_markdown(norm: &str) -> Vec<(String, String, usize, usize)> {
    let mut sections = Vec::new();
    let mut current: Option<(String, String, usize)> = None;
    let mut line_no = 0usize;
    for line in norm.lines() {
        line_no += 1;
        if line.starts_with('#') {
            if let Some((h, body, start)) = current.take() {
                sections.push((h, body, start, line_no - 1));
            }
            let heading = line.trim_start_matches('#').trim().to_string();
            current = Some((heading, format!("{line}\n"), line_no));
        } else if let Some((_, body, _)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        } else {
            // preamble before the first heading
            current = Some((String::new(), format!("{line}\n"), line_no));
        }
    }
    if let Some((h, body, start)) = current.take() {
        sections.push((h, body, start, line_no));
    }
    sections
}

fn resolve_edges(
    store: &Store,
    pending: &[PendingEdges],
    imports: &[PendingImports],
    docs: &[PendingDoc],
) -> Result<usize> {
    let mut added = 0usize;

    // Symbol map: short name -> [(id, path, role)]
    let mut by_name: HashMap<String, Vec<(NodeId, String, String)>> = HashMap::new();
    for (id, symbol, path, role) in store.symbol_entries()? {
        let short = symbol.rsplit('.').next().unwrap_or(&symbol).to_string();
        by_name.entry(short).or_default().push((id, path, role));
    }

    let resolve = |name: &str, from_path: &str| -> Option<(NodeId, f64, String)> {
        let entries = by_name.get(name)?;
        if let Some((id, p, _)) = entries.iter().find(|(_, p, _)| p == from_path) {
            return Some((*id, 1.0, p.clone()));
        }
        if entries.len() == 1 {
            let (id, p, _) = &entries[0];
            return Some((*id, 0.6, p.clone()));
        }
        None
    };

    for pe in pending {
        for call in &pe.calls {
            if let Some((target, weight, _)) = resolve(call, &pe.path) {
                if target != pe.chunk {
                    store.add_edge(&pe.chunk, EdgeKind::Calls, &target, weight)?;
                    added += 1;
                    if pe.is_test {
                        // subject TESTED_BY test chunk
                        store.add_edge(&target, EdgeKind::TestedBy, &pe.chunk, 0.8)?;
                        added += 1;
                    }
                }
            }
        }
        for base in &pe.bases {
            if let Some((target, weight, _)) = resolve(base, &pe.path) {
                if target != pe.chunk {
                    store.add_edge(&pe.chunk, EdgeKind::Inherits, &target, weight)?;
                    added += 1;
                }
            }
        }
        for imp in &pe.impls {
            if let Some((target, weight, _)) = resolve(imp, &pe.path) {
                if target != pe.chunk {
                    store.add_edge(&pe.chunk, EdgeKind::Implements, &target, weight)?;
                    added += 1;
                }
            }
        }
    }

    // File-level imports -> FILE node edges.
    for pi in imports {
        for module in &pi.modules {
            if let Some(target_path) = resolve_module(store, module, &pi.path, &pi.lang) {
                if let Some(target) = store.file_node(&target_path) {
                    if target.id != pi.file_node {
                        store.add_edge(&pi.file_node, EdgeKind::Imports, &target.id, 1.0)?;
                        added += 1;
                    }
                }
            }
        }
    }

    // Doc mentions: code chunk -> doc chunk, capped per doc.
    for pd in docs {
        let mut count = 0;
        for (name, entries) in &by_name {
            if count >= 8 {
                break;
            }
            if name.len() < 4 {
                continue;
            }
            if contains_word(&pd.content, name) {
                if let Some((id, _, _)) = entries.first() {
                    store.add_edge(id, EdgeKind::MentionsDoc, &pd.doc, 0.7)?;
                    added += 1;
                    count += 1;
                }
            }
        }
    }

    Ok(added)
}

fn contains_word(haystack: &str, word: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(word) {
        let abs = start + pos;
        let before_ok = abs == 0
            || !haystack[..abs]
                .chars()
                .next_back()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
        let after = abs + word.len();
        let after_ok = after >= haystack.len()
            || !haystack[after..]
                .chars()
                .next()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
        start = abs + word.len();
        if start >= haystack.len() {
            break;
        }
    }
    false
}

/// Resolve an import specifier to a repo-relative file path.
fn resolve_module(store: &Store, module: &str, from_path: &str, lang: &str) -> Option<String> {
    let dir = from_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    match lang {
        "python" => {
            let rel = module.trim_start_matches('.');
            let slashed = rel.replace('.', "/");
            for candidate in [
                format!("{slashed}.py"),
                format!("{slashed}/__init__.py"),
                format!("{}.py", rel.rsplit('.').next().unwrap_or(rel)),
            ] {
                if let Ok(hits) = store.paths_matching(&candidate, 2) {
                    if hits.len() == 1 {
                        return Some(hits[0].clone());
                    }
                    if let Some(h) = hits.iter().find(|h| h.starts_with(dir)) {
                        return Some(h.clone());
                    }
                    if let Some(h) = hits.first() {
                        return Some(h.clone());
                    }
                }
            }
            None
        }
        "typescript" | "tsx" => {
            if !module.starts_with('.') {
                return None; // external package
            }
            let joined = join_rel(dir, module);
            for ext in [".ts", ".tsx", ".js", ".jsx", "/index.ts", "/index.js"] {
                let candidate = format!("{joined}{ext}");
                if store.get_file(&candidate).is_some() {
                    return Some(candidate);
                }
            }
            None
        }
        "java" => {
            let slashed = module.replace('.', "/");
            let file = format!("{slashed}.java");
            let short = format!("{}.java", module.rsplit('.').next().unwrap_or(module));
            for candidate in [file, short] {
                if let Ok(hits) = store.paths_matching(&candidate, 2) {
                    if hits.len() == 1 {
                        return Some(hits[0].clone());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Join "src/app" + "../lib/x" -> "src/lib/x".
fn join_rel(dir: &str, spec: &str) -> String {
    let mut parts: Vec<&str> = if dir.is_empty() {
        vec![]
    } else {
        dir.split('/').collect()
    };
    for seg in spec.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn recompute_centrality(store: &mut Store) -> Result<()> {
    let degrees = store.degree_counts()?;
    let max = degrees.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1) as f64;
    let values: Vec<(NodeId, f64)> = degrees
        .into_iter()
        .map(|(id, c)| (id, c as f64 / max))
        .collect();
    store.set_centrality_bulk(&values)
}

/// Hot set: signatures of the most central chunks, capped at 4,000 tokens.
/// Rebuilt per index run; version-pinned for cache-stability tracking.
fn rebuild_hot_set(store: &mut Store) -> Result<i64> {
    const HOT_SET_TOKEN_CAP: i64 = 4000;
    const HOT_SET_MAX_ITEMS: usize = 64;
    let top = store.top_chunks(200)?;
    let (_, previous) = store.hot_set()?;
    let mut ids = Vec::new();
    let mut total = 0i64;
    for chunk in top {
        if chunk.centrality <= 0.0 {
            break;
        }
        let node = chunk
            .symbol
            .as_deref()
            .and_then(|s| store.signature_for(&chunk.path, s))
            .unwrap_or(chunk);
        if total + node.tokens > HOT_SET_TOKEN_CAP {
            continue;
        }
        total += node.tokens;
        ids.push(node.id);
        if ids.len() >= HOT_SET_MAX_ITEMS {
            break;
        }
    }
    // Stability: hash-sort within the hot set so membership (not query order)
    // determines byte order.
    ids.sort();
    if ids == previous {
        let (version, _) = store.hot_set()?;
        return Ok(version);
    }
    store.replace_hot_set(&ids)
}

/// Files whose on-disk state differs from the index (freshness fast path).
pub fn dirty_files(store: &Store) -> Result<Vec<String>> {
    let root = store.root.clone();
    let mut dirty = Vec::new();
    for row in store.all_files()? {
        let abs = root.join(&row.path);
        match std::fs::metadata(&abs) {
            Ok(meta) => {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                if mtime != row.mtime || meta.len() as i64 != row.size {
                    dirty.push(row.path);
                }
            }
            Err(_) => dirty.push(row.path),
        }
    }
    Ok(dirty)
}

fn git_head(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &root.to_string_lossy(),
            "rev-parse",
            "--short",
            "HEAD",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if head.is_empty() {
        None
    } else {
        Some(head)
    }
}
