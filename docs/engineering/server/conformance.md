# Conformance

Conformance means that a server SDK gives the same answer as the reference for every fixture. It is the gate for every server release.

**Status:** <Badge type="tip" text="FACT" /> The runner, the harness and the end-to-end driver exist (WP-02 S1). <Badge type="info" text="DESIGN" /> The rest of this page (pass criteria, SDK gates) is the target for the server SDKs.

## 1. Components

| Component | Role |
|---|---|
| `fixtures/` | The truth (see [Golden fixtures](/engineering/protocol/fixtures)) |
| `server/rust-reference` | Oracle: must pass 100 % of fixtures (<Badge type="info" text="DESIGN" />: WP-03) |
| `server/conformance` | Runner: sends requests, compares responses |
| `server/conformance/harness` | Test harness: wraps a server module and has the control endpoint |
| `server/conformance/e2e/run.mjs` | Driver: starts the harness, runs the runner, stops the harness |
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
| Legacy model | `legacy-canonical` | 55/55 |
| Legacy model | `legacy-java` | 31/31 |
| Legacy model | `legacy-node` | 31/31 |
| Legacy model | `legacy-php` | 31/31 |

The broken servers of `tools/rqp-protocol/test/fixtures/` fail the `rqp` suite, as expected.

## 5. Pass criteria

<Badge type="info" text="DESIGN" /> The criteria for the server SDKs:

| Level | Criterion |
|---|---|
| SDK release | 100 % of canonical-profile fixtures pass, directly and through the proxy lab |
| Profile claim | 100 % of that profile's fixtures pass |
| Regression | No previously passing fixture fails |
