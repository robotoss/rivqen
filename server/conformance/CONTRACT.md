# Conformance runner and harness contract

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Fixed by the architect for WP-02 S1 (DL-065, DL-066). Lanes do not change this file; a lane that needs a change returns `BLOCKED` with the question. Page: `docs/engineering/server/conformance.md`.

The runner tests a server **by URL** over HTTP. It does not start the server. Before each case it gives the server the case input through the control endpoint, then sends the fixture request and compares the response.

## 1. Command line

```bash
node server/conformance/bin/rivqen-conformance.mjs --target http://127.0.0.1:8080 --suite legacy --profile legacy-canonical
```

| Option | Meaning |
|---|---|
| `--target <url>` | Required. `http://` origin of the server under test (no path). The runner connects there; it does not use DNS for the fixture host |
| `--suite rqp\|legacy` | Required. `rqp` = the server cases of `fixtures/rqp/protocol` (`FX-RQ-P*`); `legacy` = `fixtures/legacy` |
| `--profile <name>` | Legacy only. `legacy-canonical` (default), `legacy-java`, `legacy-node`, `legacy-php`. Selection rule: `fixtures/legacy/README.md` ("The runner with `--profile`") |
| `--fixtures <dir>` | Optional fixture directory (default: the suite directory above) |
| `--only <prefix>` | Run only the cases whose id starts with the prefix |
| `--json <file>` | Also write the report as JSON (section 4) |
| `--timeout-ms <n>` | Per HTTP exchange; default 10000 |

Exit codes: `0` no MUST case failed; `1` at least one MUST case failed; `2` usage error, the target cannot be reached, `GET /__rivqen/conformance/info` fails, or the server does not support the requested suite or profile.

## 2. Control endpoint (server side)

Only a **test harness** has it: a small program that wraps the server code under test. It is never part of an SDK package, it is off unless the harness starts with an explicit flag (`--conformance` or `RIVQEN_CONFORMANCE=1`), and it answers only connections from `127.0.0.1` or `::1` (else `403`). Without the flag the paths below are unknown (`404`). A control request must have a `Host` of `127.0.0.1:<port>`, `localhost:<port>` or `[::1]:<port>` (else `403`, against DNS rebinding), and a `POST` must have `Content-Type: application/json` (else `415`, so that a cross-origin page cannot send it without a preflight) (DL-078). **Lab harness exception (DL-081):** a harness in a container of `tools/upstream-lab` sees the bridge address as the peer, because its port is published on `127.0.0.1` of the host only; it checks `Host` and `Content-Type`, not the peer address.

### `GET /__rivqen/conformance/info`

`200`, `application/json`:

```json
{ "contract": 1, "suites": ["rqp", "legacy"], "profiles": ["legacy-canonical", "legacy-java"] }
```

### `POST /__rivqen/conformance/case`

Request body (`application/json`, at most 8 MiB):

```json
{
  "suite": "legacy",
  "case_id": "FX-LEG-DATA-CHANGE",
  "profile": "legacy-canonical",
  "page_base64": "PCFET0NUWVBFIGh0bWw+…",
  "app": { "status": 200, "headers": [["content-type", "text/html; charset=utf-8"]] },
  "server_config": {}
}
```

| Member | Rule |
|---|---|
| `suite` | `rqp` or `legacy` |
| `profile` | Legacy only |
| `page_base64` | The exact application body bytes (`pages/<page>.html` of the case) |
| `app` | Status and field lines of the application response, in order (from `case.json`) |
| `server_config` | RQP: `server_config` of `expected.json` (or `{}`); legacy: `{}` |

Answers: `204` (stored; the next requests get this case), `400` with `{"error": "BAD_CASE"}` (invalid JSON or member), `422` with `{"error": "UNSUPPORTED"}` (suite or profile not supported). The harness keeps one case at a time; a new `POST` replaces it.

### Case requests

Every other request goes to the server code under test with the stored case: the application response is `app` with the body `page_base64`. The server code sees the request target as `<origin><path>`, where the origin is a harness option (default `https://shop.example.com`), so RQP delta bindings keep the fixture URL (DL-060). The harness writes the response field lines exactly as the server code returns them (order, repeated lines, case of names) and sets `Content-Length`.

## 3. Runner behavior

1. Read `GET /info`. Stop with exit code 2 if the suite or profile is not listed.
2. For each selected case, in directory order, one at a time: `POST /case`, then send the request of `expected.json`: method; the path and query of `request.target`; the field lines in order, exactly as written (repeated lines stay separate lines); plus `Host` (the authority of `request.target`) as the first line and `Connection: close`. No other line is added (no `Accept-Encoding`, no `User-Agent`).
3. Read the raw response: status, field lines in order with their spelling, and the body bytes. The runner does not join repeated fields and does not decode a content coding by itself.
4. Compare:
   - **RQP:** `compareServer` and `compareServerShould` of `tools/rqp-protocol/check.mjs` (fixtures.md §5). A `Content-Encoding` other than identity is a failure (the fixtures use no content coding).
   - **Legacy:** status; `fields` (`fixtures/legacy/README.md`); `Content-Length`; `Content-Encoding` absent or `gzip` (decode `gzip` before the body check, DL-073); `body` by kind.
5. A failed case does not stop the run.

## 4. Report

Text, one line per case, then the summary:

```text
PASS FX-LEG-304
FAIL FX-LEG-DATA-CHANGE: field cache-control: expected ["no-cache, private"], got ["no-cache"]
FAIL [SHOULD] FX-RQ-P35-…: …
SKIP FX-LEG-…: <reason>
54 passed, 1 failed, 0 SHOULD failed, 0 skipped
```

`--json`: `{ "target", "suite", "profile", "results": [{ "id", "level", "status": "pass" | "fail" | "skip", "problems": ["…"] }], "summary": { "passed", "failed", "should_failed", "skipped" } }`.

## 5. Server code modules (in-repository harness)

The harness `server/conformance/harness/` loads a module that exports the same function as the RQP checker:

```js
handle(request, app, pageBytes, config) → { status, headers, body }
// request: { method, target, headers: [[name, value], …] }
// app: { status, headers: [[name, value], …] }; pageBytes: Uint8Array
// config: server_config; for legacy also { profile }
// result: headers = [[name, value], …] in order; body = Uint8Array | null
```

Modules used in S1: `tools/rqp-protocol/src/server.mjs` (RQP reference), `examples/server-demo/src/rqp.js` (RQP, second implementation), `tools/legacy-fixtures/src/server.mjs` (legacy model, all four profiles).

```bash
node server/conformance/harness/harness.mjs --module <path> --suite rqp|legacy --port 0 --conformance
```

The harness prints `listening <port>` on its first stdout line.
