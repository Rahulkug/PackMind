# PackMind GitHub 20-Repo Eval

- Started: `2026-06-12T16:30:42Z`
- Finished: `2026-06-12T16:33:29Z`
- Repos indexed: `20` / `20`
- Pack runs completed: `180`
- Failures recorded: `0`
- Budgets: `2000, 6000, 12000`
- Total indexed files: `3867`
- Total AST chunks: `5426`
- Total index wall time: `21.48s`

## By Budget

| budget | runs | median selected tok | median raw tok | median saved % | median items | median stable prefix |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2000 | 60 | 1995.50 | 27291 | 92.67 | 34.50 | 1 |
| 6000 | 60 | 5991.50 | 34040.50 | 82.38 | 67.50 | 0 |
| 12000 | 60 | 11980.50 | 37794.50 | 68.64 | 91.50 | 0 |

## By Query

| query | runs | median selected tok | median saved % | median signatures | median anchors |
| --- | ---: | ---: | ---: | ---: | ---: |
| architecture | 60 | 5832 | 82.48 | 2 | 0 |
| tests | 60 | 5905.50 | 81.75 | 2 | 0 |
| top_symbol | 60 | 5965 | 82.53 | 2 | 3 |

## Indexed Repositories

| repo | commit | files | chunks | edges | index wall ms |
| --- | --- | ---: | ---: | ---: | ---: |
| pypa/sampleproject | 621e4974ca25 | 9 | 6 | 4 | 168 |
| pallets/itsdangerous | 672971d66a2e | 35 | 43 | 108 | 151 |
| pallets/click | 8a1b1a33d739 | 130 | 835 | 3218 | 1506 |
| psf/requests | 6f66281a1d63 | 96 | 218 | 574 | 605 |
| pytest-dev/pluggy | e2c9b6f101ee | 64 | 203 | 228 | 259 |
| encode/httpcore | 10a658221deb | 84 | 277 | 1738 | 543 |
| tiangolo/typer | dacdc4349df2 | 742 | 1446 | 3646 | 3320 |
| sindresorhus/is | 7821031c66cd | 11 | 311 | 329 | 322 |
| sindresorhus/ky | 61d6d66d2791 | 58 | 122 | 137 | 374 |
| ai/nanoid | 78f4a02cbef7 | 37 | 30 | 220 | 278 |
| tj/commander.js | ba6d13ddb424 | 205 | 74 | 154 | 503 |
| expressjs/express | dae209ae6559 | 201 | 54 | 20 | 323 |
| axios/axios | 2d06f96e8602 | 412 | 352 | 1343 | 2108 |
| chalk/chalk | aa06bb5ac3f1 | 25 | 41 | 25 | 167 |
| google/gson | 004e7a4949e0 | 295 | 257 | 2099 | 1926 |
| junit-team/junit4 | 300468b1efd4 | 540 | 450 | 2931 | 2454 |
| apache/commons-cli | f9a0a7fd2db7 | 125 | 89 | 312 | 532 |
| apache/commons-csv | 19b29139dfdb | 106 | 54 | 135 | 468 |
| apache/commons-io | f729612edfca | 646 | 559 | 1397 | 5326 |
| spring-guides/gs-rest-service | e9efc9dfa0ab | 46 | 5 | 1 | 144 |

## Files

- Raw pack rows: `eval/results/packmind_20_20260612T163042Z/pack_metrics.jsonl`
- Pack CSV: `eval/results/packmind_20_20260612T163042Z/pack_metrics.csv`
- Index CSV: `eval/results/packmind_20_20260612T163042Z/repo_index_metrics.csv`
- Manifest: `eval/results/packmind_20_20260612T163042Z/manifest.json`
- Failures: `eval/results/packmind_20_20260612T163042Z/failures.jsonl`
- Provenance report: `eval/results/packmind_20_20260612T163042Z/provenance.md`
- Provenance JSON: `eval/results/packmind_20_20260612T163042Z/provenance.json`

## Provenance

This run was verified with:

```sh
scripts/verify_github_eval.py eval/results/packmind_20_20260612T163042Z
```

Verifier result: `PASS`, with `164` checks and `0` failed checks.

The verifier confirms that each indexed repository maps to a local `.git`
clone under `/private/tmp/packmind-github-eval/repos`, that each local
`git rev-parse HEAD` equals the commit recorded in `repo_index_metrics.csv`,
that each clone's `origin` matches the recorded GitHub URL, that each repo has
a PackMind SQLite index database, that database counts match the CSV, and that
every `pack_id` in `pack_metrics.jsonl` exists in the corresponding repo's
`packs` table.

Scope note: these are PackMind indexing and context-pack measurements on real
GitHub repositories. They are not an end-to-end LLM answer-quality eval.
