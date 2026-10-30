# legacy-fixtures

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Model and generator of the legacy golden fixtures in `fixtures/legacy/` (format: `fixtures/legacy/README.md`). No dependencies. Node.js 22 or newer.

The model implements one server: `legacy-canonical` (rules C-01 to C-14 of `docs/engineering/protocol/legacy-wire.md` section 10) and the three upstream profiles `legacy-java`, `legacy-node` and `legacy-php` (divergences D-01 to D-13). No profile reproduces U-01 or U-11 (DL-069). The generator writes `expected.json` from the model and proves that each profile result equals the stored upstream trace in `evidence/traces/server/`.

## Run

| Task | Command |
|---|---|
| Unit and property tests | `cd tools/legacy-fixtures && npm test` |
| Write every `expected.json` | `node tools/legacy-fixtures/gen.mjs` |
| Check (writes nothing; exit 1 on a problem) | `node tools/legacy-fixtures/gen.mjs --check` |
| Mutation | The architect runs Stryker (`stryker.config.json` here, tool from `tools/rqp-protocol`), one tool at a time |

Exit codes of `gen.mjs`: `0` all files current (or written), `1` a stale, missing or unknown file or a failed check, `2` usage error.

## Files

| Path | Content |
|---|---|
| `src/model.mjs` | `respond(profile, request, app, pageBytes)`, `analyze(...)` (also returns the split), `split`, `rebuild`, `PROFILES` |
| `src/split.mjs` | Title rule, block rule and rebuild rule for each profile |
| `src/server.mjs` | `handle(request, app, pageBytes, config)` for the conformance harness; `config.profile` defaults to `legacy-canonical` |
| `src/fixture.mjs` | Reading and checking `case.json`, placeholders, `expected.json` builder |
| `src/trace.mjs` | Reader of the trace files and the trace comparison |
| `src/fields.mjs` | Field-line helpers |
| `gen.mjs` | The generator and its checks |
| `scaffold.mjs` | One-time writer of the `case.json` files of the trace-backed rows. It never overwrites a file |

The model never throws for a request, an application response or page bytes of any shape. A malformed input gives pass-through. An unknown profile name throws `ModelError` (`LEGACY_UNKNOWN_PROFILE`).

## What each check means

`gen.mjs --check` runs these checks. Each problem line starts with the fixture ID.

| Check | Meaning | Problem text |
|---|---|---|
| 1. Current files | `expected.json` equals the regenerated file byte for byte. No file is missing. No unknown file or directory | `<id>/expected.json: stale`, `missing`; `<path>: unknown file`, `unknown entry` |
| 2. Traces | The status, the fields `etag`, `template-tag`, `template-change`, `cache-offline`, `cache-control`, `sonic-etag-key` and the decoded body (exact for a page, as JSON values for a data body) of every trace-backed profile response equal the stored trace. The request lines equal the trace request lines. Every stored trace is used by a fixture. A fixture with `not_reproduced` is compared for status, other fields and body kind, and its `template-tag` must differ from the trace | `<id>: <field>: expected …, trace …`; `stored trace is not used by any fixture` |
| 3. `OVERRIDE_NOT_NEEDED` | A profile fixture whose expected response equals the canonical one | `<id>: OVERRIDE_NOT_NEEDED: …` |
| 4. `OVERRIDE_MISSING` | The trace of a profile differs from the canonical expected response and no profile fixture exists | `<id>: OVERRIDE_MISSING for <profile>: …` |
| 5. Rules | Every `rules` item exists in legacy wire section 10. Every rule C-01 to C-14 is covered by a fixture; this part runs once there are 54 canonical fixtures (`CATALOG_SIZE`); before that, a note lists the rules not yet covered | `rule C-nn is not covered by any fixture` |

The `pages/` directory is checked too: a page that also exists in `tools/upstream-lab/pages/` must be a byte copy.

`profiles_same` in a canonical `expected.json` lists the profiles whose trace equals the canonical response (no profile fixture needed).

## Rules for this directory

1. `case.json` is written by a person (`scaffold.mjs` only starts the files). `expected.json` is generated; never edit it by hand.
2. Never change a fixture to make the model pass. Fix the model or ask the protocol owner.
3. No upstream code, comments or sample HTML. The pages are Rivqen test pages.
