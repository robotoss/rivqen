# legacy-fixtures

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Model and generator of the legacy golden fixtures in `fixtures/legacy/` (format: `fixtures/legacy/README.md`). No dependencies. Node.js 22 or newer.

The model implements one server: `legacy-canonical` (rules C-01 to C-16 of `docs/engineering/protocol/legacy-wire.md` section 10) and the three upstream profiles `legacy-java`, `legacy-node` and `legacy-php` (divergences D-01 to D-15). No profile reproduces U-01, U-11, U-12 or U-13 (DL-069, H-111, DL-082): in every profile, a legacy response (`304`, data or full) drops the application `cdn-cache-control` and `surrogate-control` lines (C-13) and gets `vary: template-tag` after the application `vary` lines, unless one of them lists `template-tag` or `*` (C-15); and every profile keeps the scope of C-01 and C-03, so a `HEAD`, a `POST`, a 3xx or a 404 is pass-through. A pass-through response keeps every line. The generator writes `expected.json` from the model and proves that each profile result equals the stored upstream trace in `evidence/traces/server/`.

The client part (WP-01 S2) has a second model: a legacy client with the profiles `legacy-canonical` (rules CL-01 to CL-43 of `docs/engineering/protocol/legacy-client.md` section 9) and `legacy-android` (the canonical client with the named differences that the FACT rows and the stored Android traces prove). The generator writes the client `expected.json` files and the function fixtures, and proves each `legacy-android` result against the stored client trace in `evidence/traces/client/android/`. See [Client part](#client-part).

## Profile rules from the live upstream run (2026-10-31, DL-082)

| Item | Traces | Model |
|---|---|---|
| D-14 application `cache-offline` | S22, S23 | `legacy-java`: its own `true`, then every application `cache-offline` line unchanged (on a `304`: `true`, the application lines, `store`; without `accept-diff: true`: only the application lines, D-02). `legacy-node`, `legacy-php`: their own value only (`true` on `200`, `store` on `304`); the application value is lost. `legacy-canonical`: C-10 |
| D-15 `content-type` on `304` | S03, S04, S05, S24 | `legacy-java`, `legacy-node`: the application `content-type` lines are dropped on a `304`. `legacy-php` and `legacy-canonical` keep them (C-13) |
| U-13 not reproduced | S24 to S27 | Every profile: pass-through outside C-01. The profile fixtures of S24 to S27 have `not_reproduced: "U-13"` (`FX-LEG-PASS-HEAD--NODE` does not exist: Node.js passes the `HEAD` through, so `legacy-node` is in `profiles_same`) |
| U-12 not reproduced | S28, S29 | As before: `vary: template-tag` added, CDN fields dropped. Check 2 does not compare `vary` and the CDN fields, so these profile fixtures need no `not_reproduced` |

The Java order of D-14 on a `304` and the Java rule without `accept-diff: true` follow the field order of traces S22, S24 and S03 (Java writes `Cache-Control` and `Cache-Offline: true` before the application, and `Cache-Offline: store` after it). No trace has an application `cache-offline` on a `304` or without `accept-diff`.

The added lines of a data or full response are in the order of the stored traces (S02, S06, S07; client traces CS-01 to CS-27): `legacy-java` and `legacy-php` write `cache-control` and `cache-offline` first, then `etag`, `template-tag`, `template-change`; `legacy-node` writes `etag`, `template-tag`, `cache-offline`, `template-change`. The client part compares the legacy response fields in order. A server `expected.json` lists the fields by name, so this order changes no server fixture.

## Run

| Task | Command |
|---|---|
| Unit and property tests | `cd tools/legacy-fixtures && npm test` |
| Write every `expected.json` | `node tools/legacy-fixtures/gen.mjs` |
| Check (writes nothing; exit 1 on a problem) | `node tools/legacy-fixtures/gen.mjs --check` |
| Another fixture directory (`fixtures/malformed/legacy`) | `node tools/legacy-fixtures/gen.mjs [--check] --dir fixtures/malformed/legacy` |
| Mutation | The architect runs Stryker (`stryker.config.json` here, tool from `tools/rqp-protocol`), one tool at a time |

`--dir <path>` takes a directory in the same format with its own `pages/` directory. Checks 1, 3 and 4, the check of the `rules` names (check 5, first part) and the page checks run as for `fixtures/legacy`. The catalog size, the rule coverage, the use of every stored trace and the byte copies of the lab pages apply only to `fixtures/legacy`, also when it is named with `--dir`.

Exit codes of `gen.mjs`: `0` all files current (or written), `1` a stale, missing or unknown file or a failed check of the server part (checks 1 to 5) or the client part (checks 6 to 12), `2` usage error (also a `--dir` that is not a directory). A note (for example the client coverage before the first client fixture) does not change the exit code.

## Files

| Path | Content |
|---|---|
| `src/model.mjs` | `respond(profile, request, app, pageBytes)`, `analyze(...)` (also returns the split), `split`, `rebuild`, `PROFILES` |
| `src/split.mjs` | Title rule, block rule and rebuild rule for each profile; forward scans in linear time (C-16) |
| `src/server.mjs` | `handle(request, app, pageBytes, config)` for the conformance harness; `config.profile` defaults to `legacy-canonical` |
| `src/fixture.mjs` | Reading and checking `case.json`, placeholders, `expected.json` builder |
| `src/trace.mjs` | Reader of the trace files, the request comparison, the trace comparison and the checks of `not_reproduced` |
| `src/fields.mjs` | Field-line helpers |
| `src/client-case.mjs` | Check of a client `case.json` (`validateClientCase`, `validateBehavior`): members, types, limits, step words |
| `src/client-engine.mjs` | The step engine: schedule, phases, lab runtime, proxy faults (`hold`, `cut`, `drop_field`, `set_field`, `server_down`; the lab `setField` and `forwardFields` by import), clock and timers |
| `src/client-canonical.mjs` | The canonical session (CL-05 to CL-40), the abstract cache (CL-38), the field rules (CL-14 to CL-16, CL-21) and the canonical functions (CL-24, CL-41, CL-42) |
| `src/client-android.mjs` | The `legacy-android` differences A-01 to A-17 and the Android functions (session id, split, diff, etag, message) |
| `src/client-model.mjs` | `run(profile, mode, case, env)`: the `client` member, or `{ ok: false, error: { code, message } }` |
| `src/client-trace.mjs` | The trace projection of a stored run (DL-104), the normalization and `compareClient` (phases and message normalization by import from `tools/upstream-lab/client/src/determinism.mjs`) |
| `src/client-fixture.mjs` | The client part of the generator (checks 6 to 12) |
| `src/functions.mjs` | The function fixtures and their evidence check |
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

## Client part

Format: `fixtures/legacy/README.md` "Client fixtures". A fixture directory is a client fixture when its `case.json` has `kind: "client"`; every other directory is a server fixture.

### Client model

`run(profile, mode, case, env)` in `src/client-model.mjs` runs the schedule of a client case and returns the projection of DL-110 and DL-118: `sessions`, then `phases` (requests, cookies, actions, intercepts, results, snapshots). `env.page(id)` gives the page bytes and `env.fixture(id)` the `page` and `app` of a canonical server fixture. The server model of `server_profile` answers each request (`respond` in `src/model.mjs`).

| Property | Rule |
|---|---|
| Pure | No I/O, no real clock, no state between calls. The clock moves only at `settle` (CL-04) |
| No exception for a case | A case of any shape gives a result or `{ ok: false, error }`. Codes: `CASE_*` (a defect of the case: a member, a type, a limit, a `wait` for an event that does not occur, a `release` without a hold, a step on a session that is not open, a request before the first `case` entry, a missing page) and `CLIENT_NOT_MODELED` (an Android branch that no FACT row or trace settles; the message names the row) |
| Configuration errors | An unknown profile or mode throws `ModelError` (`LEGACY_UNKNOWN_PROFILE`, `LEGACY_UNKNOWN_MODE`) |
| Bounds | At most 256 steps, 16 sessions, 32 responses, 16 `cache_before` entries (`LIMITS` in `src/client-case.mjs`). Each step does work linear in the size of the responses. Each timer runs once |
| Order | The order of independent events is an input of the case (CL-01): a response arrives at once unless a `hold`, `cut`, `server_down` or an invalid network stops it |

The `legacy-android` differences (`src/client-android.mjs`) each cite their row:

| ID | Difference | Source |
|---|---|---|
| A-01 | Session id (the key of the Android cache and of the second-session refusal) | Section 1, D-20, D-21, FN-ID-* |
| A-02 | Split: case-sensitive markers, `{null}`, title after the blocks, empty template without a block | D-22 to D-24, FN-SPLIT-* |
| A-03 | Diff: a new key unless empty; values as strings | D-25, FN-DIFF-* |
| A-04 | Etag: a `w/` prefix in any case lowercases and removes every `w/` and `"` | D-31, FN-ETAG-* |
| A-05, A-06 | `template-change`; the page message (org.json `\/`, Android key order, nothing for an empty diff) | D-29, D-26, section 2.4 |
| A-07 | Web view fields: no `Set-Cookie`, `Cache-Control`, `Expires`, `Etag`, no repeated name | Section 2.9; CS-03, CS-08, CS-09 |
| A-08 | Cache: a record per session id with the header lines; `sonic-html-sha1` after a first load or template change; Standard writes a refused data update without a template | Section 8; trace report section 6 item 3; CS-04, CS-08 |
| A-09 | Intercept status `0` | CONTRACT section 5; CS-01 to CS-27 |
| A-10, A-11 | Cookie read at creation; empty validators without a cache; `sonic-sdk-version` | Section 2.9; CL-09 note; CS-01, CS-12, CS-17 |
| A-12 | The first data update after a first load sends every block | Trace report section 6 item 1; CS-04 |
| A-13 | The first-load and template-change save runs 1.5 s after the body; a destroy before it cancels it | Section 2.6 item 6, M-13; CS-18 |
| A-14 | `{}` at the next `get_diff` after a final-200 message | Trace report section 6 item 2; CS-04 |
| A-15 | A body of 0 bytes is never saved (no delayed save, no stream-close save); the cache stays as it was | CS-20, CS-21 (both modes) |
| A-16 | An empty body. Q3 (first load, Quick, before the preload `loadUrl`): no load call, the intercept gets the empty stream, -1 / -1 (probable upstream defect, confirmed). S1 (Standard): the empty stream, 1000 / 1000, notify. Q14 (Quick template change, network first): the empty page as data, 2000 / 304. S12 (Standard template change, network first): no result; the intercept gets 0 bytes with the header lines of the cached record | Trace report sections 5.1, 5.2, 6 item 10; CS-20, CS-21 |
| A-17 | `refresh` after a complete template change: codes reset, `isReload` `"true"`; the request has the etag and template tag of the template-change response, no new cookie read; no load call; a `304` gives 304 / 304, notify. Page messages after a refresh are at least 2 s apart; a closer one waits, a destroy cancels it. The delayed save of the template change does not write; the stream-close save writes 1.5 s after the intercept read the page, also after a destroy | Section 2.3, section 2.4 item 3, section 2.6 items 3 and 6, D-26, D-37; trace report section 4 note 7, section 5.2, section 6 item 12; CS-27 |
| Flow | Classification F4 to F9 (F4 compares with the etag of the request), the Quick slot (Q1 to Q22), the Standard pending stream (S1 to S18), M-04, M-05, no account-change rule (section 2.8). `cache-offline` cases of S2 (D-30, D-47, D-48): `http` in any case is F6 (CS-24a, CS-25: Quick `loadUrl` once, Standard keeps the cached page, cache kept); only exact `true` and `store` pass F12, so `Store` and `TRUE` take Q8 with the cache removed (Quick) and S8 with a write without a template (Standard; CS-24b, CS-24c); `store` alone takes Q9, Q13, S8, S10 (CS-22, CS-23); a head field `html-sha1` changes nothing (CS-26 equals CS-05) | Sections 2 and 3; trace report sections 4 and 5.1 |

Not modeled (`CLIENT_NOT_MODELED`, the message names the row): a `refresh` other than A-17 (Q19, S16: while the session runs, after another outcome than a template change, without a connection, after destroy, before the delayed save when no intercept read the new page, a response other than `304`, an intercept after the refresh); a second page message while a spaced one waits (section 2.4 item 3); an empty first-load body through the bridge stream (Q2, S1) or after the preload `loadUrl` (Q3 note 2); an empty template-change body after the cached page was delivered (Q12, Q13, S9, S10); a template change or a data update whose body ends with an error; a Standard rebuild failure (Q11); an intercept that arrives while the session reads a first-load body (except a destroy, CS-18); `cache_before` under `legacy-android`.

A new stored scenario that the model does not reproduce needs a new A-row (or a `CLIENT_NOT_MODELED` row) in `src/client-android.mjs`, nothing else: the tests take the list of stored modes from `evidence/traces/client/run.json` and `tools/upstream-lab/client/scenarios.json` (CS-19 excepted) and compare every one with the model.

### Trace projection

`projectRun(events, { url, account })` in `src/client-trace.mjs` makes the `client` member from one stored run; `projectRuns` requires that all runs of a scenario mode give one projection. `compareClient(expected, trace, { notReproduced })` applies the normalization of the format README (transport fields, `x-android-*`, `vary` and the CDN fields removed; `content-type` as a media type, DL-119; incomplete intercept bodies not compared; `data_sha256` by presence) and returns `null` or the first different item.

| Rule | How |
|---|---|
| DL-119 `content-type` | Each `content-type` line of a `load_data`, an intercept and a record becomes the media-type form of server check 2 (`mediaTypeKey` in `src/trace.mjs`): type, subtype, parameter names and the `charset` value in lower case, no white space around `;` and `=`, one pair of quotes removed, the parameter order kept. So the Tomcat form `text/html;charset=utf-8` (CS-08, CS-10, `lab-java`) equals `text/html; charset=utf-8` of the `legacy-java` server model. Another type, subtype, charset or parameter still differs |
| DL-120 `notReproduced: "U-13"` | For a client profile fixture with `not_reproduced: "U-13"`. The U-13 fields (`etag`, `template-tag`, `template-change`, `cache-offline`, `sonic-etag-key`) of a response whose status is not `200` or `304` are not compared. The trace must have at least one of them that the expected response does not have; else the result is `U-13: the trace does not show the documented difference …`. A `200` or a `304` is in the C-01 scope: its fields are always compared (the `304` fields are client input, CS-27). `set-cookie` is always compared. Another item is a difference |

### Checks 6 to 12

| Check | Problem text |
|---|---|
| 6. Client files current; a case error | `<id>/expected.json: stale`, `missing`; `functions/<file>: stale`; `<id>: <member message>`; `<id>: <CODE>: <message>` (model error) |
| 7. Client traces | `<id>: check 7: the steps differ from the steps of run <k> of <scenario> <mode>`; `<id>: check 7: the legacy-android result differs from the trace: first difference at item …` (with `not_reproduced: "U-13"`: `… U-13: the trace does not show the documented difference …`); `<id>: check 7: no stored run …` |
| 8. `OVERRIDE_NOT_NEEDED` | `<id>--ANDROID: OVERRIDE_NOT_NEEDED: the client member equals <id>` |
| 9. `OVERRIDE_MISSING` | `<id>: OVERRIDE_MISSING for legacy-android: first difference at item …` |
| 10. Client rules and rows | `<id>: rule CL-nn is not in legacy client section 9`; `the catalog has <n> canonical client fixtures, fewer than <floor> (CLIENT_FLOOR)`; `rule CL-nn is not covered by any client or function fixture` |
| 11. Stored runs used | `evidence/traces/client/android/<scenario>/<mode>: stored client runs are not used by any client fixture`; `<id>: the stored runs of <scenario> <mode> are already the trace of <id>` |
| 12. Function evidence | `functions/<profile>.json: <case>: no evidence line`; `functions/legacy-android.json: <case>: the Android model gives …, the evidence …` |

`CLIENT_FLOOR` in `gen.mjs` (as `CATALOG_SIZE`) is the number of canonical client fixtures that `fixtures/legacy` must keep. It is `0` until the client fixtures exist; the lane that adds them raises it. Above `0`, for `fixtures/legacy` only: fewer canonical client fixtures than the floor is a problem, and check 10 (coverage) and check 11 (stored runs used) give problems. At `0` they give one note each, also when client fixtures exist. So removing client fixtures cannot switch the checks off. The rule names of check 10 and the second line of check 11 (one stored mode, one fixture) always apply. A scenario with `webview: true` in `tools/upstream-lab/client/scenarios.json` (CS-19) is a note with its reason. When `fixtures/legacy/functions/` exists, the generator writes `cases.json` (a byte copy of `tools/upstream-lab/client/functions.json`) and the three profile files; the canonical entries cover CL-14, CL-15, CL-16, CL-23, CL-24, CL-28, CL-41, CL-42 and CL-43. A `trace` entry is the evidence line with the members of the README table; where the Android session model uses a function (session id, split, diff, etag), the model must also equal every evidence line of that function.

A new lab run needs no change in the generator: a new scenario mode in `evidence/traces/client/android/` needs one canonical client fixture (and an `--ANDROID` fixture when check 9 asks for one, with `not_reproduced: "U-13"` when its lab server adds legacy fields to an error, DL-120). If the `legacy-android` model does not reproduce the new trace, add an A-row (see the table above).

### Write a client fixture

1. Write `case.json` by hand from `fixtures/legacy/README.md`.
2. Run `node tools/legacy-fixtures/gen.mjs`.
3. Read the new `expected.json` against the CL rules.
4. If check 9 reports `OVERRIDE_MISSING`, write the `--ANDROID` fixture.
5. Run `node tools/legacy-fixtures/gen.mjs --check`.

## Rules for this directory

1. `case.json` is written by a person (`scaffold.mjs` only starts the files). `expected.json` is generated; never edit it by hand.
2. Never change a fixture to make the model pass. Fix the model or ask the protocol owner.
3. No upstream code, comments or sample HTML. The pages are Rivqen test pages.
