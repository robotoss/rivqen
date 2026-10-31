# legacy-fixtures

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Model and generator of the legacy golden fixtures in `fixtures/legacy/` (format: `fixtures/legacy/README.md`). No dependencies. Node.js 22 or newer.

The model implements one server: `legacy-canonical` (rules C-01 to C-16 of `docs/engineering/protocol/legacy-wire.md` section 10) and the three upstream profiles `legacy-java`, `legacy-node` and `legacy-php` (divergences D-01 to D-15). No profile reproduces U-01, U-11, U-12 or U-13 (DL-069, H-111, DL-082): in every profile, a legacy response (`304`, data or full) drops the application `cdn-cache-control` and `surrogate-control` lines (C-13) and gets `vary: template-tag` after the application `vary` lines, unless one of them lists `template-tag` or `*` (C-15); and every profile keeps the scope of C-01 and C-03, so a `HEAD`, a `POST`, a 3xx or a 404 is pass-through. A pass-through response keeps every line. The generator writes `expected.json` from the model and proves that each profile result equals the stored upstream trace in `evidence/traces/server/`.

## Profile rules from the live upstream run (2026-10-31, DL-082)

| Item | Traces | Model |
|---|---|---|
| D-14 application `cache-offline` | S22, S23 | `legacy-java`: its own `true`, then every application `cache-offline` line unchanged (on a `304`: `true`, the application lines, `store`; without `accept-diff: true`: only the application lines, D-02). `legacy-node`, `legacy-php`: their own value only (`true` on `200`, `store` on `304`); the application value is lost. `legacy-canonical`: C-10 |
| D-15 `content-type` on `304` | S03, S04, S05, S24 | `legacy-java`, `legacy-node`: the application `content-type` lines are dropped on a `304`. `legacy-php` and `legacy-canonical` keep them (C-13) |
| U-13 not reproduced | S24 to S27 | Every profile: pass-through outside C-01. The profile fixtures of S24 to S27 have `not_reproduced: "U-13"` (`FX-LEG-PASS-HEAD--NODE` does not exist: Node.js passes the `HEAD` through, so `legacy-node` is in `profiles_same`) |
| U-12 not reproduced | S28, S29 | As before: `vary: template-tag` added, CDN fields dropped. Check 2 does not compare `vary` and the CDN fields, so these profile fixtures need no `not_reproduced` |

The Java order of D-14 on a `304` and the Java rule without `accept-diff: true` follow the field order of traces S22, S24 and S03 (Java writes `Cache-Control` and `Cache-Offline: true` before the application, and `Cache-Offline: store` after it). No trace has an application `cache-offline` on a `304` or without `accept-diff`.

## Run

| Task | Command |
|---|---|
| Unit and property tests | `cd tools/legacy-fixtures && npm test` |
| Write every `expected.json` | `node tools/legacy-fixtures/gen.mjs` |
| Check (writes nothing; exit 1 on a problem) | `node tools/legacy-fixtures/gen.mjs --check` |
| Another fixture directory (`fixtures/malformed/legacy`) | `node tools/legacy-fixtures/gen.mjs [--check] --dir fixtures/malformed/legacy` |
| Mutation | The architect runs Stryker (`stryker.config.json` here, tool from `tools/rqp-protocol`), one tool at a time |

`--dir <path>` takes a directory in the same format with its own `pages/` directory. Checks 1, 3 and 4, the check of the `rules` names (check 5, first part) and the page checks run as for `fixtures/legacy`. The catalog size, the rule coverage, the use of every stored trace and the byte copies of the lab pages apply only to `fixtures/legacy`, also when it is named with `--dir`.

Exit codes of `gen.mjs`: `0` all files current (or written), `1` a stale, missing or unknown file or a failed check, `2` usage error (also a `--dir` that is not a directory).

## Files

| Path | Content |
|---|---|
| `src/model.mjs` | `respond(profile, request, app, pageBytes)`, `analyze(...)` (also returns the split), `split`, `rebuild`, `PROFILES` |
| `src/split.mjs` | Title rule, block rule and rebuild rule for each profile; forward scans in linear time (C-16) |
| `src/server.mjs` | `handle(request, app, pageBytes, config)` for the conformance harness; `config.profile` defaults to `legacy-canonical` |
| `src/fixture.mjs` | Reading and checking `case.json`, placeholders, `expected.json` builder |
| `src/trace.mjs` | Reader of the trace files, the request comparison, the trace comparison and the checks of `not_reproduced` |
| `src/fields.mjs` | Field-line helpers |
| `gen.mjs` | The generator and its checks |
| `scaffold.mjs` | One-time writer of the `case.json` files of the trace-backed rows S01 to S21. It never overwrites a file. The files of S22 to S29 were written by hand |

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
| 2. Traces | The status, the fields `etag`, `template-tag`, `template-change`, `cache-offline`, `cache-control`, `sonic-etag-key`, `content-type` (not `vary` and not the CDN fields: no profile reproduces U-12) and the decoded body (exact for a page, as JSON values for a data body) of every trace-backed profile response equal the stored trace. `content-type` compares as a media type (see below). The request equals the trace request (see below). Every stored trace is used by a fixture. A fixture with `not_reproduced` gets the check of its item instead (see below) | `<id>: <field>: expected …, trace …`; `<id>: the request differs from the request of <trace>`; `stored trace is not used by any fixture` |
| 3. `OVERRIDE_NOT_NEEDED` | A profile fixture without `not_reproduced` whose expected response equals the canonical one. A fixture with `not_reproduced` can equal it (a U-13 fixture expects the canonical pass-through); check 2 proves that its trace differs | `<id>: OVERRIDE_NOT_NEEDED: …` |
| 4. `OVERRIDE_MISSING` | The trace of a profile differs from the canonical expected response and no profile fixture exists | `<id>: OVERRIDE_MISSING for <profile>: …` |
| 5. Rules | Every `rules` item exists in legacy wire section 10. Every rule C-01 to C-16 is covered by a fixture; for `fixtures/legacy` it always runs, and the catalog must keep at least 59 canonical fixtures (`CATALOG_SIZE`); another directory (`--dir`) has no rule coverage check | `rule C-nn is not covered by any fixture` |

The `pages/` directory is checked too: a page that also exists in `tools/upstream-lab/pages/` must be a byte copy.

### Trace layout and the request comparison

A trace file has the request line `<method> <path> HTTP/1.1`, the request field lines, an empty line, `--- response ---`, `HTTP <status>`, the response field lines, an empty line, `[body decoded from content-encoding=<coding>]` and the decoded body. Traces S01 to S21 use the lab route `/p/<page>` and have no `Host` line. Traces S22 to S29 are case requests through the lab control endpoint: their first field line is `Host: shop.example.com` (the runner sends `Host` first), and the path is `/catalog`.

The request of a fixture equals the trace request when the method equals the method of the request line and the field lines (without a `Host` first line) are equal in order and spelling. For a trace with a `Host` first line, the authority and the path of `request.target` must also equal the `Host` value and the path. For S01 to S21 the path is not compared: it is the lab route, not the fixture target.

### `content-type` as a media type

Check 2 compares `content-type` on every response, also on a `304` (D-15). Each value is first written in one form: the type, the parameter names and the `charset` value in lower case, no white space around `;` and `=`, one pair of quotes removed, the parameter order kept. A byte comparison is not possible without false differences: Tomcat writes the application value `text/html; charset=utf-8` as `text/html;charset=utf-8` (all Java traces), and the PHP traces S01 to S21 have the PHP default `text/html; charset=UTF-8`. These spellings are the same media type (RFC 9110 section 8.3.1). The number of lines is compared exactly, so a missing or a second `content-type` is a difference.

### `not_reproduced`

`not_reproduced` takes only an item with a check (`U-11` or `U-13`; another value is a case error). The check proves that the trace shows the documented upstream difference and that the profile does not reproduce it.

| Item | Check |
|---|---|
| `U-11` | As check 2 without `template-tag` and the body content (the body kind is compared). The `template-tag` of the trace must differ. The request is not compared: the placeholder `{{tag:<page>}}` resolves with the safe title rule, so it is not the tag that the upstream server got |
| `U-13` | The fixture is outside C-01 (the method is not `GET`, or the application status is not `200`). The expected response has no split, no data body and no `template-change`. The trace shows legacy processing: a `304` for another expected status, a data body, or an `etag`, `template-tag`, `template-change` or `cache-offline` line that the expected response does not have. The request is compared as in check 2 |

`fields` in every `expected.json` always lists `etag`, `template-tag`, `template-change`, `cache-offline`, `cache-control`, `content-type`, `sonic-etag-key`, `vary`, `cdn-cache-control` and `surrogate-control` (`[]` = absent), then the other response fields in order.

`profiles_same` in a canonical `expected.json` lists the profiles whose trace equals the canonical response (no profile fixture needed).

## Rules for this directory

1. `case.json` is written by a person (`scaffold.mjs` only starts the files). `expected.json` is generated; never edit it by hand.
2. Never change a fixture to make the model pass. Fix the model or ask the protocol owner.
3. No upstream code, comments or sample HTML. The pages are Rivqen test pages.
