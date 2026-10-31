# Upstream lab (legacy protocol evidence)

This lab runs the **unmodified** upstream Tencent VasSonic server code in isolated containers and captures HTTP traces. The traces confirm the [legacy protocol pages](../../docs/engineering/protocol/legacy-wire.md) and feed the golden fixtures (WP-01, WP-02).

> **Clean-room note.** This directory contains only Rivqen code (harnesses, test pages, runner). Upstream code is mounted read-only from a separate checkout and is never copied into this repository.

## Contents

| Path | Content |
|---|---|
| `pages/` | Rivqen test pages (one per scenario: data change, template change, empty names, duplicates, titles, nesting, Unicode, …) |
| `node/app.js` | Koa app that serves a page through the upstream `compress.js` middleware (which calls `sonic_differ`) |
| `node/control.cjs`, `node/control.test.cjs` | Pure parts of the control endpoint (case parsing, `Host` and `Content-Type` checks) and their unit tests |
| `php/index.php` | Router that serves a page through the upstream `sonic.php`, with the control endpoint |
| `java/PageServlet.java`, `java/ControlFilter.java`, `java/web.xml` | Servlet, control filter (listed before the upstream filter) and filter mapping for the upstream `SonicFilter` jar |
| `start.sh` | Starts the three servers on `127.0.0.1:18081-18083` |
| `run_traces.py` | Sends the scenarios, saves raw traces, checks them against the documented model |
| `conformance.sh` | Runs the conformance runner against the three servers and writes `evidence/conformance/upstream-lab-<date>.json` |

## Routes of a lab server

| Route | Use |
|---|---|
| `/p/<page>` | A stored page of `pages/` through the upstream code (traces S01–S21) |
| `GET /__rivqen/conformance/info` | `{"contract":1,"suites":["legacy"],"profiles":["legacy-node"]}` (`legacy-php`, `legacy-java` for the others) |
| `POST /__rivqen/conformance/case` | Stores one case: `204`; `400` `BAD_CASE`; `415` (no `application/json`); `413` (over 8 MiB); `422` `UNSUPPORTED` (another suite or profile). A new case replaces the old one |
| any other path | The stored case: the harness sets the application status and field lines, writes the page bytes, and the upstream code acts on it (`503` before the first case) |

The control endpoint is test code ([CONTRACT.md](../../server/conformance/CONTRACT.md) §2). It is off unless `RIVQEN_CONFORMANCE=1` (`start.sh` sets it). Inside a container the peer is the bridge address, so the lab checks `Host` (`127.0.0.1:<published port>`, `localhost:…`, `[::1]:…`; `start.sh` passes the port as `LAB_HOST_PORT`) and `Content-Type`, not the peer address (DL-081). The ports are published on `127.0.0.1` of the host only. Limit: the Node.js harness writes a repeated application field name as one group at the place of its first line, so interleaved repeats (`a`, `b`, `a`) change order.

## Run

1. Clone the upstream at the pinned commit:

   ```bash
   git clone https://github.com/Tencent/VasSonic.git /tmp/vassonic
   git -C /tmp/vassonic checkout 59936beff656d4b5718ff6444d6c5e001a2c5231
   ```

2. Start the servers:

   ```bash
   UPSTREAM=/tmp/vassonic sh tools/upstream-lab/start.sh
   ```

3. Capture and check:

   ```bash
   python3 -I tools/upstream-lab/run_traces.py tools/upstream-lab/pages evidence/traces/server
   ```

4. Run the conformance runner against the three servers (profiles `legacy-node`, `legacy-php`, `legacy-java`):

   ```bash
   sh tools/upstream-lab/conformance.sh
   ```

   It writes `evidence/conformance/upstream-lab-<date>.json` (the `--json` report of each profile and its exit code). The exit code of the script is the largest runner exit code. `DATE=<yyyy-mm-dd>` and `OUT_DIR=<dir>` override the defaults.

5. Stop the servers: `docker rm -f lab-node lab-php lab-java`.

`run_traces.py` also runs the scenarios S22–S29 through the control endpoint: application `cache-offline: false` (S22) and `http` (S23), status `302` (S24), `404` (S25), `POST` (S26), `HEAD` (S27), application `Vary` (S28), application `CDN-Cache-Control` and `Surrogate-Control` (S29). The request is the data-change request of the catalog (page `b`, the tag of page `a`), except S24 (page `a`). The traces are saved as `S22-…` to `S29-…` next to the old ones. In `results.json` these scenarios are `observation` entries: they record the observed status and fields, with no pass/fail model.

## Tests without Docker

```bash
node --test tools/upstream-lab/node/control.test.cjs   # case parsing and checks (Node.js harness)
node --check tools/upstream-lab/node/app.js
php -l tools/upstream-lab/php/index.php
python3 -I -m py_compile tools/upstream-lab/run_traces.py
```

The upstream code runs only in the containers, so the live behavior is checked by `start.sh`, `run_traces.py` and `conformance.sh`.

## Last result

2026-10-31: 139 checks, 137 passed, 2 expected failures (Java, D-02 and D-12); S01–S21 reproduced the traces of 2026-10-09; S22–S29 recorded as observations. Conformance run by URL: `evidence/conformance/upstream-lab-2026-10-31.md`. See the [server trace report](../../docs/engineering/protocol/legacy-traces.md) §9.

2026-10-09: 115 checks, 113 passed, 2 expected failures (Java).
