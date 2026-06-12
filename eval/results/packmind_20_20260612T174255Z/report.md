# PackMind GitHub 20-Repo Eval

- Started: `2026-06-12T17:42:55Z`
- Finished: `2026-06-12T17:43:34Z`
- Repos indexed: `20` / `20`
- Pack runs completed: `180`
- Failures recorded: `0`
- Budgets: `2000, 6000, 12000`
- Total indexed files: `3867`
- Total AST chunks: `5426`
- Total index wall time: `19.83s`

## By Budget

| budget | runs | median selected tok | median raw tok | median saved % | median items | median stable prefix |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2000 | 60 | 1995.50 | 28029.50 | 92.87 | 34.50 | 1 |
| 6000 | 60 | 5990 | 34040.50 | 82.38 | 67.50 | 0 |
| 12000 | 60 | 11978.50 | 37635 | 68.34 | 91.50 | 0 |

## By Query

| query | runs | median selected tok | median saved % | median signatures | median anchors |
| --- | ---: | ---: | ---: | ---: | ---: |
| architecture | 60 | 5832 | 82.48 | 2 | 0 |
| tests | 60 | 5905.50 | 81.89 | 2 | 0 |
| top_symbol | 60 | 5970.50 | 82.53 | 2 | 3 |

## Indexed Repositories

| repo | commit | files | chunks | edges | index wall ms |
| --- | --- | ---: | ---: | ---: | ---: |
| pypa/sampleproject | 621e4974ca25 | 9 | 6 | 4 | 119 |
| pallets/itsdangerous | 672971d66a2e | 35 | 43 | 108 | 158 |
| pallets/click | 8a1b1a33d739 | 130 | 835 | 3218 | 1502 |
| psf/requests | 6f66281a1d63 | 96 | 218 | 574 | 624 |
| pytest-dev/pluggy | e2c9b6f101ee | 64 | 203 | 228 | 277 |
| encode/httpcore | 10a658221deb | 84 | 277 | 1738 | 554 |
| tiangolo/typer | dacdc4349df2 | 742 | 1446 | 3646 | 3442 |
| sindresorhus/is | 7821031c66cd | 11 | 311 | 329 | 293 |
| sindresorhus/ky | 61d6d66d2791 | 58 | 122 | 137 | 336 |
| ai/nanoid | 78f4a02cbef7 | 37 | 30 | 220 | 207 |
| tj/commander.js | ba6d13ddb424 | 205 | 74 | 154 | 481 |
| expressjs/express | dae209ae6559 | 201 | 54 | 20 | 347 |
| axios/axios | 2d06f96e8602 | 412 | 352 | 1343 | 2169 |
| chalk/chalk | aa06bb5ac3f1 | 25 | 41 | 25 | 163 |
| google/gson | 004e7a4949e0 | 295 | 257 | 2099 | 1653 |
| junit-team/junit4 | 300468b1efd4 | 540 | 450 | 2931 | 2001 |
| apache/commons-cli | f9a0a7fd2db7 | 125 | 89 | 312 | 529 |
| apache/commons-csv | 19b29139dfdb | 106 | 54 | 135 | 444 |
| apache/commons-io | f729612edfca | 646 | 559 | 1397 | 4406 |
| spring-guides/gs-rest-service | e9efc9dfa0ab | 46 | 5 | 1 | 122 |

## Files

- Raw pack rows: `eval/results/packmind_20_20260612T174255Z/pack_metrics.jsonl`
- Pack CSV: `eval/results/packmind_20_20260612T174255Z/pack_metrics.csv`
- Index CSV: `eval/results/packmind_20_20260612T174255Z/repo_index_metrics.csv`
- Manifest: `eval/results/packmind_20_20260612T174255Z/manifest.json`
- Failures: `eval/results/packmind_20_20260612T174255Z/failures.jsonl`
- Provenance report: `eval/results/packmind_20_20260612T174255Z/provenance.md`
- Provenance JSON: `eval/results/packmind_20_20260612T174255Z/provenance.json`

## Provenance

After the run completes, verify it with:

```sh
scripts/verify_github_eval.py eval/results/packmind_20_20260612T174255Z
```

The verifier checks local git clones, recorded commits, GitHub origins,
PackMind SQLite index counts, and that every recorded pack id exists in
the corresponding repo database.
