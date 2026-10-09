# Upstream lab (legacy protocol evidence)

This lab runs the **unmodified** upstream Tencent VasSonic server code in isolated containers and captures HTTP traces. The traces confirm the [legacy protocol pages](../../docs/engineering/protocol/legacy-wire.md) and feed the golden fixtures (WP-01, WP-02).

> **Clean-room note.** This directory contains only Rivqen code (harnesses, test pages, runner). Upstream code is mounted read-only from a separate checkout and is never copied into this repository.

## Contents

| Path | Content |
|---|---|
| `pages/` | Rivqen test pages (one per scenario: data change, template change, empty names, duplicates, titles, nesting, Unicode, …) |
| `node/app.js` | Koa app that serves a page through the upstream `compress.js` middleware (which calls `sonic_differ`) |
| `php/index.php` | Router that serves a page through the upstream `sonic.php` |
| `java/PageServlet.java`, `java/web.xml` | Servlet + filter mapping for the upstream `SonicFilter` jar |
| `start.sh` | Starts the three servers on `127.0.0.1:18081-18083` |
| `run_traces.py` | Sends the scenarios, saves raw traces, checks them against the documented model |

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

4. Stop the servers: `docker rm -f lab-node lab-php lab-java`.

## Last result

2026-10-09: 115 checks, 113 passed, 2 expected failures (Java). See the [server trace report](../../docs/engineering/protocol/legacy-traces.md).
