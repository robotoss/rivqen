# legacy-fixtures

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Model and generator of the legacy golden fixtures in `fixtures/legacy/` (format: `fixtures/legacy/README.md`). No dependencies. Node.js 22 or newer.

The model implements one server: `legacy-canonical` (rules C-01 to C-16 of `docs/engineering/protocol/legacy-wire.md` section 10) and the three upstream profiles `legacy-java`, `legacy-node` and `legacy-php` (divergences D-01 to D-13). No profile reproduces U-01, U-11 or U-12 (DL-069, H-111): in every profile, a legacy response (`304`, data or full) drops the application `cdn-cache-control` and `surrogate-control` lines (C-13) and gets `vary: template-tag` after the application `vary` lines, unless one of them lists `template-tag` or `*` (C-15). A pass-through response keeps every line. The generator writes `expected.json` from the model and proves that each profile result equals the stored upstream trace in `evidence/traces/server/`.

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
| `src/split.mjs` | Title rule, block rule and rebuild rule for each profile; forward scans in linear time (C-16) |
| `src/server.mjs` | `handle(request, app, pageBytes, config)` for the conformance harness; `config.profile` defaults to `legacy-canonical` |
| `src/fixture.mjs` | Reading and checking `case.json`, placeholders, `expected.json` builder |
| `src/trace.mjs` | Reader of the trace files and the trace comparison |
| `src/fields.mjs` | Field-line helpers |
| `gen.mjs` | The generator and its checks |
| `scaffold.mjs` | One-time writer of the `case.json` files of the trace-backed rows. It never overwrites a file |

The model never throws for a request, an application response or page bytes of any shape. A malformed input gives pass-through. An unknown profile name throws `ModelError` (`LEGACY_UNKNOWN_PROFILE`).

## Linear time (C-16, DL-077)

The regular expressions of C-06 and C-07 define the split result. A backtracking engine takes quadratic time on many unclosed `<title` or `<!--sonicdiff` openers (256 KiB: about 2.8 s and 1.2 s on the development machine). `src/split.mjs` does not use them. It scans forward:

| Rule | Scan |
|---|---|
| C-06 title | For each `<title` opener (ASCII case-insensitive), take the next `</title>` and the next LF after the opener. A match when the `</title>` comes first. Both positions only move forward. No `</title>` left: stop |
| C-07 blocks | For each `<!--sonicdiff` opener with a valid name and `-->`, take the next valid end marker that starts at least one code unit after the opener. The end-marker position only moves forward. No end marker left: stop. After a match, continue after its end marker |

The same scan applies to field values: `trimOws` is a loop, not `/[ \t]+$/` (also quadratic).

Tests: `split_equals_regex_on_random_pages` compares the scan with the expressions (the oracle, in the test only) on 3000 seeded pages in every profile; `split_scan_edge_cases_equal_the_expressions` lists the edge cases (an opener named `end`, `-?`, `\w`, ASCII-only case folding, content of at least one code unit, no overlap, `<title</title>`). The timing tests require less than 100 ms for each 256 KiB adversarial input. Fixture `FX-LEG-UNCLOSED-OPENERS` (page `openers`) covers C-16 for a server under test.

## What each check means

`gen.mjs --check` runs these checks. Each problem line starts with the fixture ID.

| Check | Meaning | Problem text |
|---|---|---|
| 1. Current files | `expected.json` equals the regenerated file byte for byte. No file is missing. No unknown file or directory | `<id>/expected.json: stale`, `missing`; `<path>: unknown file`, `unknown entry` |
| 2. Traces | The status, the fields `etag`, `template-tag`, `template-change`, `cache-offline`, `cache-control`, `sonic-etag-key` (not `vary` and not the CDN fields: no profile reproduces U-12) and the decoded body (exact for a page, as JSON values for a data body) of every trace-backed profile response equal the stored trace. The request lines equal the trace request lines. Every stored trace is used by a fixture. A fixture with `not_reproduced` is compared for status, other fields and body kind, and its `template-tag` must differ from the trace | `<id>: <field>: expected …, trace …`; `stored trace is not used by any fixture` |
| 3. `OVERRIDE_NOT_NEEDED` | A profile fixture whose expected response equals the canonical one | `<id>: OVERRIDE_NOT_NEEDED: …` |
| 4. `OVERRIDE_MISSING` | The trace of a profile differs from the canonical expected response and no profile fixture exists | `<id>: OVERRIDE_MISSING for <profile>: …` |
| 5. Rules | Every `rules` item exists in legacy wire section 10. Every rule C-01 to C-16 is covered by a fixture; this part runs once there are 54 canonical fixtures (`CATALOG_SIZE`); before that, a note lists the rules not yet covered | `rule C-nn is not covered by any fixture` |

The `pages/` directory is checked too: a page that also exists in `tools/upstream-lab/pages/` must be a byte copy.

`fields` in every `expected.json` always lists `etag`, `template-tag`, `template-change`, `cache-offline`, `cache-control`, `content-type`, `sonic-etag-key`, `vary`, `cdn-cache-control` and `surrogate-control` (`[]` = absent), then the other response fields in order.

`profiles_same` in a canonical `expected.json` lists the profiles whose trace equals the canonical response (no profile fixture needed).

## Rules for this directory

1. `case.json` is written by a person (`scaffold.mjs` only starts the files). `expected.json` is generated; never edit it by hand.
2. Never change a fixture to make the model pass. Fix the model or ask the protocol owner.
3. No upstream code, comments or sample HTML. The pages are Rivqen test pages.
