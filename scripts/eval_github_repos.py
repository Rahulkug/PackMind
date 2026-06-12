#!/usr/bin/env python3
"""Run PrefixGraph pack/index measurements against real GitHub repositories.

The harness clones or reuses a fixed 20-repository corpus, indexes each repo
with the PrefixGraph CLI, then runs pack generation across several budgets and
query profiles. Outputs are written as JSONL/CSV plus a compact Markdown report.
"""

from __future__ import annotations

import argparse
import csv
import json
import shutil
import sqlite3
import subprocess
import sys
import time
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from statistics import median
from typing import Any


REPOS: list[dict[str, str]] = [
    {
        "repo": "pypa/sampleproject",
        "url": "https://github.com/pypa/sampleproject.git",
        "language": "python",
    },
    {
        "repo": "pallets/itsdangerous",
        "url": "https://github.com/pallets/itsdangerous.git",
        "language": "python",
    },
    {
        "repo": "pallets/click",
        "url": "https://github.com/pallets/click.git",
        "language": "python",
    },
    {
        "repo": "psf/requests",
        "url": "https://github.com/psf/requests.git",
        "language": "python",
    },
    {
        "repo": "pytest-dev/pluggy",
        "url": "https://github.com/pytest-dev/pluggy.git",
        "language": "python",
    },
    {
        "repo": "encode/httpcore",
        "url": "https://github.com/encode/httpcore.git",
        "language": "python",
    },
    {
        "repo": "tiangolo/typer",
        "url": "https://github.com/tiangolo/typer.git",
        "language": "python",
    },
    {
        "repo": "sindresorhus/is",
        "url": "https://github.com/sindresorhus/is.git",
        "language": "typescript",
    },
    {
        "repo": "sindresorhus/ky",
        "url": "https://github.com/sindresorhus/ky.git",
        "language": "typescript",
    },
    {
        "repo": "ai/nanoid",
        "url": "https://github.com/ai/nanoid.git",
        "language": "javascript",
    },
    {
        "repo": "tj/commander.js",
        "url": "https://github.com/tj/commander.js.git",
        "language": "javascript",
    },
    {
        "repo": "expressjs/express",
        "url": "https://github.com/expressjs/express.git",
        "language": "javascript",
    },
    {
        "repo": "axios/axios",
        "url": "https://github.com/axios/axios.git",
        "language": "javascript",
    },
    {
        "repo": "chalk/chalk",
        "url": "https://github.com/chalk/chalk.git",
        "language": "javascript",
    },
    {
        "repo": "google/gson",
        "url": "https://github.com/google/gson.git",
        "language": "java",
    },
    {
        "repo": "junit-team/junit4",
        "url": "https://github.com/junit-team/junit4.git",
        "language": "java",
    },
    {
        "repo": "apache/commons-cli",
        "url": "https://github.com/apache/commons-cli.git",
        "language": "java",
    },
    {
        "repo": "apache/commons-csv",
        "url": "https://github.com/apache/commons-csv.git",
        "language": "java",
    },
    {
        "repo": "apache/commons-io",
        "url": "https://github.com/apache/commons-io.git",
        "language": "java",
    },
    {
        "repo": "spring-guides/gs-rest-service",
        "url": "https://github.com/spring-guides/gs-rest-service.git",
        "language": "java",
    },
]

DEFAULT_BUDGETS = [2000, 6000, 12000]

BASE_QUERIES = [
    {
        "name": "architecture",
        "query": "Explain the main architecture, entry points, and important data flow.",
    },
    {
        "name": "tests",
        "query": "Which tests cover the main public API and important behavior?",
    },
]


@dataclass
class CommandResult:
    args: list[str]
    returncode: int
    stdout: str
    stderr: str
    elapsed_ms: int


class CommandFailed(RuntimeError):
    def __init__(self, result: CommandResult):
        self.result = result
        super().__init__(
            f"command failed with exit {result.returncode}: {' '.join(result.args)}"
        )


def run_cmd(
    args: list[str],
    *,
    cwd: Path | None = None,
    timeout: int,
    check: bool = True,
) -> CommandResult:
    started = time.perf_counter()
    proc = subprocess.run(
        args,
        cwd=str(cwd) if cwd else None,
        text=True,
        capture_output=True,
        timeout=timeout,
    )
    elapsed_ms = int((time.perf_counter() - started) * 1000)
    result = CommandResult(
        args=args,
        returncode=proc.returncode,
        stdout=proc.stdout,
        stderr=proc.stderr,
        elapsed_ms=elapsed_ms,
    )
    if check and proc.returncode != 0:
        raise CommandFailed(result)
    return result


def repo_key(repo: str) -> str:
    return repo.replace("/", "__").replace(".", "_")


def tail(text: str, limit: int = 1600) -> str:
    text = text.strip()
    if len(text) <= limit:
        return text
    return text[-limit:]


def parse_budgets(value: str) -> list[int]:
    budgets = []
    for part in value.split(","):
        part = part.strip()
        if not part:
            continue
        budgets.append(int(part))
    if not budgets:
        raise argparse.ArgumentTypeError("at least one budget is required")
    return budgets


def clone_or_reuse(
    repo: dict[str, str],
    repo_dir: Path,
    *,
    reuse_repos: bool,
    clone_timeout: int,
) -> tuple[str, int]:
    if repo_dir.exists() and (repo_dir / ".git").is_dir() and reuse_repos:
        return "reused", 0
    if repo_dir.exists():
        shutil.rmtree(repo_dir)
    result = run_cmd(
        ["git", "clone", "--depth", "1", repo["url"], str(repo_dir)],
        timeout=clone_timeout,
    )
    return "cloned", result.elapsed_ms


def git_metadata(repo_dir: Path) -> dict[str, str]:
    def git(*args: str) -> str:
        return run_cmd(["git", *args], cwd=repo_dir, timeout=30).stdout.strip()

    branch = git("branch", "--show-current")
    return {
        "commit": git("rev-parse", "HEAD"),
        "branch": branch or git("rev-parse", "--abbrev-ref", "HEAD"),
        "origin": git("remote", "get-url", "origin"),
    }


def read_index_state(repo_dir: Path) -> dict[str, Any]:
    db_path = repo_dir / ".prefixgraph" / "index.db"
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    try:
        meta = {
            row["key"]: row["value"]
            for row in conn.execute("SELECT key, value FROM meta")
        }
        report = json.loads(meta.get("last_index_report", "{}"))
        counts = {
            "files": scalar(conn, "SELECT COUNT(*) FROM files WHERE skipped IS NULL"),
            "skipped_files": scalar(
                conn, "SELECT COUNT(*) FROM files WHERE skipped IS NOT NULL"
            ),
            "chunks": scalar(conn, "SELECT COUNT(*) FROM nodes WHERE valid=1 AND kind=2"),
            "signatures": scalar(
                conn, "SELECT COUNT(*) FROM nodes WHERE valid=1 AND kind=3"
            ),
            "docs": scalar(conn, "SELECT COUNT(*) FROM nodes WHERE valid=1 AND kind=4"),
            "edges": scalar(conn, "SELECT COUNT(*) FROM edges"),
        }
        top = conn.execute(
            """
            SELECT path, symbol, tokens, centrality
            FROM nodes
            WHERE valid=1 AND kind=2 AND symbol IS NOT NULL AND role != 'test'
            ORDER BY centrality DESC, tokens DESC, path ASC
            LIMIT 1
            """
        ).fetchone()
        if top is None:
            top = conn.execute(
                """
                SELECT path, symbol, tokens, centrality
                FROM nodes
                WHERE valid=1 AND kind=2
                ORDER BY centrality DESC, tokens DESC, path ASC
                LIMIT 1
                """
            ).fetchone()
        lang_counts = {
            (row["lang"] or "unknown"): row["count"]
            for row in conn.execute(
                """
                SELECT COALESCE(lang, 'unknown') AS lang, COUNT(*) AS count
                FROM nodes
                WHERE valid=1 AND kind=2
                GROUP BY COALESCE(lang, 'unknown')
                ORDER BY count DESC
                """
            )
        }
        return {
            "index_report": report,
            "counts": counts,
            "top_symbol": dict(top) if top else None,
            "lang_counts": lang_counts,
        }
    finally:
        conn.close()


def scalar(conn: sqlite3.Connection, query: str) -> int:
    row = conn.execute(query).fetchone()
    return int(row[0] if row else 0)


def query_set(top_symbol: dict[str, Any] | None) -> list[dict[str, str]]:
    queries = list(BASE_QUERIES)
    if top_symbol and top_symbol.get("symbol") and top_symbol.get("path"):
        queries.append(
            {
                "name": "top_symbol",
                "query": (
                    f"Explain {top_symbol['symbol']} in {top_symbol['path']} "
                    "and its important dependencies."
                ),
            }
        )
    elif top_symbol and top_symbol.get("path"):
        queries.append(
            {
                "name": "top_file",
                "query": f"Explain {top_symbol['path']} and its important dependencies.",
            }
        )
    else:
        queries.append(
            {
                "name": "top_symbol",
                "query": "Explain the most central symbol and its important dependencies.",
            }
        )
    return queries


def summarize_pack(pack: dict[str, Any]) -> dict[str, Any]:
    items = pack.get("items", [])
    type_counts = Counter(item.get("type", "unknown") for item in items)
    reason_counts = Counter(
        item.get("why", {}).get("reason", "unknown") for item in items
    )
    scores = [
        item.get("why", {}).get("score")
        for item in items
        if isinstance(item.get("why", {}).get("score"), (int, float))
    ]
    paths = {item.get("path", "") for item in items if item.get("path")}
    totals = pack.get("totals", {})
    layout = pack.get("layout", {})
    freshness = pack.get("freshness", {})
    return {
        "pack_id": pack.get("pack_id", ""),
        "item_count": len(items),
        "path_count": len(paths),
        "selected_tokens": totals.get("selected_tokens", 0),
        "estimated_raw_tokens": totals.get("estimated_raw_tokens", 0),
        "saved_tokens": totals.get("saved_tokens", 0),
        "saved_pct": totals.get("saved_pct", 0),
        "stable_prefix_count": len(layout.get("stable_prefix_items", [])),
        "hot_set_version": layout.get("hot_set_version", 0),
        "freshness_state": freshness.get("state", ""),
        "stale_files": freshness.get("stale_files", 0),
        "tokenizer": pack.get("tokenizer", ""),
        "token_estimate": pack.get("token_estimate", False),
        "signature_items": type_counts.get("signature", 0),
        "ast_chunk_items": type_counts.get("ast_chunk", 0),
        "test_items": type_counts.get("test", 0),
        "doc_chunk_items": type_counts.get("doc_chunk", 0),
        "anchor_items": reason_counts.get("anchor", 0),
        "search_hit_items": reason_counts.get("search_hit", 0),
        "graph_edge_items": sum(
            count
            for reason, count in reason_counts.items()
            if reason
            not in {
                "anchor",
                "search_hit",
            }
        ),
        "avg_score": round(sum(scores) / len(scores), 4) if scores else "",
        "max_score": max(scores) if scores else "",
        "type_counts_json": json.dumps(type_counts, sort_keys=True),
        "reason_counts_json": json.dumps(reason_counts, sort_keys=True),
    }


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row, sort_keys=True))
            f.write("\n")


def write_csv(path: Path, rows: list[dict[str, Any]], preferred: list[str]) -> None:
    keys = list(preferred)
    seen = set(keys)
    for row in rows:
        for key in row:
            if key not in seen:
                keys.append(key)
                seen.add(key)
    with path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=keys)
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def flatten_index_row(
    repo: dict[str, str],
    metadata: dict[str, str],
    state: dict[str, Any],
    clone_action: str,
    clone_ms: int,
    index_wall_ms: int,
) -> dict[str, Any]:
    report = state["index_report"]
    counts = state["counts"]
    top = state["top_symbol"] or {}
    row = {
        "repo": repo["repo"],
        "repo_key": repo_key(repo["repo"]),
        "url": repo["url"],
        "primary_language": repo["language"],
        "commit": metadata.get("commit", ""),
        "branch": metadata.get("branch", ""),
        "clone_action": clone_action,
        "clone_ms": clone_ms,
        "index_wall_ms": index_wall_ms,
        "files_seen": report.get("files_seen", 0),
        "files_indexed": report.get("files_indexed", 0),
        "files_unchanged": report.get("files_unchanged", 0),
        "files_deleted": report.get("files_deleted", 0),
        "chunks_new": report.get("chunks_new", 0),
        "chunks_preserved": report.get("chunks_preserved", 0),
        "chunks_staled": report.get("chunks_staled", 0),
        "edges_added": report.get("edges_added", 0),
        "index_report_duration_ms": report.get("duration_ms", 0),
        "skipped_count": len(report.get("skipped", [])),
        "hot_set_version": report.get("hot_set_version", 0),
        "count_files": counts.get("files", 0),
        "count_skipped_files": counts.get("skipped_files", 0),
        "count_chunks": counts.get("chunks", 0),
        "count_signatures": counts.get("signatures", 0),
        "count_docs": counts.get("docs", 0),
        "count_edges": counts.get("edges", 0),
        "top_symbol": top.get("symbol", ""),
        "top_symbol_path": top.get("path", ""),
        "top_symbol_tokens": top.get("tokens", ""),
        "top_symbol_centrality": top.get("centrality", ""),
        "lang_counts_json": json.dumps(state["lang_counts"], sort_keys=True),
    }
    return row


def failure_row(
    repo: dict[str, str],
    stage: str,
    error: Exception,
    *,
    query_name: str = "",
    budget: int | str = "",
) -> dict[str, Any]:
    row = {
        "repo": repo["repo"],
        "repo_key": repo_key(repo["repo"]),
        "url": repo["url"],
        "primary_language": repo["language"],
        "stage": stage,
        "query_name": query_name,
        "budget": budget,
        "error": str(error),
    }
    if isinstance(error, CommandFailed):
        row.update(
            {
                "returncode": error.result.returncode,
                "stderr_tail": tail(error.result.stderr),
                "stdout_tail": tail(error.result.stdout),
                "command": " ".join(error.result.args),
                "elapsed_ms": error.result.elapsed_ms,
            }
        )
    return row


def make_report(
    *,
    run_dir: Path,
    started_at: str,
    finished_at: str,
    budgets: list[int],
    index_rows: list[dict[str, Any]],
    pack_rows: list[dict[str, Any]],
    failures: list[dict[str, Any]],
) -> str:
    ok_pack_rows = [row for row in pack_rows if row.get("status") == "ok"]
    total_index_ms = sum(int(row.get("index_wall_ms", 0)) for row in index_rows)
    total_files = sum(int(row.get("count_files", 0)) for row in index_rows)
    total_chunks = sum(int(row.get("count_chunks", 0)) for row in index_rows)

    lines = [
        "# PrefixGraph GitHub 20-Repo Eval",
        "",
        f"- Started: `{started_at}`",
        f"- Finished: `{finished_at}`",
        f"- Repos indexed: `{len(index_rows)}` / `{len(REPOS)}`",
        f"- Pack runs completed: `{len(ok_pack_rows)}`",
        f"- Failures recorded: `{len(failures)}`",
        f"- Budgets: `{', '.join(str(b) for b in budgets)}`",
        f"- Total indexed files: `{total_files}`",
        f"- Total AST chunks: `{total_chunks}`",
        f"- Total index wall time: `{round(total_index_ms / 1000, 2)}s`",
        "",
        "## By Budget",
        "",
        "| budget | runs | median selected tok | median raw tok | median saved % | median items | median stable prefix |",
        "| ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    by_budget: dict[int, list[dict[str, Any]]] = defaultdict(list)
    for row in ok_pack_rows:
        by_budget[int(row["budget"])].append(row)
    for budget in budgets:
        rows = by_budget.get(budget, [])
        lines.append(
            "| "
            + " | ".join(
                [
                    str(budget),
                    str(len(rows)),
                    fmt_median(rows, "selected_tokens"),
                    fmt_median(rows, "estimated_raw_tokens"),
                    fmt_median(rows, "saved_pct"),
                    fmt_median(rows, "item_count"),
                    fmt_median(rows, "stable_prefix_count"),
                ]
            )
            + " |"
        )

    lines.extend(
        [
            "",
            "## By Query",
            "",
            "| query | runs | median selected tok | median saved % | median signatures | median anchors |",
            "| --- | ---: | ---: | ---: | ---: | ---: |",
        ]
    )
    by_query: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for row in ok_pack_rows:
        by_query[str(row["query_name"])].append(row)
    for name in sorted(by_query):
        rows = by_query[name]
        lines.append(
            "| "
            + " | ".join(
                [
                    name,
                    str(len(rows)),
                    fmt_median(rows, "selected_tokens"),
                    fmt_median(rows, "saved_pct"),
                    fmt_median(rows, "signature_items"),
                    fmt_median(rows, "anchor_items"),
                ]
            )
            + " |"
        )

    lines.extend(
        [
            "",
            "## Indexed Repositories",
            "",
            "| repo | commit | files | chunks | edges | index wall ms |",
            "| --- | --- | ---: | ---: | ---: | ---: |",
        ]
    )
    for row in index_rows:
        lines.append(
            "| "
            + " | ".join(
                [
                    str(row["repo"]),
                    str(row["commit"])[:12],
                    str(row["count_files"]),
                    str(row["count_chunks"]),
                    str(row["count_edges"]),
                    str(row["index_wall_ms"]),
                ]
            )
            + " |"
        )

    if failures:
        lines.extend(
            [
                "",
                "## Failures",
                "",
                "| repo | stage | query | budget | error |",
                "| --- | --- | --- | ---: | --- |",
            ]
        )
        for row in failures[:30]:
            err = str(row.get("error", "")).replace("|", "\\|")
            lines.append(
                f"| {row.get('repo', '')} | {row.get('stage', '')} | "
                f"{row.get('query_name', '')} | {row.get('budget', '')} | {err} |"
            )
    lines.extend(
        [
            "",
            "## Files",
            "",
            f"- Raw pack rows: `{run_dir / 'pack_metrics.jsonl'}`",
            f"- Pack CSV: `{run_dir / 'pack_metrics.csv'}`",
            f"- Index CSV: `{run_dir / 'repo_index_metrics.csv'}`",
            f"- Manifest: `{run_dir / 'manifest.json'}`",
            f"- Failures: `{run_dir / 'failures.jsonl'}`",
            "",
        ]
    )
    return "\n".join(lines)


def fmt_median(rows: list[dict[str, Any]], key: str) -> str:
    values = []
    for row in rows:
        value = row.get(key)
        if isinstance(value, (int, float)):
            values.append(value)
        elif isinstance(value, str) and value:
            try:
                values.append(float(value))
            except ValueError:
                pass
    if not values:
        return ""
    value = median(values)
    if abs(value - int(value)) < 0.0001:
        return str(int(value))
    return f"{value:.2f}"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        type=Path,
        default=Path("target/release/prefixgraph"),
        help="PrefixGraph CLI binary to run.",
    )
    parser.add_argument(
        "--workdir",
        type=Path,
        default=Path("/private/tmp/prefixgraph-github-eval"),
        help="Directory used for cloned repositories.",
    )
    parser.add_argument(
        "--results-dir",
        type=Path,
        default=Path("eval/results"),
        help="Directory where a timestamped result run will be written.",
    )
    parser.add_argument(
        "--budgets",
        type=parse_budgets,
        default=DEFAULT_BUDGETS,
        help="Comma-separated token budgets.",
    )
    parser.add_argument(
        "--repo-limit",
        type=int,
        default=0,
        help="Limit the run to the first N repos for smoke tests.",
    )
    parser.add_argument(
        "--no-reuse-repos",
        action="store_true",
        help="Delete and re-clone repos even when a prior clone exists.",
    )
    parser.add_argument("--clone-timeout", type=int, default=240)
    parser.add_argument("--index-timeout", type=int, default=300)
    parser.add_argument("--pack-timeout", type=int, default=120)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    binary = args.binary.resolve()
    if not binary.exists():
        print(f"missing PrefixGraph binary: {binary}", file=sys.stderr)
        return 2

    started_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    run_slug = datetime.now(timezone.utc).strftime("github_20_%Y%m%dT%H%M%SZ")
    run_dir = args.results_dir / run_slug
    run_dir.mkdir(parents=True, exist_ok=False)
    args.workdir.mkdir(parents=True, exist_ok=True)

    selected_repos = REPOS[: args.repo_limit] if args.repo_limit else REPOS
    manifest: dict[str, Any] = {
        "started_at": started_at,
        "prefixgraph_binary": str(binary),
        "prefixgraph_version": run_cmd(
            [str(binary), "--version"], timeout=30
        ).stdout.strip(),
        "workdir": str(args.workdir),
        "budgets": args.budgets,
        "queries": BASE_QUERIES + [{"name": "top_symbol", "query": "repo-specific"}],
        "repos": selected_repos,
    }

    index_rows: list[dict[str, Any]] = []
    pack_rows: list[dict[str, Any]] = []
    failures: list[dict[str, Any]] = []

    for number, repo in enumerate(selected_repos, start=1):
        key = repo_key(repo["repo"])
        repo_dir = args.workdir / "repos" / key
        print(f"[{number}/{len(selected_repos)}] {repo['repo']}", flush=True)
        try:
            clone_action, clone_ms = clone_or_reuse(
                repo,
                repo_dir,
                reuse_repos=not args.no_reuse_repos,
                clone_timeout=args.clone_timeout,
            )
            metadata = git_metadata(repo_dir)
            state_dir = repo_dir / ".prefixgraph"
            if state_dir.exists():
                shutil.rmtree(state_dir)
            run_cmd([str(binary), "init", str(repo_dir)], timeout=60)
            index_result = run_cmd(
                [str(binary), "--repo", str(repo_dir), "index", "--force"],
                timeout=args.index_timeout,
            )
            state = read_index_state(repo_dir)
            index_rows.append(
                flatten_index_row(
                    repo,
                    metadata,
                    state,
                    clone_action,
                    clone_ms,
                    index_result.elapsed_ms,
                )
            )
        except Exception as exc:  # keep the corpus moving
            failures.append(failure_row(repo, "index", exc))
            print(f"  index failed: {exc}", flush=True)
            continue

        for query in query_set(state["top_symbol"]):
            for budget in args.budgets:
                try:
                    pack_result = run_cmd(
                        [
                            str(binary),
                            "--repo",
                            str(repo_dir),
                            "pack",
                            query["query"],
                            "--budget",
                            str(budget),
                            "--json",
                            "--no-content",
                        ],
                        timeout=args.pack_timeout,
                    )
                    pack = json.loads(pack_result.stdout)
                    row = {
                        "status": "ok",
                        "repo": repo["repo"],
                        "repo_key": key,
                        "url": repo["url"],
                        "primary_language": repo["language"],
                        "commit": metadata.get("commit", ""),
                        "branch": metadata.get("branch", ""),
                        "query_name": query["name"],
                        "query": query["query"],
                        "budget": budget,
                        "pack_wall_ms": pack_result.elapsed_ms,
                    }
                    row.update(summarize_pack(pack))
                    pack_rows.append(row)
                except Exception as exc:
                    failures.append(
                        failure_row(
                            repo,
                            "pack",
                            exc,
                            query_name=query["name"],
                            budget=budget,
                        )
                    )
                    print(f"  pack failed ({query['name']} {budget}): {exc}", flush=True)

    finished_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    manifest["finished_at"] = finished_at
    manifest["run_dir"] = str(run_dir)

    (run_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    write_jsonl(run_dir / "pack_metrics.jsonl", pack_rows)
    write_jsonl(run_dir / "failures.jsonl", failures)
    write_csv(
        run_dir / "repo_index_metrics.csv",
        index_rows,
        [
            "repo",
            "repo_key",
            "primary_language",
            "commit",
            "branch",
            "clone_action",
            "clone_ms",
            "index_wall_ms",
            "files_seen",
            "count_files",
            "count_chunks",
            "count_signatures",
            "count_docs",
            "count_edges",
            "skipped_count",
            "top_symbol",
            "top_symbol_path",
        ],
    )
    write_csv(
        run_dir / "pack_metrics.csv",
        pack_rows,
        [
            "repo",
            "repo_key",
            "primary_language",
            "commit",
            "query_name",
            "budget",
            "pack_wall_ms",
            "item_count",
            "path_count",
            "selected_tokens",
            "estimated_raw_tokens",
            "saved_tokens",
            "saved_pct",
            "stable_prefix_count",
            "signature_items",
            "ast_chunk_items",
            "test_items",
            "doc_chunk_items",
            "anchor_items",
            "search_hit_items",
            "graph_edge_items",
        ],
    )
    report = make_report(
        run_dir=run_dir,
        started_at=started_at,
        finished_at=finished_at,
        budgets=args.budgets,
        index_rows=index_rows,
        pack_rows=pack_rows,
        failures=failures,
    )
    (run_dir / "report.md").write_text(report, encoding="utf-8")
    print(f"wrote {run_dir}", flush=True)
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
