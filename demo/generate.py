#!/usr/bin/env python3
"""End-to-end PackMind run -> self-contained interactive HTML demo.

Runs the real binary against a repo (default: the bundled example service),
collects pack/report/bench JSON plus the code graph from the index, and
injects it all into template.html. The output opens offline in any browser.

Usage:
    python3 demo/generate.py [--repo PATH] [--bin PATH] [--out PATH]
"""

import argparse
import datetime
import json
import sqlite3
import subprocess
import sys
from pathlib import Path

PROJECT = Path(__file__).resolve().parent.parent

EDGE_KINDS = {1: "imports", 2: "calls", 3: "inherits", 4: "implements",
              5: "tested_by", 6: "doc_mention"}  # 7 supersedes: bookkeeping, skip

# (query, mode, budget) — a matrix that shows budgets and modes doing work.
PACKS = [
    ("Refactor PaymentValidator to use FxRateService", "default", 500),
    ("Refactor PaymentValidator to use FxRateService", "default", 1000),
    ("Refactor PaymentValidator to use FxRateService", "default", 4000),
    ("Refactor PaymentValidator to use FxRateService", "refactor", 2000),
    ("fix currency rounding in payment validation", "default", 2000),
    ("fix currency rounding in payment validation", "bugfix", 2000),
    ("review payment request handling", "default", 2000),
    ("review payment request handling", "security", 2000),
    ("write tests for process_payment", "test", 2000),
    ("explain the architecture of this service", "architecture", 2000),
]


def run(binary, repo, *args, parse=True):
    cmd = [str(binary), "--repo", str(repo), *args]
    print(f"  $ packmind {' '.join(args)}")
    out = subprocess.run(cmd, capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(f"command failed ({out.returncode}): {' '.join(cmd)}\n{out.stderr}")
    return json.loads(out.stdout) if parse else out.stdout


def graph_data(repo):
    """Nodes + typed edges straight from the index — the demo shows real state."""
    db = repo / ".packmind" / "index.db"
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    nodes, edges = [], []
    for hid, kind, path, symbol, role, tokens, cent, l0, l1 in con.execute(
        "SELECT hex(id), kind, path, symbol, role, tokens, centrality,"
        "       line_start, line_end FROM nodes WHERE valid=1 AND kind IN (1,2,4)"
    ):
        nodes.append({"id": hid.lower(), "kind": kind, "path": path,
                      "symbol": symbol, "role": role, "tokens": tokens,
                      "centrality": cent, "lines": [l0, l1]})
    for src, kind, dst in con.execute("SELECT hex(src), kind, hex(dst) FROM edges"):
        if kind in EDGE_KINDS:
            edges.append({"s": src.lower(), "t": dst.lower(), "kind": EDGE_KINDS[kind]})
    counts = dict(con.execute(
        "SELECT kind, COUNT(*) FROM nodes WHERE valid=1 GROUP BY kind"))
    n_files = con.execute(
        "SELECT COUNT(*) FROM files WHERE skipped IS NULL").fetchone()[0]
    n_edges = con.execute("SELECT COUNT(*) FROM edges").fetchone()[0]
    hot_v = con.execute(
        "SELECT value FROM meta WHERE key='hot_set_version'").fetchone()
    con.close()
    repo_stats = {"name": repo.name, "files": n_files,
                  "chunks": counts.get(2, 0), "signatures": counts.get(3, 0),
                  "docs": counts.get(4, 0), "edges": n_edges,
                  "hot_set_version": int(hot_v[0]) if hot_v else 0}
    return nodes, edges, repo_stats


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--repo", type=Path,
                    default=PROJECT / "examples" / "small-python-service")
    ap.add_argument("--bin", type=Path, default=None,
                    help="packmind binary (default: target/release, else target/debug)")
    ap.add_argument("--out", type=Path, default=PROJECT / "demo" / "packmind-demo.html")
    args = ap.parse_args()

    binary = args.bin or next(
        (p for p in (PROJECT / "target/release/packmind",
                     PROJECT / "target/debug/packmind") if p.exists()), None)
    if not binary:
        sys.exit("no packmind binary found — run: cargo build --release")
    repo = args.repo.resolve()

    print(f"[1/6] index {repo.name}")
    run(binary, repo, "init", parse=False)
    run(binary, repo, "index", str(repo), parse=False)

    print(f"[2/6] build {len(PACKS)} context packs (queries x modes x budgets)")
    packs = []
    for query, mode, budget in PACKS:
        pack = run(binary, repo, "pack", query, "--mode", mode,
                   "--budget", str(budget), "--json")
        packs.append({"label": f"{query}  ·  mode: {mode}  ·  budget {budget}",
                      "pack": pack})

    print("[3/6] cache-report")
    cache_report = run(binary, repo, "cache-report", "--json")

    print("[4/6] bench token-savings")
    bench_savings = run(binary, repo, "bench", "token-savings",
                        "--budget", "2000", "--json")

    print("[5/6] bench cache-stability (edit replay on a temp copy)")
    bench_stability = run(binary, repo, "bench", "cache-stability", "--json")

    print("[6/6] render HTML")
    nodes, edges, repo_stats = graph_data(repo)
    version = run(binary, repo, "--version", parse=False).split()[-1]
    data = {
        "generated_at": datetime.datetime.now().strftime("%Y-%m-%d %H:%M"),
        "version": version,
        "repo": repo_stats,
        "nodes": nodes,
        "edges": edges,
        "packs": packs,
        "cache_report": cache_report,
        "bench_savings": bench_savings,
        "bench_stability": bench_stability,
    }
    template = (PROJECT / "demo" / "template.html").read_text()
    html = template.replace("__PACKMIND_DATA__", json.dumps(data))
    args.out.write_text(html)
    print(f"\nwrote {args.out} ({len(html) // 1024} KB)")
    print(f"open it:  open {args.out}")


if __name__ == "__main__":
    main()
