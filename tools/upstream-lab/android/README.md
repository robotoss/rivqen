# Android client lab (WP-01)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

A Rivqen harness app runs the **unmodified** upstream Android SDK (VasSonic `59936bef`) on an API 28 emulator. The conductor (`tools/upstream-lab/client/`) gives the steps; the harness calls the SDK entry points in that order and reports every client call and callback as an event. Interfaces: [CONTRACT.md](../client/CONTRACT.md). Decisions: H-118 (harness, no patches), DL-099…DL-102. Sprint record: `docs/engineering/plan/sprints/WP-01-S1.md`.

> **Clean-room.** This directory holds only Rivqen code. The build reads the SDK sources in place from the read-only clone in `UPSTREAM` as a second Java source directory: no copy, no change, and `build.sh` stops when a file differs from `evidence/source-manifest.lock`. The harness is written from [legacy-client.md](../../../docs/engineering/protocol/legacy-client.md), CONTRACT.md and the public SDK entry points (names and signatures are interface facts, CR-03). The upstream sample and the upstream tests were not read.

## Contents

| Path | Content |
|---|---|
| `lab-env.sh` | Lab paths and variables (sourced; no side effects) |
| `env.sh` | Creates `LAB_HOME`: lab SDK root, emulator, platform-tools, API 28 image, AVD, `GRADLE_USER_HOME` |
| `clean.sh` | Stops the lab emulator and the Gradle daemon; deletes `LAB_HOME` and the build output |
| `build.sh` | Checks the upstream pin and file hashes; builds the APK; checks the library list |
| `emulator.sh` | `start` (cold boot, no window), `stop`, `status` of the lab emulator |
| `install.sh` | Installs the APK on the lab emulator |
| `run.sh` | One scenario run (`pm clear`, then `am instrument` with the run id and the control URL) |
| `functions.sh` | The function cases of CONTRACT §7 (mode `functions`) |
| `package-access.json` | The package-private and protected SDK members the harness uses (copy into `run.json`) |
| `tools/fake-conductor.mjs` | Smoke-test stand-in for the conductor: control channel and one static first-load answer |
| `tools/sample-functions.json` | Smoke cases for `functions.sh` (the real cases: `client/functions.json`) |
| `app/` | The harness app (Java, package `org.rivqen.lab.harness`) |
| `gradle/verification-metadata.xml` | SHA-256 of every build dependency (Gradle dependency verification) |

## Environment

All lab state is in `LAB_HOME` (default `/Volumes/BackCat/rivqen-lab`, DL-102). Nothing new goes to the system volume.

| Path in `LAB_HOME` | Content | Size (2026-11-05) |
|---|---|---|
| `sdk/` | Lab SDK root: copy of `cmdline-tools/latest`, `emulator` 37.2.13, `platform-tools` 37.0.1, `system-images;android-28;default;arm64-v8a` r02, licenses (copied from the main SDK) | 4.1 GiB |
| `avd/` | `ANDROID_AVD_HOME`, AVD `rivqen-api28` (2 GiB RAM, 2 GiB data, no snapshots) | 365 MiB |
| `user/` | `ANDROID_USER_HOME` (debug keystore, emulator state) | 246 MiB |
| `gradle/` | `GRADLE_USER_HOME` (wrapper distribution, AGP, caches, daemon) | 711 MiB |
| `tmp/`, `logs/` | `TMPDIR`, emulator log and PID file | small |

The build compiles against the **main** SDK (`~/Library/Android/sdk`, `platforms/android-36`, `build-tools/36.0.0`) and only reads it: `android.builder.sdkDownload=false` stops AGP from downloading into it.

| Item | Version |
|---|---|
| JDK | Zulu 21.0.11 (`JAVA_HOME` in `lab-env.sh`) |
| Gradle | 9.8.2 (wrapper; `distributionSha256Sum` pinned; wrapper JAR SHA-256 `3b8a2577…68e98` equals the published value) |
| Android Gradle plugin | 9.5.0 (needs Gradle 9.6 or later) |
| `compileSdk` / `minSdk` / `targetSdk` | 36 / 28 / 28 |
| Library in the APK | `com.android.support:support-annotations:28.0.0` only (`build.sh` checks it; SHA-1 equals the Google Maven value) |
| Emulator image | Android 9, API 28, `default`, `arm64-v8a`; WebView `com.android.webview` 66.0.3359.158 |

## Procedure

1. Set the upstream clone (read only, commit `59936beff656d4b5718ff6444d6c5e001a2c5231`):

   ```bash
   export UPSTREAM=<path of the VasSonic clone>
   ```

2. Create the lab (52 s on the development Mac on 2026-11-04, downloads included):

   ```bash
   sh tools/upstream-lab/android/env.sh
   ```

3. Build the APK:

   ```bash
   sh tools/upstream-lab/android/build.sh
   ```

4. Start the emulator (one emulator at a time; console port 5600, serial `emulator-5600`):

   ```bash
   sh tools/upstream-lab/android/emulator.sh start
   ```

5. Install the APK:

   ```bash
   sh tools/upstream-lab/android/install.sh
   ```

6. Start the conductor on `127.0.0.1:18090` and `127.0.0.1:18091`. Then start one run:

   ```bash
   sh tools/upstream-lab/android/run.sh <run-id>
   ```

7. Run the function cases:

   ```bash
   sh tools/upstream-lab/android/functions.sh tools/upstream-lab/client/functions.json evidence/traces/client/android/functions.jsonl
   ```

8. Stop the emulator: `sh tools/upstream-lab/android/emulator.sh stop`.
9. After the sprint, delete the lab: `sh tools/upstream-lab/android/clean.sh`.

Every lab `adb` call names the serial (`-s emulator-5600`), so it cannot reach another emulator or device on the machine.

### Commands for the conductor (L2, L5)

Use these commands to start one run. `<adb>` is `$LAB_HOME/sdk/platform-tools/adb`.

```bash
<adb> -s emulator-5600 shell pm clear org.rivqen.lab.harness
<adb> -s emulator-5600 shell am instrument -w -e run <run-id> -e control http://10.0.2.2:18091 org.rivqen.lab.harness/org.rivqen.lab.harness.LabInstrumentation
```

- The run id must match `[A-Za-z0-9._:/-]{1,128}`. The control URL must be `http://10.0.2.2:<port>`; the harness refuses all other URLs.
- `am instrument -w` returns when the run ends. The output has `INSTRUMENTATION_RESULT: rivqen.ok=true` (or `false` and `rivqen.error=<reason>`), `rivqen.run`, `rivqen.upstream_sha`, and `INSTRUMENTATION_CODE: -1` (ok) or `0` (failed).
- Function mode: `... am instrument -w -e mode functions org.rivqen.lab.harness/org.rivqen.lab.harness.LabInstrumentation`. It reads `files/functions.json` and writes `files/functions.jsonl` in the app storage; `functions.sh` moves the files with `run-as` (the APK is debuggable).

## Harness

| Class | Role |
|---|---|
| `LabInstrumentation` | Entry point: one run per `am instrument`; result bundle |
| `ControlChannel`, `ControlEvents` | CONTRACT §4 client (bounded answers, 1 MiB); events in one FIFO queue to `POST /run/event` |
| `ScenarioRun`, `RunConfig` | Step loop and step execution; scenario `config` through the public SDK builders |
| `LabRuntime` | The runtime of the SDK (account, cookie, network validity, response object, worker thread) |
| `RecordingClient` | The session client: each page action becomes an event; in a WebView run it also goes to the WebView |
| `RecordingStream` | Wraps the intercepted stream: passes bytes unchanged, counts, hashes, records `client.intercept` once |
| `WebViewActivity` | The real WebView of a WebView run (no JavaScript interface) |
| `FunctionCases`, `FixedResponseConnection` | CONTRACT §7 cases; a fixed-answer connection for cases that need a server object |
| `com.tencent.sonic.sdk.RivqenLabAccess` | Rivqen class in the SDK package; it calls package-private and protected members (list below) |

### Steps (CONTRACT §3)

The harness executes a step only after the conductor gives it, and reports `step.start` and `step.done` (`error` is `null` or a reason).

| Step | Thread | Call | `step.done` |
|---|---|---|---|
| `create_session` | main | `SonicEngine.createSession(url, config)` | After the call; `session.created` with the id or `null` |
| `bind_client` | main | `SonicSession.bindClient(client)` | After the call; `false` is an error |
| `client_ready` | main | `SonicSessionClient.clientReady()` | After the call |
| `request_resource` | `rq-intercept-<n>` | `SonicSessionClient.requestResource(url)`, then it reads the stream to the end | When the call returns, or after `wait_ms` (default 1000, max 30000) if the call still waits. `client.intercept` comes at the end of the stream |
| `page_finished` | main | `SonicSessionClient.pageFinish(url)` | After the call |
| `get_diff` | `rq-bridge-<n>` | `SonicSessionClient.getDiffData(callback)` | After the call; `session.message` when the SDK calls back |
| `refresh` | main | `SonicSession.refresh()` | After the call |
| `destroy` | main | `session.result` (codes), then `SonicSession.destroy()` | After the call |
| `set_account`, `set_cookie`, `set_network` | control | Change the value that the runtime returns | At once |
| `snapshot_cache` | control | Lists the SDK cache directory (`files/Sonic/`) | After `cache.snapshot` |

Step members that the harness reads besides `do`: `session` (1-based index of the created sessions; default: the last one), `wait_ms` (`request_resource`), `value`, `valid`.

Scenario members that the harness reads: `config`, `url` (optional; must start with `http://10.0.2.2:18090/`; default `http://10.0.2.2:18090/catalog`), `webview` (optional boolean; a real WebView run).

`config` keys (an unknown key stops the run): `account` (default `rq-test-account`), `cookie`, `network_valid` (runtime); `account_related`, `reload_in_bad_network`, `accept_diff`, `support_cache_control`, `support_local_server`, `auto_start`, `connect_timeout_ms`, `read_timeout_ms` (`SonicSessionConfig.Builder`); `get_cookie_when_session_create`, `cache_verify_with_sha1`, `unavailable_time_ms` (`SonicConfig.Builder`). The mode is `SonicSessionConfig.Builder.setSessionMode`: `quick` = `SESSION_MODE_QUICK`, `standard` = `SESSION_MODE_DEFAULT` (H-118).

### WebView run

With `"webview": true` the harness starts `WebViewActivity`. The bound client sends page actions to the WebView. The WebView calls `requestResource` for main-frame requests (on its own thread) and `pageFinish` from `onPageFinished`. The steps `request_resource` and `page_finished` are refused (`step.done` error `webview_drives: …`). The harness makes the bridge call itself (`get_diff`); the page gets no message and no JavaScript interface is added.

### Events

The harness sends the events of CONTRACT §5 with `ev`, `src: "harness"` and `thread` (`main` or the worker name); the conductor adds `seq` and `t`. Additional members and events (shared-file request to the architect):

| Event or member | Content |
|---|---|
| `hseq` member of every harness event | Harness order (1, 2, …), set when the event is queued. `seq` of the conductor is the receive order, and a harness event arrives later than a proxy event of the same moment |
| `session` member of `session.created`, `session.callback`, `session.result`, `session.message` | 1-based index of the session (more than one session per run) |
| `session.callback` `onSessionStateChange` | `args`: old and new state (0 none, 1 running, 2 ready, 3 destroy) |
| `runtime.notify_error` | `url`, `code`: the error report of the SDK (`SonicRuntime.notifyError`) |
| `runtime.toast` | `text`, `duration` (`SonicRuntime.showToast`) |
| `client.clear_history` | `SonicSessionClient.clearHistory()` |
| `webview.intercept_request`, `webview.page_started`, `webview.page_finished` | `url` (and `method`); WebView run only |
| `harness.error` | `error`: a harness defect that is not a step error |
| `run.end` | Sent by the harness only when the harness stops the run (`ok: false`) |

`session.result` is sent once per session, at `destroy`, from `getSrcResultCode()` and `getFinalResultCode()`: a reading at a fixed step, not at a race with the session thread.

### Runtime choices

These are Rivqen choices where the SDK leaves the host free. Each one is fixed for all runs.

| Item | Choice |
|---|---|
| `isSonicUrl` | `true` for every non-empty URL |
| `getUserAgent` | `RivqenLab/1 (Android harness)` (the SDK adds `Sonic/2.0.0`) |
| `getCookie`, `setCookie` | The scripted cookie; `setCookie` records the values and returns `true`. The WebView cookie store is not used |
| `createWebResourceResponse` | `android.webkit.WebResourceResponse(mime, encoding, stream)` plus `setResponseHeaders(headers)`. On API 28 this object has status `0` and reason `null` (observed); the WebView shows the page |
| `postTaskToThread` | One worker thread `rq-sdk-worker` (`HandlerThread`) |
| `postTaskToSessionThread` | The SDK default pool. During `create_session` the flow task waits (max 5 s) until the harness callbacks are registered |
| `log` | SDK lines of level INFO and higher go to the device log under the tag `RivqenLab` |

### Package access

`com.tencent.sonic.sdk.RivqenLabAccess` is Rivqen code in the SDK package. It uses these members; no reflection and no change of visibility (CONTRACT §7). The same list is in `package-access.json`.

| Member | Kind | Use |
|---|---|---|
| `SonicSessionCallback` | package-private interface, implemented | Session callbacks of every run |
| `SonicSession.addSessionStateChangedCallback` | protected method | State changes of every run |
| `SonicUtils.separateTemplateAndData` | package-private static method | fn `split` |
| `SonicUtils.getDiffData(JSONObject, JSONObject)` | package-private static method | fn `diff` |
| `SonicUtils.getCharset` | package-private static method | fn `charset` |
| `SonicServer.connect()` | protected method | fn `etag` |
| `SonicSession.setResult` | protected method | fn `message` |
| `SonicSession.server`, `.pendingDiffData`, `.diffDataCallback` | protected fields, written | fn `message` |

### Function cases (CONTRACT §7)

| `fn` | SDK member | Note |
|---|---|---|
| `session_id` | `SonicRuntime.makeSessionId(url, account_related)` (public) | `account` `null` → the runtime returns `null` |
| `split` | `SonicUtils.separateTemplateAndData` | `data` keeps the key order of the SDK; `false` from the SDK is a case error |
| `diff` | `SonicUtils.getDiffData(local, server)` | `local: null` → `null` (the two-argument function; the file-reading variant is not used) |
| `message` | `SonicSession.setResult(src, final, true)` on a session that never starts | `fields`: object or `[name, value]` pairs. A diff without `local_refresh_time` gets the current time, as the SDK data update flow does |
| `etag` | `SonicServer.connect()` over a fixed answer (status 200, field `eTag` = `value`) | The weak ETag rule of `connect()` |
| `charset` | `SonicUtils.getCharset(content_type)` | |
| `is_legacy_response` | — | `unsupported`: private `SonicServer.isSonicResponse` |
| `template_change` | — | `unsupported`: inline code of `SonicSession.handleFlow_Connection` |

The fixed-answer connection is given through the public `SonicSessionConfig.Builder.setConnectionInterceptor`. It always names the ETag field `eTag`: the custom ETag key of a response is not covered.

## Limits

- **Session start inside `createSession`.** With `auto_start` on (the SDK default), the session starts inside `createSession`. The callback `onSonicSessionStart` and the state change 0 → 1 occur before the harness can register, so they are not in the log.
- **Time values from the Android HTTP stack.** The response headers that the SDK passes on (`client.load_data`, `client.intercept`) and stores (`<id>.header` file) contain `X-Android-Sent-Millis` and `X-Android-Received-Millis`. Their values and the SHA-256 of the `.header` file change from run to run. The determinism rule of CONTRACT §6 does not remove them yet (shared-file request).
- **Order of harness and proxy events.** Harness events reach the conductor over HTTP; proxy events are recorded in the conductor at once. Between two steps, the conductor order (`seq`) of a harness event and a proxy event can change from run to run (observed: `runtime.cookie_get` before or after `proxy.request`). In two Quick runs the harness events (by `hseq`), the proxy events and the conductor events were each equal after the normalization of CONTRACT §6 plus the `X-Android-*-Millis` values; only the interleaving differed. CONTRACT §6 compares the merged order (shared-file request).
- **Page message age.** In `message` and in a data update, `local_refresh_time` is the age of the diff in milliseconds; the SDK leaves out `0`. Two runs of the same case can give `0` (left out) or `1`.
- **Device log.** One SDK path writes a stack trace to `System.err` (`SonicSession.setResult` on a JSON error); it is not under the `RivqenLab` tag.
- **API 28 only.** The WebView is the AOSP WebView 66 of the image. Results of a later WebView can differ.
- **Fake conductor.** `tools/fake-conductor.mjs` is a smoke tool: one fixed scenario, one static answer from `evidence/traces/server/node/S02-first-load.http`, no proxy faults. The real runs use the conductor of `tools/upstream-lab/client/`.

## Tests

The lab code has no unit tests and no mutation run (sprint record: not applicable). It is checked by `build.sh`, by the smoke runs below, and by the determinism rule of the stored runs (CONTRACT §6).

Smoke runs (2026-11-05, fake conductor, page `a`, first load):

| Run | Result |
|---|---|
| Quick, scripted | `client.load_data` (with headers) at `client_ready`; `request_resource` returns the stream (296 bytes, complete); message `code 304`, `srcCode 1000`, `isReload` `"false"`; 4 cache files |
| Standard, scripted | `client.load_url` at `client_ready`; stream 296 bytes; same message and codes |
| Quick, WebView | WebView loads the data; its main-frame intercept has a `data:` URL → `null`; page finished |
| Standard, WebView | WebView loads the URL; the intercept returns the SDK stream (296 bytes, complete); page finished |
| Functions (13 sample cases, run twice) | 11 results, 2 `unsupported`; equal except `local_refresh_time` of one message case |
