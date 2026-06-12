# GitHub Eval Provenance

- Status: `PASS`
- Verified at: `2026-06-12T15:22:38Z`
- Run directory: `eval/results/github_20_20260612T151403Z`
- Clone workdir: `/private/tmp/prefixgraph-github-eval`
- Indexed repos: `20`
- Pack rows: `180`
- Failure rows: `0`
- Failed checks: `0`

## What Was Verified

- Each row in `repo_index_metrics.csv` maps to a local directory containing `.git`.
- `git rev-parse HEAD` in each clone equals the recorded commit.
- `git remote get-url origin` equals the recorded GitHub URL.
- Each clone has `.prefixgraph/index.db` from the eval run.
- SQLite node/file/edge counts match `repo_index_metrics.csv`.
- Every `pack_id` in `pack_metrics.jsonl` exists in that repo's `packs` table.

## Repositories

| repo | commit | local HEAD | origin | packs in DB | status |
| --- | --- | --- | --- | ---: | --- |
| pypa/sampleproject | 621e4974ca25 | 621e4974ca25 | https://github.com/pypa/sampleproject.git | 9 | PASS |
| pallets/itsdangerous | 672971d66a2e | 672971d66a2e | https://github.com/pallets/itsdangerous.git | 9 | PASS |
| pallets/click | 8a1b1a33d739 | 8a1b1a33d739 | https://github.com/pallets/click.git | 9 | PASS |
| psf/requests | 6f66281a1d63 | 6f66281a1d63 | https://github.com/psf/requests.git | 9 | PASS |
| pytest-dev/pluggy | e2c9b6f101ee | e2c9b6f101ee | https://github.com/pytest-dev/pluggy.git | 9 | PASS |
| encode/httpcore | 10a658221deb | 10a658221deb | https://github.com/encode/httpcore.git | 9 | PASS |
| tiangolo/typer | dacdc4349df2 | dacdc4349df2 | https://github.com/tiangolo/typer.git | 9 | PASS |
| sindresorhus/is | 7821031c66cd | 7821031c66cd | https://github.com/sindresorhus/is.git | 9 | PASS |
| sindresorhus/ky | 61d6d66d2791 | 61d6d66d2791 | https://github.com/sindresorhus/ky.git | 9 | PASS |
| ai/nanoid | 78f4a02cbef7 | 78f4a02cbef7 | https://github.com/ai/nanoid.git | 9 | PASS |
| tj/commander.js | ba6d13ddb424 | ba6d13ddb424 | https://github.com/tj/commander.js.git | 9 | PASS |
| expressjs/express | dae209ae6559 | dae209ae6559 | https://github.com/expressjs/express.git | 9 | PASS |
| axios/axios | 2d06f96e8602 | 2d06f96e8602 | https://github.com/axios/axios.git | 9 | PASS |
| chalk/chalk | aa06bb5ac3f1 | aa06bb5ac3f1 | https://github.com/chalk/chalk.git | 9 | PASS |
| google/gson | 004e7a4949e0 | 004e7a4949e0 | https://github.com/google/gson.git | 9 | PASS |
| junit-team/junit4 | 300468b1efd4 | 300468b1efd4 | https://github.com/junit-team/junit4.git | 9 | PASS |
| apache/commons-cli | f9a0a7fd2db7 | f9a0a7fd2db7 | https://github.com/apache/commons-cli.git | 9 | PASS |
| apache/commons-csv | 19b29139dfdb | 19b29139dfdb | https://github.com/apache/commons-csv.git | 9 | PASS |
| apache/commons-io | f729612edfca | f729612edfca | https://github.com/apache/commons-io.git | 9 | PASS |
| spring-guides/gs-rest-service | e9efc9dfa0ab | e9efc9dfa0ab | https://github.com/spring-guides/gs-rest-service.git | 9 | PASS |
