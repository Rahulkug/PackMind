# PrefixGraph GitHub 20-Repo Eval

- Started: `2026-06-12T15:14:03Z`
- Finished: `2026-06-12T15:14:42Z`
- Repos indexed: `20` / `20`
- Pack runs completed: `180`
- Failures recorded: `0`
- Budgets: `2000, 6000, 12000`
- Total indexed files: `3867`
- Total AST chunks: `5426`
- Total index wall time: `19.4s`

## By Budget

| budget | runs | median selected tok | median raw tok | median saved % | median items | median stable prefix |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2000 | 60 | 1996 | 28186.50 | 92.91 | 34.50 | 1 |
| 6000 | 60 | 5991.50 | 34040.50 | 82.38 | 67.50 | 0 |
| 12000 | 60 | 11980.50 | 37821 | 68.40 | 91.50 | 0 |

## By Query

| query | runs | median selected tok | median saved % | median signatures | median anchors |
| --- | ---: | ---: | ---: | ---: | ---: |
| architecture | 60 | 5832 | 82.48 | 2 | 0 |
| tests | 60 | 5905.50 | 81.72 | 1.50 | 0 |
| top_symbol | 60 | 5965 | 82.53 | 1.50 | 3 |

## Indexed Repositories

| repo | commit | files | chunks | edges | index wall ms |
| --- | --- | ---: | ---: | ---: | ---: |
| pypa/sampleproject | 621e4974ca25 | 9 | 6 | 4 | 132 |
| pallets/itsdangerous | 672971d66a2e | 35 | 43 | 108 | 155 |
| pallets/click | 8a1b1a33d739 | 130 | 835 | 3218 | 1445 |
| psf/requests | 6f66281a1d63 | 96 | 218 | 574 | 616 |
| pytest-dev/pluggy | e2c9b6f101ee | 64 | 203 | 228 | 277 |
| encode/httpcore | 10a658221deb | 84 | 277 | 1738 | 555 |
| tiangolo/typer | dacdc4349df2 | 742 | 1446 | 3646 | 3379 |
| sindresorhus/is | 7821031c66cd | 11 | 311 | 329 | 291 |
| sindresorhus/ky | 61d6d66d2791 | 58 | 122 | 137 | 321 |
| ai/nanoid | 78f4a02cbef7 | 37 | 30 | 220 | 264 |
| tj/commander.js | ba6d13ddb424 | 205 | 74 | 154 | 447 |
| expressjs/express | dae209ae6559 | 201 | 54 | 20 | 336 |
| axios/axios | 2d06f96e8602 | 412 | 352 | 1343 | 2164 |
| chalk/chalk | aa06bb5ac3f1 | 25 | 41 | 25 | 157 |
| google/gson | 004e7a4949e0 | 295 | 257 | 2099 | 1525 |
| junit-team/junit4 | 300468b1efd4 | 540 | 450 | 2931 | 1901 |
| apache/commons-cli | f9a0a7fd2db7 | 125 | 89 | 312 | 524 |
| apache/commons-csv | 19b29139dfdb | 106 | 54 | 135 | 478 |
| apache/commons-io | f729612edfca | 646 | 559 | 1397 | 4317 |
| spring-guides/gs-rest-service | e9efc9dfa0ab | 46 | 5 | 1 | 119 |

## Files

- Raw pack rows: `eval/results/github_20_20260612T151403Z/pack_metrics.jsonl`
- Pack CSV: `eval/results/github_20_20260612T151403Z/pack_metrics.csv`
- Index CSV: `eval/results/github_20_20260612T151403Z/repo_index_metrics.csv`
- Manifest: `eval/results/github_20_20260612T151403Z/manifest.json`
- Failures: `eval/results/github_20_20260612T151403Z/failures.jsonl`
- Provenance report: `eval/results/github_20_20260612T151403Z/provenance.md`
- Provenance JSON: `eval/results/github_20_20260612T151403Z/provenance.json`

## Provenance

This run was verified with:

```sh
scripts/verify_github_eval.py eval/results/github_20_20260612T151403Z
```

Verifier result: `PASS`, with `164` checks and `0` failed checks.

The verifier confirms that each indexed repository maps to a local `.git`
clone under `/private/tmp/prefixgraph-github-eval/repos`, that each local
`git rev-parse HEAD` equals the commit recorded in `repo_index_metrics.csv`,
that each clone's `origin` matches the recorded GitHub URL, that each repo has
a PrefixGraph SQLite index database, that database counts match the CSV, and
that every `pack_id` in `pack_metrics.jsonl` exists in the corresponding
repo's `packs` table.

Scope note: these are PrefixGraph indexing and context-pack measurements on
real GitHub repositories. They are not an end-to-end LLM answer-quality eval.
