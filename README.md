# PackMind

**A context compiler for AI coding agents.** PackMind compiles your repository
into small, explained, cache-stable context packs under a token budget — so
agents read the right code, not the whole codebase.

It is local-first and works with Claude Code, Cursor, Cline, Continue, Aider,
and any custom MCP agent. Instead of dumping whole files into prompts, PackMind
indexes your repo locally and returns the exact symbols, callers, tests, docs,
and configs an agent should read — each with a reason it was included.

```text
$ packmind pack "fix currency rounding in payment validation" --mode bugfix --budget 6000

Selected 18 items, 1195 tokens.  Raw equivalent: ~1440 tokens.  Saved: 17.0%.
pack 01KV… · mode: bugfix · freshness: fresh · hot set v1
...
- payments.py::PaymentError                  [ast_chunk]  (search_hit 0.74)
    why: query term 'payment'
- auth.py::require_auth                       [ast_chunk]  (calls 0.39)
    why: calls edge from handle_payment_request
- tests/test_payments.py::validator          [test]       (tested_by 0.43)
    why: tested_by edge from process_payment

Sufficiency:  ✓ direct implementation  ✓ related tests  ✓ callers  ✓ docs/config
Pack risk: low
```

> On a six-file example repo the savings are small by design. On 20 real GitHub
> repositories the same planner saves a **median 92.9%** of context tokens at a
> 2,000-token budget — [see the verified eval below](#20-repo-github-evaluation).

See the whole thing for yourself in 30 seconds — no Python, no setup:

```sh
cargo build --release
target/release/packmind demo --repo examples/small-python-service --open
```

That indexes the repo, builds packs across modes and budgets, runs the
benchmarks, and opens a self-contained interactive HTML page (pack explorer,
code graph, cache report). Full walkthrough: [docs/USAGE.md](docs/USAGE.md);
editor/agent setup: [docs/integrations/](docs/integrations/).

![PackMind context pack screenshot](docs/assets/pack-screenshot.svg)

## The Problem

AI coding agents are often good at editing code once they have the right
context. The hard part is getting that context:

- dumping whole files wastes tokens and hides the important symbols;
- lexical search misses callers, tests, imports, and related declarations;
- stale context causes wrong answers during active editing;
- prompt caches only help when repeated context is ordered stably;
- most repo-context tools do not explain why a file or symbol was included.

PackMind treats repository context as persistent local infrastructure. It
builds a graph once, updates it incrementally, and returns compact packs with a
reason for every included item.

## What PackMind Does

Current `0.4.0` implementation:

- indexes Python, TypeScript/JavaScript, Java, Markdown, and text files;
- extracts AST chunks, signatures, docs, imports, calls, inheritance, and test
  relations where supported;
- stores everything locally in `.packmind/index.db` using SQLite + FTS5;
- builds context packs with token budgets and per-item explanations, plus a
  sufficiency check and a pack risk level so incomplete context is visible;
- biases retrieval per task with `--mode bugfix | refactor | test | security |
  architecture | pr` (bugfix/pr also anchor on files changed since the last
  index);
- reads scoring weights, default budget, and a prune threshold from
  `.packmind/config.toml` (written by `init`, every value optional);
- substitutes signatures when full chunks do not fit;
- orders selected items deterministically for prompt-cache stability;
- copies a ready-to-paste pack to the clipboard (`pack --copy`);
- builds PR-shaped review context (`pr-context`): changed symbols, impact, and
  a suggested pack;
- self-diagnoses setup (`doctor`) and renders a one-command interactive demo
  (`demo`);
- reports cache health (`cache-report`) and ships two reproducible offline
  benchmarks (`bench token-savings`, `bench cache-stability`);
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

## Install

Install the CLI with Cargo (Rust toolchain required):

```sh
cargo install --git https://github.com/Rahulkug/PackMind packmind-cli
```

This puts a `packmind` binary on your `PATH`. Or build from source:

```sh
git clone https://github.com/Rahulkug/PackMind && cd PackMind
cargo build --release   # binary at target/release/packmind
```

A one-line install script and prebuilt binaries are planned; for now Cargo is
the supported path. The examples below assume `packmind` is on your `PATH` (use
`target/release/packmind` if you built from source).

## Quick Start

Initialize and index a repo, then confirm the setup:

```sh
packmind init /path/to/repo
packmind --repo /path/to/repo index --force
packmind --repo /path/to/repo doctor
```

Check index state:

```sh
packmind --repo /path/to/repo status
```

Search code:

```sh
packmind --repo /path/to/repo search "payment validation"
```

Build a context pack:

```sh
packmind --repo /path/to/repo pack \
  "Explain the main architecture and important data flow" \
  --budget 6000 \
  --json
```

Build a task-biased pack (modes change scoring priors, not the contract):

```sh
packmind --repo /path/to/repo pack "fix the currency rounding bug" --mode bugfix
packmind --repo /path/to/repo pack "review token handling" --mode security
```

Copy a ready-to-paste pack straight to the clipboard:

```sh
packmind --repo /path/to/repo pack "fix login bug" --mode bugfix --copy
# -> Copied 5.8k-token context pack to the clipboard.
```

Get PR-shaped review context (changed symbols, impact, suggested pack):

```sh
packmind --repo /path/to/repo pr-context main..HEAD --budget 8000
```

Render for a prompt:

```sh
packmind --repo /path/to/repo pack \
  "Refactor PaymentValidator to use FxRateService" \
  --budget 12000 \
  --render plain
```

Check prompt-cache health and run the offline benchmarks:

```sh
packmind --repo /path/to/repo cache-report
packmind --repo /path/to/repo bench token-savings
packmind --repo /path/to/repo bench cache-stability
```

For screenshots, MCP setup, playground commands, and common workflows, see
[docs/USAGE.md](docs/USAGE.md). For editor/agent setup, see
[docs/integrations/](docs/integrations/).

## Interactive Demo

One command runs the full pipeline end to end — index, packs across modes and
budgets, cache-report, both benchmarks — and renders a single self-contained
HTML file. No Python, no server, no network; the template is embedded in the
binary:

```sh
packmind demo --repo /path/to/repo --open
```

The page shows three views:

- **Pack explorer** — switch between real packs; every item is clickable and
  shows its mandatory `why`, its code, and its token cost, next to a savings
  bar against the whole-file counterfactual.
- **Code graph** — the indexed AST graph (calls, imports, tests, doc
  mentions); selecting a pack highlights exactly which nodes it shipped.
- **Cache & benchmarks** — the cache-report numbers, per-query token savings,
  and the edit-replay steps proving the cacheable prefix survives normal
  editing.

(Maintainers can regenerate the checked-in `demo/packmind-demo.html` with the
richer example matrix via `python3 demo/generate.py`.)

## MCP Usage

Run the MCP server over stdio:

```sh
packmind --repo /path/to/repo mcp
```

Example MCP client config shape:

```json
{
  "mcpServers": {
    "packmind": {
      "command": "/absolute/path/to/packmind",
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
- `build_context_pack` (accepts an optional `mode`: bugfix, refactor, test,
  security, architecture, pr)
- `changed_since`
- `impact_analysis`
- `get_content`

## Context Pack Contract

A context pack is the main output of PackMind. It contains:

- the original query and the task mode it was planned under;
- repo and freshness metadata;
- selected items with path, symbol, line range, token count, and content;
- `why` explanations such as `anchor`, `search_hit`, `calls`, `called_by`,
  `tested_by`, or `doc_mention`;
- token accounting: selected tokens, raw-file counterfactual tokens, and
  estimated savings;
- stable-prefix metadata for cache-friendly rendering.

This lets an agent answer repo questions without reading many files one by one.

## Proof It Works

### End-to-End Run (v0.4.0, bundled example)

Every release is verified end to end against
`examples/small-python-service`: init → index → packs in every mode →
cache-report → both benchmarks (this is exactly what `packmind demo` runs).
Numbers from a clean run:

| Check | Result |
| --- | --- |
| Workspace tests (`cargo test --workspace`) | 22 / 22 pass |
| Token savings (24 bench packs @ 2,000 budget) | 26.2% median, 27.2% mean |
| Edit replay (`bench cache-stability`, temp copy) | hot prefix byte-identical in 3/3 edits, 100% chunk preservation |
| Stable prefix | 2,449 bytes, ~304 reusable tokens, hot set v1 |

(The savings figures look modest only because the example is six tiny files —
there is little to leave out. See the 20-repo numbers below for real scale:
median 92.9% saved at a 2,000-token budget.)

The test suite pins the contracts, not just the happy path: anchors named in
the query always ship as full chunks when budget allows; signature nodes can
enter a pack only via planner substitution; threshold pruning never drops
anchors; bugfix/pr modes anchor dirty files while default mode does not;
mode score boosts must be visible in the item's `why` (no invisible scoring);
planner output is deterministic for a fixed snapshot.

### 20-Repo GitHub Evaluation

The repository includes a reproducible 20-repo GitHub evaluation.

Clean run:

- Report: `eval/results/packmind_20_20260612T174255Z/report.md`
- Raw pack rows: `eval/results/packmind_20_20260612T174255Z/pack_metrics.jsonl`
- Pack CSV: `eval/results/packmind_20_20260612T174255Z/pack_metrics.csv`
- Index CSV: `eval/results/packmind_20_20260612T174255Z/repo_index_metrics.csv`
- Provenance: `eval/results/packmind_20_20260612T174255Z/provenance.md`
- Machine-readable provenance: `eval/results/packmind_20_20260612T174255Z/provenance.json`

That run indexed 20 real public GitHub repositories and generated 180 context
packs: 20 repos x 3 query profiles x 3 token budgets. It was produced by the
v0.3.0 binary; the v0.4.0 release changes only added commands (demo, doctor,
pr-context, --copy, scorecard) and leaves the indexer and planner byte-for-byte
identical, so these savings numbers carry forward unchanged.

Summary from the run:

| Metric | Value |
| --- | ---: |
| Repos indexed | 20 / 20 |
| Pack runs | 180 |
| Failures | 0 |
| Indexed files | 3,867 |
| AST chunks | 5,426 |
| Total index wall time | 19.83s |

Median pack savings by budget:

| Token budget | Runs | Median selected tokens | Median raw tokens | Median saved |
| ---: | ---: | ---: | ---: | ---: |
| 2,000 | 60 | 1,995.50 | 28,029.50 | 92.87% |
| 6,000 | 60 | 5,990 | 34,040.50 | 82.38% |
| 12,000 | 60 | 11,978.50 | 37,635 | 68.34% |

### Provenance Check

The eval is not just a hand-written report. It has a verifier:

```sh
scripts/verify_github_eval.py eval/results/packmind_20_20260612T174255Z
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
- every repo has a `.packmind/index.db`;
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
/private/tmp/packmind-github-eval/repos
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
3. PackMind stores file nodes, AST chunks, signatures, docs, and edges in
   SQLite.
4. Search combines explicit query anchors, FTS lexical hits, and a bounded graph
   walk.
5. The planner selects high-value items under a token budget, using signature
   substitution when needed.
6. The renderer emits stable, explained context suitable for an LLM prompt.

## Development

Run tests (22 across core, indexer, and the CLI integration suite):

```sh
cargo test --workspace
```

Useful commands:

```sh
cargo build --release
target/release/packmind --help
target/release/packmind --repo examples/small-python-service doctor
target/release/packmind --repo examples/small-python-service demo --open
```

The small example repo lives in `examples/small-python-service`.

## Current Limitations

- MCP is stdio-only today; streamable HTTP is not implemented yet.
- Optional embeddings are not implemented; lexical + graph retrieval is the
  current path.
- The eval measures context-pack compactness and provenance, not downstream LLM
  answer quality. An agent-replay benchmark (solve rate / tool calls with and
  without packs) is the planned next step.
- Current language support is intentionally narrow: Python, TypeScript/JS, and
  Java get AST extraction first.

## License

PackMind is licensed under the Apache License, Version 2.0. See `LICENSE`.

Copyright and attribution notices are in `NOTICE`.

The proof trail for the published work is in `PROVENANCE.md` and the verified
20-repo eval artifacts under `eval/results/packmind_20_20260612T174255Z`.
