//! Binary smoke tests for the v0.4.0 commands: demo, doctor, pr-context.
//! Each runs the real `packmind` binary against a throwaway copy of the
//! bundled example repo, so the working tree is never touched.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_packmind")
}

fn example_repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/small-python-service")
        .canonicalize()
        .unwrap()
}

/// Copy the example (minus its state dir) into a tempdir and index it.
fn indexed_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = example_repo();
    let mut stack = vec![src.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(&src).unwrap();
            if rel.starts_with(".packmind") {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else {
                let to = dir.path().join(rel);
                std::fs::create_dir_all(to.parent().unwrap()).unwrap();
                std::fs::copy(&p, &to).unwrap();
            }
        }
    }
    run(dir.path(), &["init", "."]);
    run(dir.path(), &["--repo", ".", "index", "."]);
    dir
}

fn run(cwd: &Path, args: &[&str]) -> std::process::Output {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "packmind {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

#[test]
fn doctor_reports_healthy_index() {
    let dir = indexed_copy();
    let out = run(dir.path(), &["--repo", ".", "doctor"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("index:"), "doctor output: {stdout}");
    assert!(stdout.contains("MCP setup:"), "doctor output: {stdout}");
}

#[test]
fn demo_writes_self_contained_html_with_data() {
    let dir = indexed_copy();
    let out_html = dir.path().join("d.html");
    run(
        dir.path(),
        &["--repo", ".", "demo", "--out", out_html.to_str().unwrap()],
    );
    let html = std::fs::read_to_string(&out_html).unwrap();
    // The placeholder must be fully substituted with real data.
    assert!(!html.contains("__PACKMIND_DATA__"), "placeholder not replaced");
    assert!(html.contains("\"packs\""), "no pack data embedded");
    assert!(html.contains("\"bench_savings\""), "no bench data embedded");
    assert!(html.contains("\"cache_report\""), "no cache report embedded");
}

#[test]
fn pr_context_lists_changed_symbols_and_impact() {
    let dir = indexed_copy();
    // Dirty a file so the working-tree form has something to report.
    let payments = dir.path().join("payments.py");
    let mut text = std::fs::read_to_string(&payments).unwrap();
    text.push_str("\n# pr-context test edit\n");
    std::fs::write(&payments, text).unwrap();

    let out = run(dir.path(), &["--repo", ".", "pr-context", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["changed_files"].as_array().unwrap().len(),
        1,
        "expected one changed file"
    );
    let symbols = v["changed_symbols"].as_array().unwrap();
    assert!(
        symbols.iter().any(|s| s.as_str().unwrap().contains("PaymentValidator")),
        "changed symbols missing PaymentValidator: {symbols:?}"
    );
    assert!(v["suggested_pack"]["items"].as_array().unwrap().len() > 0);
    // pr mode must anchor the dirty file.
    let anchored = v["suggested_pack"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["path"] == "payments.py" && i["why"]["reason"] == "anchor");
    assert!(anchored, "pr-context pack did not anchor the changed file");
}

#[test]
fn pack_copy_flag_runs_without_clipboard_or_errors_clearly() {
    let dir = indexed_copy();
    // Clipboard tools are usually absent in CI; --copy must either succeed or
    // fail with an actionable message, never panic.
    let out = Command::new(bin())
        .args(["--repo", ".", "pack", "explain authentication", "--budget", "1500", "--copy"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("clipboard") || combined.contains("Copied"),
        "unexpected --copy output: {combined}"
    );
}
