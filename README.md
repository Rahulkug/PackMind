# PrefixGraph

PrefixGraph is a local-first context engine for AI coding agents.

It indexes a repository into an AST-aware graph, then builds small, explained,
token-budgeted context packs for coding questions. The goal is simple: stop
making agents rediscover the same repo structure by dumping files into every
prompt.

## The Problem

AI coding agents are often good at editing code once they have the right
context. The hard part is getting that context:

- dumping whole files wastes tokens and hides the important symbols;
- lexical search misses callers, tests, imports, and related declarations;
- stale context causes wrong answers during active editing;
- prompt caches only help when repeated context is ordered stably;
- most repo-context tools do not explain why a file or symbol was included.

PrefixGraph treats repository context as persistent local infrastructure. It
builds a graph once, updates it incrementally, and returns compact packs with a
reason for every included item.

## What PrefixGraph Does

Current `0.2.0` implementation:

- indexes Python, TypeScript/JavaScript, Java, Markdown, and text files;
- extracts AST chunks, signatures, docs, imports, calls, inheritance, and test
  relations where supported;
- stores everything locally in `.prefixgraph/index.db` using SQLite + FTS5;
- builds context packs with token budgets and per-item explanations;
- substitutes signatures when full chunks do not fit;
- orders selected items deterministically for prompt-cache stability;
- exposes a CLI and a read-only MCP stdio server.

It does not send code to a cloud service. The index is local to the repo.

## Status By Adoption Level

| Level | Surface | Status | Use it for |
| --- | --- | --- | --- |
| 1 | CLI | Implemented | Inspectable local indexing, search, and context packs |
| 2 | MCP stdio | Implemented | Claude Code / MCP clients with read-only repo tools |
| 2b | MCP streamable HTTP | Not implemented yet | Remote MCP clients |
| 3 | OpenAI/Anthropic-compatible proxy | Planned | Transparent context injection |
| 4 | Cache-aware gateway | Planned | Provider/local-model prefix-cache optimization |

Use it today at Level 1 or Level 2.

## Quick Start

Build from source:

```sh
cargo build --release
```

Initialize and index a repo:

```sh
target/release/prefixgraph init /path/to/repo
target/release/prefixgraph --repo /path/to/repo index --force
```

Check index state:

```sh
target/release/prefixgraph --repo /path/to/repo status
```

Search code:

```sh
target/release/prefixgraph --repo /path/to/repo search "payment validation"
```

Build a context pack:

```sh
target/release/prefixgraph --repo /path/to/repo pack \
  "Explain the main architecture and important data flow" \
  --budget 6000 \
  --json
```

Render for a prompt:

```sh
target/release/prefixgraph --repo /path/to/repo pack \
  "Refactor PaymentValidator to use FxRateService" \
  --budget 12000 \
  --render plain
```

## MCP Usage

Run the MCP server over stdio:

```sh
target/release/prefixgraph --repo /path/to/repo mcp
```

Example MCP client config shape:

```json
{
  "mcpServers": {
    "prefixgraph": {
      "command": "/absolute/path/to/prefixgraph",
      "args": ["--repo", "/absolute/path/to/repo", "mcp"]
    }
  }
}
```

The MCP tools are read-only:

- `search_code`
- `explain_symbol`
- `find_callers`
- `find_tests`
- `build_context_pack`
- `changed_since`
- `impact_analysis`
- `get_content`

## Context Pack Contract

A context pack is the main output of PrefixGraph. It contains:

- the original query;
- repo and freshness metadata;
- selected items with path, symbol, line range, token count, and content;
- `why` explanations such as `anchor`, `search_hit`, `calls`, `called_by`,
  `tested_by`, or `doc_mention`;
- token accounting: selected tokens, raw-file counterfactual tokens, and
  estimated savings;
- stable-prefix metadata for cache-friendly rendering.

This lets an agent answer repo questions without reading many files one by one.

## Proof It Works

The repository includes a reproducible 20-repo GitHub evaluation.

Clean run:

- Report: `eval/results/github_20_20260612T151403Z/report.md`
- Raw pack rows: `eval/results/github_20_20260612T151403Z/pack_metrics.jsonl`
- Pack CSV: `eval/results/github_20_20260612T151403Z/pack_metrics.csv`
- Index CSV: `eval/results/github_20_20260612T151403Z/repo_index_metrics.csv`
- Provenance: `eval/results/github_20_20260612T151403Z/provenance.md`
- Machine-readable provenance: `eval/results/github_20_20260612T151403Z/provenance.json`

That run indexed 20 real public GitHub repositories and generated 180 context
packs: 20 repos x 3 query profiles x 3 token budgets.

Summary from the run:

| Metric | Value |
| --- | ---: |
| Repos indexed | 20 / 20 |
| Pack runs | 180 |
| Failures | 0 |
| Indexed files | 3,867 |
| AST chunks | 5,426 |
| Total index wall time | 19.4s |

Median pack savings by budget:

| Token budget | Runs | Median selected tokens | Median raw tokens | Median saved |
| ---: | ---: | ---: | ---: | ---: |
| 2,000 | 60 | 1,996 | 28,186.50 | 92.91% |
| 6,000 | 60 | 5,991.50 | 34,040.50 | 82.38% |
| 12,000 | 60 | 11,980.50 | 37,821 | 68.40% |

### Provenance Check

The eval is not just a hand-written report. It has a verifier:

```sh
scripts/verify_github_eval.py eval/results/github_20_20260612T151403Z
```

Current verifier result:

```text
PASS
checks: 164
failed_checks: 0
index_rows: 20
pack_rows: 180
repo_proofs: 20
failure_rows: 0
```

The verifier checks that:

- every indexed repo maps to a local `.git` clone;
- every local `git rev-parse HEAD` equals the commit in
  `repo_index_metrics.csv`;
- every local `origin` equals the recorded GitHub URL;
- every repo has a `.prefixgraph/index.db`;
- SQLite file/chunk/doc/edge counts match the CSV metrics;
- every `pack_id` in `pack_metrics.jsonl` exists in that repo's `packs` table.

Scope note: this proves real-repo indexing and context-pack generation. It is
not yet an end-to-end LLM answer-quality benchmark.

## Reproduce The GitHub Eval

Build the release binary first:

```sh
cargo build --release
```

Run the benchmark:

```sh
scripts/eval_github_repos.py
```

This clones or reuses the corpus under:

```text
/private/tmp/prefixgraph-github-eval/repos
```

It writes a timestamped result directory under:

```text
eval/results/
```

Verify a result directory:

```sh
scripts/verify_github_eval.py eval/results/<run-id>
```

For a smaller smoke test:

```sh
scripts/eval_github_repos.py --repo-limit 1
```

## How It Works

1. The indexer walks the repo, respecting gitignore-style exclusions.
2. Tree-sitter extracts top-level declarations for supported languages.
3. PrefixGraph stores file nodes, AST chunks, signatures, docs, and edges in
   SQLite.
4. Search combines explicit query anchors, FTS lexical hits, and a bounded graph
   walk.
5. The planner selects high-value items under a token budget, using signature
   substitution when needed.
6. The renderer emits stable, explained context suitable for an LLM prompt.

## Development

Run tests:

```sh
cargo test --workspace
```

Useful commands:

```sh
cargo build --release
target/release/prefixgraph --help
target/release/prefixgraph --repo examples/small-python-service status
```

The small example repo lives in `examples/small-python-service`.

## Current Limitations

- MCP is stdio-only today; streamable HTTP is not implemented yet.
- Optional embeddings are not implemented; lexical + graph retrieval is the
  current path.
- There is no top-level config file support yet; scoring weights are compiled
  into the planner.
- The eval measures context-pack compactness and provenance, not downstream LLM
  answer quality.
- Current language support is intentionally narrow: Python, TypeScript/JS, and
  Java get AST extraction first.

## License

PrefixGraph is licensed under the Apache License, Version 2.0. See `LICENSE`.

Copyright and attribution notices are in `NOTICE`.

The proof trail for the published work is in `PROVENANCE.md` and the verified
20-repo eval artifacts under `eval/results/github_20_20260612T151403Z`.
