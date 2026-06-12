//! Repo file walker: respects .gitignore and .prefixgraphignore, skips the
//! state dir, binaries, lockfiles, minified bundles, and oversized files.

use std::path::{Path, PathBuf};

pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

const BINARY_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "pdf", "zip", "gz", "tar", "bz2", "xz", "7z",
    "jar", "class", "so", "dylib", "dll", "exe", "bin", "o", "a", "woff", "woff2", "ttf", "eot",
    "otf", "mp3", "mp4", "mov", "avi", "wasm", "pyc", "db", "sqlite", "parquet",
];

const SKIP_NAMES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "poetry.lock",
    "uv.lock",
    "Pipfile.lock",
    "composer.lock",
    "Gemfile.lock",
];

pub fn repo_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .add_custom_ignore_filename(".prefixgraphignore")
        .build();
    for entry in walker.flatten() {
        let path = entry.path();
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        if path
            .components()
            .any(|c| c.as_os_str() == prefixgraph_core::STATE_DIR)
        {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if SKIP_NAMES.contains(&name) || name.ends_with(".min.js") || name.ends_with(".min.css") {
            continue;
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if BINARY_EXTS.contains(&ext.to_ascii_lowercase().as_str()) {
                continue;
            }
        }
        if entry
            .metadata()
            .map(|m| m.len() > MAX_FILE_BYTES)
            .unwrap_or(true)
        {
            continue;
        }
        out.push(path.to_path_buf());
    }
    out.sort();
    out
}

/// Repo-relative path with forward slashes (the canonical path form stored
/// everywhere in the graph).
pub fn rel_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}
