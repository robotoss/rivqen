<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

# Conformance harness

Test code only. It MUST NOT ship in an SDK package and MUST NOT run on a public interface. It binds to `127.0.0.1` only. Contract: `../CONTRACT.md` sections 2 and 5.

The harness serves a `handle()` module over HTTP. The conformance runner sets the case through the control endpoint, then sends the fixture request.

```bash
node server/conformance/harness/harness.mjs --module tools/rqp-protocol/src/server.mjs --suite rqp --port 0 --conformance
```

The first stdout line is `listening <port>`. Nothing else goes to stdout. Errors go to stderr as case id and error code only (no page bytes).

| Option | Meaning |
|---|---|
| `--module <path>` | Required. Module that exports `handle(request, app, pageBytes, config)` |
| `--suite rqp\|legacy` | Required. The one suite this harness serves |
| `--port <n>` | Default `0` (any free port) |
| `--origin <url>` | Origin of the request target seen by `handle()`. Default `https://shop.example.com` |
| `--conformance` | Turns the control endpoint on. `RIVQEN_CONFORMANCE=1` does the same |

## Control endpoint

| Request | Answer |
|---|---|
| any control path without the flag | `404` |
| any control path from an address other than `127.0.0.1` or `::1` | `403` |
| `GET /__rivqen/conformance/info` | `200` `{contract, suites: [suite], profiles}`. Legacy lists the four ADR-005 profiles. RQP lists none |
| `POST /__rivqen/conformance/case` | `204` stored (replaces the previous case); `400 BAD_CASE`; `422 UNSUPPORTED` (other suite or unknown profile); `413` body over 8 MiB |
| a case request before any case | `503` |

An over-limit body is read to its end and dropped, then answered with `413`. An early close would reset the connection and the client would not see the answer.

## Case requests

- `handle()` gets `target = origin + path`, the raw request field lines in order (from `req.rawHeaders`: case and repeated lines kept, `Host` and `Connection` included), the stored `app`, the page bytes and `server_config`. For `legacy`, `server_config` also has `profile`.
- The response field lines are written in the order, spelling and repetition that `handle()` returns. `Content-Length` is added only when `handle()` gives none and there is a body. There is no body for `HEAD`, for `304` or when `body` is `null`.
- If `handle()` throws, the answer is `500` `{"error":"HANDLE_FAILED"}`.

## What Node adds (FACT, checked by tests)

- `Connection: close` (or `keep-alive`) follows the request. The runner sends `Connection: close`.
- `Date` is switched off (`res.sendDate = false`), so the fixture field lines stay exact.
- Repeated names given in the flat array of `writeHead` stay separate lines.
