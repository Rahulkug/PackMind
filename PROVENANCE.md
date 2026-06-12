# PrefixGraph Provenance

This file records the public proof trail for the first published PrefixGraph
release.

## Public Repository

- Repository: `https://github.com/Rahulkug/PrefixGraph`
- Main commit published for the initial project import:
  `a5c0e6c46538ad00b571cb303ba3288a1b0a7bf7`
- Release tag: `v0.2.0`
- Release URL: `https://github.com/Rahulkug/PrefixGraph/releases/tag/v0.2.0`
- License: Apache-2.0, see `LICENSE`
- Notice: see `NOTICE`

## What Is Licensed Here

The following repository content is licensed under Apache-2.0 unless a file
states otherwise:

- Rust source code in `crates/`
- CLI/MCP/indexer/planner/core implementation
- documentation, including `README.md`
- scripts in `scripts/`
- example code in `examples/small-python-service/`
- PrefixGraph-generated evaluation reports and metrics in `eval/results/`

The evaluation artifacts do not include source code copied from the external
GitHub repositories used in the benchmark. Those repositories remain under
their own upstream licenses.

## Reproducible Evaluation Proof

The clean public evaluation run is:

```text
eval/results/github_20_20260612T151403Z
```

It contains:

- `report.md` - human-readable summary
- `manifest.json` - run inputs and corpus
- `repo_index_metrics.csv` - per-repo index metrics and exact commits
- `pack_metrics.csv` - per-pack metrics
- `pack_metrics.jsonl` - raw per-pack metric rows
- `provenance.md` - human-readable verification report
- `provenance.json` - machine-readable verification report
- `failures.jsonl` - empty for the clean run

Verification command:

```sh
scripts/verify_github_eval.py eval/results/github_20_20260612T151403Z
```

Verified result:

```text
PASS
checks: 164
failed_checks: 0
failure_rows: 0
index_rows: 20
pack_rows: 180
repo_proofs: 20
```

The verifier checks that each indexed repository maps to a local Git clone,
that each local `HEAD` matches the commit recorded in the CSV, that each clone's
origin matches the recorded GitHub URL, that each repository has a PrefixGraph
SQLite index database, that SQLite counts match the metrics CSV, and that each
recorded pack id exists in the corresponding database.

## Scope Of The Proof

This proof establishes that PrefixGraph was run against real GitHub repositories
and generated the included context-pack metrics. It does not claim an
end-to-end LLM answer-quality benchmark.

