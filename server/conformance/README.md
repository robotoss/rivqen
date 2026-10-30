# rivqen-conformance — server conformance runner

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The runner tests an RQP server or a legacy server **by URL** over HTTP/1.1. It does not start the server. Before each case it gives the case input to the server through the control endpoint of a test harness, then it sends the fixture request and compares the response with `expected.json`. Contract: [`CONTRACT.md`](CONTRACT.md) (DL-065, DL-066). Page: `docs/engineering/server/conformance.md`.

Status (FACT, with tests): the command line, the suites `rqp` and `legacy`, the profile selection, the raw HTTP client, the comparison rules and the report. The harness (`harness/`) is a separate part (task WP-02/S1/T-04).

## Requirements

- Node.js 22 or later. No dependency. The runner imports `compareServer`, `compareServerShould` and `parseStrict` of `tools/rqp-protocol` by a relative path (one set of RQP comparison rules, DL-065).
- A server under test with the control endpoint (`CONTRACT.md` section 2), for example `harness/harness.mjs` around a `handle()` module.

## Procedure: run the runner

1. Start the server under test with its control endpoint on a loopback address. Example with the harness:

   ```bash
   node server/conformance/harness/harness.mjs --module tools/legacy-fixtures/src/server.mjs --suite legacy --port 8080 --conformance
   ```

2. Run the suite against the origin of the server:

   ```bash
   node server/conformance/bin/rivqen-conformance.mjs --target http://127.0.0.1:8080 --suite legacy --profile legacy-canonical
   node server/conformance/bin/rivqen-conformance.mjs --target http://127.0.0.1:8080 --suite rqp --json /tmp/rqp-report.json
   ```

3. Read the exit code (see below) and the `FAIL` lines.

## Options

| Option | Meaning |
|---|---|
| `--target <url>` | Required. `http://` origin of the server (no path, query, fragment or user information). The runner connects there; it does not use DNS for the fixture host |
| `--suite rqp\|legacy` | Required. `rqp`: the server cases `FX-RQ-P*` of `fixtures/rqp/protocol`. `legacy`: `fixtures/legacy` |
| `--profile <name>` | Legacy only: `legacy-canonical` (default), `legacy-java`, `legacy-node`, `legacy-php`. With `--suite rqp` it is a usage error |
| `--fixtures <dir>` | Another fixture directory (default: the suite directory) |
| `--only <prefix>` | Only the cases whose id starts with the prefix |
| `--json <file>` | Also write the JSON report |
| `--timeout-ms <n>` | Deadline of one HTTP exchange, from the connection to the end of the response; 1 to 600000, default 10000 |

### Profile selection (legacy)

| Profile | Cases |
|---|---|
| `legacy-canonical` | Every canonical fixture `FX-LEG-<NAME>` |
| `legacy-java`, `legacy-node`, `legacy-php` | The canonical fixtures that list the profile in `profiles_same` and that have no profile fixture of that profile, plus the profile fixtures `FX-LEG-<NAME>--JAVA` (or `--NODE`, `--PHP`) |

A profile fixture takes `page` and `app` from the `case.json` of the fixture that it `replaces`. A directory name with `--` and another suffix is not a fixture name and is not selected. Cases run in byte order of their directory names, one at a time.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | No MUST case failed (SHOULD failures and skips are allowed) |
| `1` | At least one MUST case failed |
| `2` | Usage error; the target cannot be reached; `GET /__rivqen/conformance/info` fails or is not an answer of contract 1; the server does not list the suite or the profile; the fixture directory cannot be read; no case is selected; the JSON report cannot be written |

## Report

One line per case, then the summary:

```text
PASS FX-LEG-304
FAIL FX-LEG-DATA-CHANGE: field cache-control: expected ["no-cache, private"], got ["no-cache"]
FAIL [SHOULD] FX-RQ-P09-DELTA: header vary: does not list rq-nonce
SKIP FX-LEG-OFFLINE-TRUE: no expected.json
54 passed, 1 failed, 0 SHOULD failed, 1 skipped
```

- Several problems of one case are on one line, separated by `; `. On a MUST failure, the SHOULD problems of the same case follow with the prefix `[SHOULD] `.
- `FAIL [SHOULD]`: the case has `level: "should"`, or only a SHOULD rule failed (RQP `Vary: Rq-Nonce` on a delta, DL-063). It does not change the exit code.
- `SKIP`: a canonical legacy fixture without `expected.json` (not generated yet).
- Every text on stdout or stderr is escaped: C0 and C1 controls, U+2028, U+2029, U+202A to U+202E and U+2066 to U+2069 are written as `\uXXXX`. The JSON report keeps the raw strings.
- At most 50 problems are kept per case. When there are more, the first 50 stay and one more line `… N more` is added (text and JSON).

JSON (`--json`): `{ "target", "suite", "profile", "results": [{ "id", "level", "status", "problems" }], "summary": { "passed", "failed", "should_failed", "skipped" } }`. `profile` is `null` for `rqp`. `level` is `"should"` for a SHOULD result as above; `summary.failed` counts the results with `status: "fail"` and `level: "must"`.

## What the runner sends

- `GET /__rivqen/conformance/info` once, then for each case `POST /__rivqen/conformance/case` (`suite`, `case_id`, `profile` for legacy only, `page_base64` = the bytes of `pages/<page>.html`, `app` from `case.json` (legacy default: `200` with `content-type: text/html; charset=utf-8`), `server_config` = `expected.server_config` or `{}`). A case over 8 MiB fails without a request.
- The fixture request: `<method> <path and query of request.target> HTTP/1.1`, `Host: <authority of request.target>` as the first line, the field lines of `expected.json` in order and spelling (a repeated name stays separate lines), `Connection: close`. Nothing else (no `Accept-Encoding`, no `User-Agent`, no `Content-Length`). The bytes are the fixture strings; a value with a byte outside visible ASCII, SP and HTAB fails the case as a fixture defect.

## Comparison rules

The raw response is the status, the field lines in order with their spelling (OWS removed around each value), and the body after the transfer framing (RFC 9112 section 6.3: `chunked` is decoded; it is framing, not a content coding). The runner never joins repeated fields and never decodes a content coding by itself.

| Suite | Rules |
|---|---|
| `rqp` | `compareServer` and `compareServerShould` of `tools/rqp-protocol/check.mjs` (fixtures page section 5). A `Content-Encoding` other than `identity` fails |
| `legacy` | Status. Each name of `fields`: the exact list of values, one per field line, names case-insensitive; `[]` = absent. `Content-Encoding`: absent or exactly one `gzip` line (case-insensitive, DL-073); `gzip` is decoded before the body check. Body: `page` = exactly the page bytes; `data` = a JSON object with exactly `data`, `template-tag`, `html-sha1`, `diff`, compared as JSON values (D-08; strict JSON: no duplicate member); `none` = no body bytes |

`Content-Length` is checked by the framing: a body shorter than `Content-Length` (the connection closes first) or bytes after the end of the response fail the case. For `HEAD`, `204` and `304` the response has no body and `Content-Length` is not compared (RFC 9110 section 8.6).

## Limits and failure modes

The response is untrusted. A defect is a failed case with the reason, never a crash; the run goes on with the next case.

| Limit or rule | Value |
|---|---|
| Field section (status line, field lines, empty line) | 64 KiB |
| Body after the transfer framing; `Content-Length` larger than this fails before reading | 64 MiB |
| Decoded `gzip` body (legacy) | 64 MiB |
| One chunk-size line; the trailer section | 64 KiB each |
| One HTTP exchange | `--timeout-ms` (whole exchange, not idle time) |
| Rejected as malformed | No CRLF line ends, obsolete line folding, a field name that is not a token, more than 1,048,576 chunks in one body, a control character in a value or the reason phrase, `Transfer-Encoding` together with `Content-Length`, a transfer coding other than `chunked`, a bad or inconsistent `Content-Length`, a bad chunk, an interim `1xx` response |

## Procedure: run the tests

1. Go to `server/conformance`.
2. Run `node --test "test/*.test.mjs"`.

The tests use local servers on `127.0.0.1`, port 0: `test/helpers.mjs` has a fake server with the control endpoint (enough for these tests; it is not the harness). `test/fixtures/legacy/` has mini fixtures in the format of `fixtures/legacy/README.md`, written for the tests (not golden fixtures). The RQP tests use the real `fixtures/rqp/protocol` and the reference server step of `tools/rqp-protocol` behind the fake server.

Mutation check: `stryker.config.json` (the Stryker of `tools/rqp-protocol`), see `docs/engineering/quality/mutation.md`.
