#!/usr/bin/env python3
"""Verify that a PackMind GitHub eval run is backed by real git clones.

This checks the run artifacts against the local repository clones used during
the eval:

- each indexed repo has a .git directory;
- each local HEAD equals the commit recorded in repo_index_metrics.csv;
- each .packmind/index.db exists and its counts match the CSV;
- every pack_id in pack_metrics.jsonl exists in the repo's packs table.

The script writes provenance.json and provenance.md into the run directory.
"""

from __future__ import annotations

import argparse
import csv
import json
import sqlite3
import subprocess
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


@dataclass
class Check:
    name: str
    ok: bool
    detail: str = ""


def run_git(repo_dir: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(repo_dir), *args],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.strip()


def repo_path(workdir: Path, repo_key: str) -> Path:
    return workdir / "repos" / repo_key


def scalar(conn: sqlite3.Connection, sql: str) -> int:
    row = conn.execute(sql).fetchone()
    return int(row[0] if row else 0)


def db_counts(db_path: Path) -> dict[str, int]:
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        return {
            "count_files": scalar(conn, "SELECT COUNT(*) FROM files WHERE skipped IS NULL"),
            "count_skipped_files": scalar(
                conn, "SELECT COUNT(*) FROM files WHERE skipped IS NOT NULL"
            ),
            "count_chunks": scalar(
                conn, "SELECT COUNT(*) FROM nodes WHERE valid=1 AND kind=2"
            ),
            "count_signatures": scalar(
                conn, "SELECT COUNT(*) FROM nodes WHERE valid=1 AND kind=3"
            ),
            "count_docs": scalar(
                conn, "SELECT COUNT(*) FROM nodes WHERE valid=1 AND kind=4"
            ),
            "count_edges": scalar(conn, "SELECT COUNT(*) FROM edges"),
            "pack_count": scalar(conn, "SELECT COUNT(*) FROM packs"),
        }
    finally:
        conn.close()


def pack_exists(db_path: Path, pack_id: str) -> bool:
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        row = conn.execute("SELECT 1 FROM packs WHERE id=?1", [pack_id]).fetchone()
        return row is not None
    finally:
        conn.close()


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows = []
    with path.open(encoding="utf-8") as f:
        for line in f:
            if line.strip():
                rows.append(json.loads(line))
    return rows


def add(checks: list[Check], name: str, ok: bool, detail: str = "") -> None:
    checks.append(Check(name, ok, detail))


def verify(run_dir: Path) -> tuple[dict[str, Any], str]:
    manifest = json.loads((run_dir / "manifest.json").read_text(encoding="utf-8"))
    workdir = Path(manifest["workdir"])
    index_rows = read_csv(run_dir / "repo_index_metrics.csv")
    pack_rows = read_jsonl(run_dir / "pack_metrics.jsonl")
    failure_lines = [
        line
        for line in (run_dir / "failures.jsonl").read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]

    checks: list[Check] = []
    add(checks, "manifest_exists", True, str(run_dir / "manifest.json"))
    add(checks, "index_rows_20", len(index_rows) == 20, f"{len(index_rows)} rows")
    add(checks, "pack_rows_180", len(pack_rows) == 180, f"{len(pack_rows)} rows")
    add(checks, "failures_empty", len(failure_lines) == 0, f"{len(failure_lines)} rows")

    by_repo = {row["repo"]: row for row in index_rows}
    packs_by_repo: dict[str, list[dict[str, Any]]] = {}
    for row in pack_rows:
        packs_by_repo.setdefault(row["repo"], []).append(row)

    repo_proofs = []
    for row in index_rows:
        repo = row["repo"]
        local = repo_path(workdir, row["repo_key"])
        git_dir = local / ".git"
        db_path = local / ".packmind" / "index.db"
        repo_checks: list[Check] = []

        add(repo_checks, "git_dir_exists", git_dir.is_dir(), str(git_dir))
        add(repo_checks, "index_db_exists", db_path.is_file(), str(db_path))

        head = ""
        origin = ""
        shallow = ""
        commit_type = ""
        try:
            head = run_git(local, "rev-parse", "HEAD")
            origin = run_git(local, "remote", "get-url", "origin")
            shallow = run_git(local, "rev-parse", "--is-shallow-repository")
            commit_type = run_git(local, "cat-file", "-t", head)
        except Exception as exc:  # keep collecting all failures
            add(repo_checks, "git_commands", False, str(exc))
        else:
            add(repo_checks, "head_matches_csv", head == row["commit"], head)
            add(repo_checks, "origin_matches_csv", origin == row["url"], origin)
            add(repo_checks, "head_is_commit", commit_type == "commit", commit_type)

        count_match = False
        pack_count = 0
        packs_exist = False
        if db_path.is_file():
            counts = db_counts(db_path)
            count_keys = [
                "count_files",
                "count_skipped_files",
                "count_chunks",
                "count_signatures",
                "count_docs",
                "count_edges",
            ]
            count_match = all(int(row[key]) == counts[key] for key in count_keys)
            repo_pack_ids = [str(p["pack_id"]) for p in packs_by_repo.get(repo, [])]
            pack_count = counts["pack_count"]
            packs_exist = all(pack_exists(db_path, pack_id) for pack_id in repo_pack_ids)
            add(repo_checks, "db_counts_match_csv", count_match, json.dumps(counts))
            add(repo_checks, "nine_packs_in_db", pack_count == 9, f"{pack_count} packs")
            add(repo_checks, "pack_ids_exist_in_db", packs_exist, f"{len(repo_pack_ids)} ids")
        else:
            add(repo_checks, "db_counts_match_csv", False, "missing database")
            add(repo_checks, "nine_packs_in_db", False, "missing database")
            add(repo_checks, "pack_ids_exist_in_db", False, "missing database")

        repo_ok = all(c.ok for c in repo_checks)
        for check in repo_checks:
            add(checks, f"{repo}:{check.name}", check.ok, check.detail)
        repo_proofs.append(
            {
                "repo": repo,
                "url": row["url"],
                "local_path": str(local),
                "commit": row["commit"],
                "head": head,
                "origin": origin,
                "branch": row["branch"],
                "shallow_repository": shallow,
                "index_db": str(db_path),
                "pack_count_in_db": pack_count,
                "ok": repo_ok,
            }
        )

    ok = all(check.ok for check in checks)
    proof = {
        "verified_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "run_dir": str(run_dir),
        "workdir": str(workdir),
        "ok": ok,
        "summary": {
            "index_rows": len(index_rows),
            "pack_rows": len(pack_rows),
            "failure_rows": len(failure_lines),
            "repo_proofs": len(repo_proofs),
            "checks": len(checks),
            "failed_checks": sum(1 for check in checks if not check.ok),
        },
        "repos": repo_proofs,
        "checks": [check.__dict__ for check in checks],
    }
    return proof, render_markdown(proof, by_repo)


def render_markdown(proof: dict[str, Any], by_repo: dict[str, dict[str, str]]) -> str:
    status = "PASS" if proof["ok"] else "FAIL"
    lines = [
        "# GitHub Eval Provenance",
        "",
        f"- Status: `{status}`",
        f"- Verified at: `{proof['verified_at']}`",
        f"- Run directory: `{proof['run_dir']}`",
        f"- Clone workdir: `{proof['workdir']}`",
        f"- Indexed repos: `{proof['summary']['index_rows']}`",
        f"- Pack rows: `{proof['summary']['pack_rows']}`",
        f"- Failure rows: `{proof['summary']['failure_rows']}`",
        f"- Failed checks: `{proof['summary']['failed_checks']}`",
        "",
        "## What Was Verified",
        "",
        "- Each row in `repo_index_metrics.csv` maps to a local directory containing `.git`.",
        "- `git rev-parse HEAD` in each clone equals the recorded commit.",
        "- `git remote get-url origin` equals the recorded GitHub URL.",
        "- Each clone has `.packmind/index.db` from the eval run.",
        "- SQLite node/file/edge counts match `repo_index_metrics.csv`.",
        "- Every `pack_id` in `pack_metrics.jsonl` exists in that repo's `packs` table.",
        "",
        "## Repositories",
        "",
        "| repo | commit | local HEAD | origin | packs in DB | status |",
        "| --- | --- | --- | --- | ---: | --- |",
    ]
    for repo in proof["repos"]:
        recorded = by_repo[repo["repo"]]["commit"][:12]
        head = repo["head"][:12]
        status = "PASS" if repo["ok"] else "FAIL"
        lines.append(
            f"| {repo['repo']} | {recorded} | {head} | {repo['origin']} | "
            f"{repo['pack_count_in_db']} | {status} |"
        )

    failed = [check for check in proof["checks"] if not check["ok"]]
    if failed:
        lines.extend(["", "## Failed Checks", ""])
        for check in failed:
            lines.append(f"- `{check['name']}`: {check['detail']}")
    lines.append("")
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "run_dir",
        type=Path,
        nargs="?",
        default=None,
        help="Result directory to verify. Defaults to the newest eval/results/* directory.",
    )
    args = parser.parse_args()
    if args.run_dir is None:
        candidates = [p for p in Path("eval/results").iterdir() if p.is_dir()]
        if not candidates:
            parser.error("no eval result directories found")
        args.run_dir = max(candidates, key=lambda p: p.stat().st_mtime)
    return args


def main() -> int:
    args = parse_args()
    proof, markdown = verify(args.run_dir)
    (args.run_dir / "provenance.json").write_text(
        json.dumps(proof, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (args.run_dir / "provenance.md").write_text(markdown, encoding="utf-8")
    print(f"{'PASS' if proof['ok'] else 'FAIL'} {args.run_dir}")
    print(json.dumps(proof["summary"], sort_keys=True))
    return 0 if proof["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
