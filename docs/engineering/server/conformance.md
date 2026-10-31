# Conformance

Conformance means that a server SDK gives the same answer as the reference for every fixture. It is the gate for every server release.

**Status:** <Badge type="tip" text="FACT" /> The runner, the harness, the end-to-end driver (WP-02 S1), the malformed suite and the lab run against the three upstream servers (WP-02 S2) exist. <Badge type="info" text="DESIGN" /> The rest of this page (pass criteria, SDK gates) is the target for the server SDKs.

## 1. Components

| Component | Role |
|---|---|
| `fixtures/` | The truth (see [Golden fixtures](/engineering/protocol/fixtures)) |
| `server/rust-reference` | Oracle: must pass 100 % of fixtures (<Badge type="info" text="DESIGN" />: WP-03) |
| `server/conformance` | Runner: sends requests, compares responses |
| `server/conformance/harness` | Test harness: wraps a server module and has the control endpoint |
| `server/conformance/e2e/run.mjs` | Driver: starts the harness, runs the runner, stops the harness |
| `tools/upstream-lab/conformance.sh` | Runs the runner against the three upstream lab servers (section 2.8) |
| `evidence/conformance/` | Reports of the lab runs |
| Server under test | Node.js, Java or PHP example app with the SDK |

## 2. How to run

<Badge type="tip" text="FACT" /> The runner tests a server by URL over HTTP/1.1. It does not start the server. It needs Node.js 22 or later and has no dependency.

### 2.1 One command (end to end)

1. Run the driver with the module under test.

   ```bash
   node server/conformance/e2e/run.mjs --module tools/legacy-fixtures/src/server.mjs --suite legacy --profile legacy-java
   node server/conformance/e2e/run.mjs --module tools/rqp-protocol/src/server.mjs --suite rqp
   ```

2. Read the exit code and the report lines.

Add `--expect fail` for a server that must fail. The driver then exits 0 only when the runner reports a MUST failure. Options after `--` go to the runner.

### 2.2 Two commands (harness and runner)

1. Start the harness with the control endpoint on a loopback address.

   ```bash
   node server/conformance/harness/harness.mjs --module tools/legacy-fixtures/src/server.mjs --suite legacy --port 8080 --conformance
   ```

2. Run the runner against the origin.

   ```bash
   node server/conformance/bin/rivqen-conformance.mjs --target http://127.0.0.1:8080 --suite legacy --profile legacy-canonical
   ```

3. Read the exit code and the report.

### 2.3 Options

| Option | Meaning |
|---|---|
| `--target <url>` | Required. `http://` origin of the server (no path) |
| `--suite rqp\|legacy` | Required |
| `--profile <name>` | Legacy only. Default `legacy-canonical` |
| `--fixtures <dir>` | Another fixture directory |
| `--only <prefix>` | Only cases whose id starts with the prefix |
| `--json <file>` | Also write a JSON report |
| `--timeout-ms <n>` | Deadline of one HTTP exchange. Default 10000 |

### 2.4 Exit codes

| Code | Meaning |
|---|---|
| `0` | No MUST case failed. SHOULD failures and skips are allowed |
| `1` | At least one MUST case failed |
| `2` | Usage error, target not reachable, bad `info` answer, suite or profile not listed, fixtures unreadable, no case selected, or the JSON report cannot be written |

### 2.5 Report

<Badge type="tip" text="FACT" /> One line per case, then a summary.

```text
PASS FX-LEG-304
FAIL FX-LEG-DATA-CHANGE: field cache-control: expected ["no-cache, private"], got ["no-cache"]
SKIP FX-LEG-OFFLINE-TRUE: no expected.json
54 passed, 1 failed, 0 SHOULD failed, 1 skipped
```

### 2.6 Control endpoint

<Badge type="tip" text="FACT" /> The control endpoint is part of a test harness only. It is never part of an SDK package.

1. The harness starts it only with an explicit flag (`--conformance` or `RIVQEN_CONFORMANCE=1`). Without the flag, the paths are unknown (`404`).
2. It answers only connections from `127.0.0.1` or `::1`. Other connections get `403`.
3. `GET /__rivqen/conformance/info` lists the contract version, the suites and the profiles.
4. `POST /__rivqen/conformance/case` stores one case: the page bytes, the application response and the server configuration. The answers are `204`, `400` (bad case) and `422` (suite or profile not supported).

Contract: `server/conformance/CONTRACT.md`.

<Badge type="tip" text="FACT" /> Lab harness exception (DL-081): a harness in a container of `tools/upstream-lab` sees the bridge address as the peer, because its port is published on `127.0.0.1` of the host only. It checks `Host` (`127.0.0.1:<port>`, `localhost:<port>` or `[::1]:<port>`) and `Content-Type: application/json`, not the peer address. A server under test must accept a request field section of at least 8 KiB (CONTRACT.md section 3). Every fixture request stays under it.

### 2.7 Runner behavior

<Badge type="tip" text="FACT" /> The runner has these rules. They are stricter than the contract in places (sprint decisions D1 to D8 of the runner lane).

| Rule | Behavior |
|---|---|
| Skip | Only a legacy fixture without `expected.json` is skipped |
| Case refused | A `422` (or any status other than `204`) on `POST /case` is a failure of the case |
| No case | If no case is selected, the exit code is 2 |
| HTTP framing | A response fails the case if it has `Transfer-Encoding` with `Content-Length`, a transfer coding other than `chunked`, an interim `1xx` response, line ends other than CRLF, or obsolete line folding |
| Request values | A fixture value may hold only visible ASCII, space and tab. Else the case fails as a fixture defect |
| Content coding | RQP: `identity` only. Legacy: absent or `gzip` only |
| SHOULD failures | They are reported as `FAIL [SHOULD]`. The exit code stays 0 |
| RQP pages | Read from `<fixtures>/pages/` |

The runner joins no repeated field and decodes no content coding by itself, except `gzip` in the legacy suite. A bad response is a failed case with a reason, never a crash. Limits and failure modes: `server/conformance/README.md`.

### 2.8 Lab run against the upstream servers

<Badge type="tip" text="FACT" /> `tools/upstream-lab/conformance.sh` runs the runner against the three upstream servers by URL. The servers run the unmodified upstream code in Docker containers on `127.0.0.1:18081` (Node.js), `18082` (PHP) and `18083` (Java).

1. Start the three servers (`tools/upstream-lab/start.sh`, see `tools/upstream-lab/README.md`).
2. Run the script.

   ```bash
   sh tools/upstream-lab/conformance.sh
   ```

3. Read `evidence/conformance/upstream-lab-<date>.json`. It has the `--json` report and the exit code of each profile (`legacy-node`, `legacy-php`, `legacy-java`).
4. Read the exit code of the script. It is the largest runner exit code. A MUST failure of the runner is a finding, not an error of the script.
5. Stop the servers: `docker rm -f lab-node lab-php lab-java`.

Set `DATE=<yyyy-mm-dd>` or `OUT_DIR=<dir>` to change the file name or the directory.

## 3. Profiles

A profile is a named set of behaviors. `legacy-canonical` is the default. Other profiles reproduce documented divergences of upstream implementations (see [legacy divergences](/engineering/protocol/legacy-divergences)). A server SDK must pass the canonical profile and may declare extra profiles.

<Badge type="tip" text="FACT" /> The `--profile` selection rule (DL-068):

| Profile | Cases |
|---|---|
| `legacy-canonical` | Every canonical fixture `FX-LEG-<NAME>` |
| `legacy-java`, `legacy-node`, `legacy-php` | The canonical fixtures that list the profile in `profiles_same`, and the profile fixtures `FX-LEG-<NAME>--JAVA` (or `--NODE`, `--PHP`) |

A profile run checks only behavior that a stored upstream trace confirms. A profile never reproduces an unsafe upstream behavior (DL-069).

## 4. Results

<Badge type="tip" text="FACT" /> Runs over HTTP on 2026-10-30 (WP-02 S1):

| Module | Suite and profile | Result |
|---|---|---|
| RQP reference (`tools/rqp-protocol`) | `rqp` | 112/112 |
| Server demo (`examples/server-demo/src/rqp.js`) | `rqp` | 112/112 |
| Legacy model | `legacy-canonical` | 59/59 |
| Legacy model | `legacy-java` | 39/39 (31/31 on 2026-10-30) |
| Legacy model | `legacy-node` | 39/39 (31/31 on 2026-10-30) |
| Legacy model | `legacy-php` | 39/39 (31/31 on 2026-10-30) |
| Legacy model, malformed suite (`fixtures/malformed/legacy`) | `legacy-canonical` | 22/22 |

The broken servers of `tools/rqp-protocol/test/fixtures/` fail the `rqp` suite, as expected.

<Badge type="tip" text="FACT" /> The legacy rows of 2026-10-31 are runs of the driver (section 2.1) on 2026-10-31 (WP-02 S2). The profile counts grew from 31 to 39 because the eight fixtures of the traces S22 to S29 are now profile-checked.

### 4.1 Live run against the upstream servers, 2026-10-31

<Badge type="tip" text="FACT" /> The runner tested the three upstream servers by URL with the profile fixtures of `fixtures/legacy`. Evidence: `evidence/conformance/upstream-lab-2026-10-31.md` and `.json`. The profile fixtures expect the safe behavior (DL-069, DL-082), so a live server fails where it shows an unsafe behavior that the profiles do not reproduce.

| Profile | Passed | Failed |
|---|---|---|
| `legacy-node` | 3 | 36 |
| `legacy-php` | 4 | 35 |
| `legacy-java` | 0 | 39 |

Every failure is U-12, U-11 or U-13; the Tomcat spelling shows in 31 of the Java failures. No other difference was found.

| Class | Node.js | PHP | Java |
|---|---|---|---|
| U-12: no `Vary: template-tag`; CDN fields kept | 33 | 29 | 33 |
| U-11 (`not_reproduced`): several titles | 0 | 2 | 2 |
| U-13 (`not_reproduced`): legacy processing outside C-01 | 3 | 4 | 4 |
| Tomcat writes `text/html;charset=utf-8` | — | — | 31 |

The Java count of the Tomcat spelling overlaps the U-12 count: 31 of the 33 U-12 failures also show the spelling. The spelling is the same media type as `text/html; charset=utf-8` (RFC 9110 section 8.3.1). The servlet container writes it, not the upstream filter. The generator compares `content-type` as a media type. The runner compares field values byte for byte, so it reports the spelling as a failure.

## 5. Pass criteria

<Badge type="info" text="DESIGN" /> The criteria for the server SDKs:

| Level | Criterion |
|---|---|
| SDK release | 100 % of canonical-profile fixtures pass, directly and through the proxy lab |
| Profile claim | 100 % of that profile's fixtures pass |
| Regression | No previously passing fixture fails |
