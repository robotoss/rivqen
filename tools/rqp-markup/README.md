# RQP markup tools

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Tools for the RQP markup parser candidates. The interface of the candidates is in `CONTRACT.md`.

## Differential runner

`diff.mjs` runs every candidate on every fixture of `fixtures/rqp/markup/` and compares each result with the `expected.json` of the fixture. It needs Node.js 22 or later. It has no dependencies.

```sh
node tools/rqp-markup/diff.mjs [--only name,...] [--fixtures dir] [--timeout seconds]
```

| Option | Meaning | Default |
|---|---|---|
| `--only name,...` | Run only the named candidates | all in `candidates.json` |
| `--fixtures dir` | Fixtures directory | `fixtures/rqp/markup` |
| `--candidates file` | Candidate list | `tools/rqp-markup/candidates.json` |
| `--timeout seconds` | Limit for each build and each batch run | 120 |

For each candidate, the runner does these steps:

1. Run the `build` command in the `cwd` of the candidate (skipped when `build` is `null`).
2. Run the `run` command once, with `{dir}` replaced by the absolute path of the fixtures directory (batch mode).
3. Compare each result line with `expected.json`: `valid`, `error`, `template_revision`, `page_revision` and `blocks`, element by element.

### Output

A matrix with one row for each fixture and one column for each candidate. Cell values:

| Cell | Meaning |
|---|---|
| `OK` | The result equals `expected.json` |
| `DIFF` | At least one compared field differs. A detail line follows for each difference. |
| `MISSING` | The candidate printed no result for this fixture |
| `CRASH` | Exit code other than 0, output that is not valid JSON, or a timeout. The cell is `CRASH` for every fixture. |
| `NOT BUILT` | The candidate directory does not exist, or the build failed, or a command cannot start |

Below the matrix, one line names the reason for each candidate that is `CRASH` or `NOT BUILT`. A detail line has the form `fixture [candidate] field: expected X, got Y`. The runner prints at most 80 characters of a value and never prints a full document.

### Exit code

| Code | Meaning |
|---|---|
| 0 | Every cell is `OK` |
| 1 | At least one cell is not `OK`, or the fixture set is empty |
| 2 | Usage error or unreadable input (arguments, `candidates.json`, fixtures) |

A candidate that is not built is never skipped silently. Use `--only` to leave it out on purpose.

### `candidates.json`

```json
[{ "name": "node", "cwd": "tools/rqp-markup/node", "build": ["npm", "ci"], "run": ["node", "src/cli.mjs", "--batch", "{dir}"] }]
```

| Field | Rule |
|---|---|
| `name` | Unique name for `--only` |
| `cwd` | Directory of the candidate, relative to the repository root |
| `build` | Command as an argument list, or `null` |
| `run` | Command as an argument list. `{dir}` is replaced in every argument. |

The runner starts commands without a shell. It stops a command that runs too long and its child processes.

### Tests

```sh
node --test "tools/rqp-markup/test/*.test.mjs"
```

The tests use small fake candidates (`test/fake-candidate.mjs`) and fixture directories that they create in a temporary directory. They do not use the real candidates.
