# WebView lab: host part

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The lab runs test pages in a real WebView and compares the DOM with parse5. It answers questions Q-19 and Q-20. The contract is `CONTRACT.md`. This page describes the host tools in `host/`. The apps are in `app/`.

## Requirements

- Node.js 22 or later. The host tools have no runtime dependency.
- parse5 8.0.1 for `compare.mjs`. The lab does not install it. It uses the copy of `tools/rqp-markup/crosscheck`.

Procedure to install parse5:

1. Run `npm ci` in `tools/rqp-markup/crosscheck`.
2. To use another copy, set `WEBVIEW_LAB_PARSE5` to the parse5 package directory. The version must be 8.0.1.

## Procedure

1. Run `node host/build-cases.mjs` to write `cases/`. Commit the result.
2. Run `node host/server.mjs [--port N]`. The default port is 18090. The server binds `127.0.0.1` only.
3. Run the app on the simulator (iOS: `127.0.0.1:<port>`) or on the emulator (Android: `10.0.2.2:<port>`). The app writes `results/<platform>/<id>.<view>.json`.
4. Run `node host/compare.mjs --platform ios` (or `android`). The tool writes `results/<platform>/summary.md`.
5. Run `node host/build-cases.mjs --check` to find out if `cases/` is current. It exits 1 if not.

## Tools

| File | Task |
|---|---|
| `host/build-cases.mjs` | Writes `cases/<id>.json` and `cases/index.json` (the ordered list of ids). Output is the same on every run. |
| `host/server.mjs` | Serves `/cases.json` (the content of `cases/index.json`) and `/c/<id>`. Everything else gets 404. A broken case file gets 500. |
| `host/serialize.js` | The DOM walker. The apps inject it. |
| `host/compare.mjs` | Compares the results of one platform with parse5. |
| `host/lab.mjs`, `host/parse5.mjs`, `host/tree.mjs` | Shared code: case parser and Q-20 list, parse5 loader, parse5 serializer. |
| `host/pages/*.html` | The Q-20 pages, written for Rivqen. `q20-bom.html` starts with a UTF-8 byte order mark. |

### Cases

- Q-19: the golden fixtures of `fixtures/rqp/markup/` whose name has `NOSCRIPT`, `EDGE-E3-` or `EDGE-E8-`. The case id is the fixture name without `FX-RQ-MARKUP-`. Views `on` and `off`.
- Q-20: eight cases, view `on`. Each page has `<p id="m">Grüße – 日本</p>`. The probe is `#m`.
- The S5 `S5-*` cases are not in the list yet. They come with S5 T-02.

### Server

- The server writes each response to the socket as raw bytes. The header lines of a case stay as they are, also two lines with the same name.
- The server adds `Content-Length` and `Connection: close` only when the case has no such line.
- The server does not parse or run a case body. It makes no outbound request. A `source` path outside the repository gives 500.

### What `serialize.js` reads

`serialize.js` is a plain script. Its value is the tree string. It reads these properties and nothing else:

| Node | Properties |
|---|---|
| `document` | `childNodes` |
| Every node | `nodeType` (1, 3, 8, 10) |
| Element | `localName` (or `nodeName`), `namespaceURI` (svg and math get a prefix word), `attributes` (each: `name`, `localName`, `prefix`, `value`), `childNodes`, `content` (a template: `content.childNodes`) |
| Text, comment | `data` |
| Doctype | `name`, `publicId`, `systemId` |

The output is the html5lib tree format of CONTRACT section 4. Attributes are sorted by the shown name (`xlink href` for a prefixed attribute). The walk uses an explicit stack, so a deep tree does not overflow the call stack.

The tests check `serialize.js` in Node with a fake DOM (`host/test/fake-dom.mjs`) that has only the properties above. The fake DOM is built from parse5 nodes. The tests compare the string with the independent parse5 serializer of `host/tree.mjs` for the same tree, on hand-written expectations and on every Q-19 case in both views.

### Compare

- Q-19: the tree of the platform against the parse5 tree. `scriptingEnabled` is true for `on` and false for `off`. A leading byte order mark is removed before the parse, as a WebView does. Result: `equal`, `different` (with the first different line) or `unavailable` (walker unavailable).
- Q-20: `characterSet` and the text of the probe. Expected values:

| Case | Expected |
|---|---|
| `CT-UTF8`, `BOM-LATIN1`, `BOM-NONE`, `META-LATIN1` | `UTF-8` and the marker text. A byte order mark and a header charset come before a `<meta>` element. |
| `CT-LATIN1` | `windows-1252` (or `ISO-8859-1`) and the marker bytes read as windows-1252. The header charset wins. |
| `CT-NONE`, `CT-NO-CHARSET`, `CT-TWO-LINES` | No rule decides. Result `info` with the observed values. |

- A case with no result file gets `missing`.
- Exit code: 0 also when results differ. 1 for a malformed result file (not JSON, wrong `id` or `view`, bad `walker`, wrong type, no `tree` with walker `ok`). 2 for a usage error, a missing results directory, or a missing parse5.

## Checks

```sh
node --test tools/webview-lab/host/test/*.test.mjs
node tools/webview-lab/host/build-cases.mjs --check
```

## Android

The Android app is `app/WebViewLab/androidApp` (package `dev.rivqen.webviewlab`). It is a plain `Activity` with `android.webkit.WebView`. It does not use the shared Compose UI. The script `android-run.sh` does the full run: server, emulator, build, input, self-test, run, results, `run.json` and compare.

### Requirements

- Android SDK with `platform-tools` and `emulator` (default `~/Library/Android/sdk`, or set `ANDROID_HOME`).
- An AVD (default `Pixel_10_Pro_XL`, or set `AVD`), or a running emulator.
- JDK 21 for Gradle (set `JAVA_HOME`; the script uses the Gradle JDK when `JAVA_HOME` is not set).
- Node.js 22 and parse5 8.0.1 (section "Requirements" above). Install with `npm ci --ignore-scripts` in `tools/rqp-markup/crosscheck`.

### Procedure

1. Make sure that no other process uses port 18092.
2. Run `sh tools/webview-lab/android-run.sh`.
3. Wait for the summary line. A full run takes approximately 3 minutes, including the emulator start.
4. Read `results/android/summary.md` and `results/android/run.json`.

The script stops with an error when a step fails. It stops the lab server, and it stops the emulator only when the script started that emulator.

| Variable | Default | Use |
|---|---|---|
| `PORT` | `18092` | Lab server port. The emulator uses `10.0.2.2:<port>` |
| `AVD` | `Pixel_10_Pro_XL` | Emulator to start when no emulator runs |
| `EMU_PORT` | `5584` | Console port of the emulator that the script starts |
| `ANDROID_SERIAL` | first running emulator | Device to use |
| `SETTLE_MS` | `1000` | Wait after `onPageFinished` in view `on` (0 to 2000) |
| `CASES` | all | Comma-separated ids for a partial run (development only; other result files stay) |
| `RUN_TIMEOUT` | `1200` | Seconds for the app run |

### What the script does

1. Checks that `cases/` is current.
2. Starts `host/server.mjs --port 18092` on `127.0.0.1`.
3. Uses a running emulator, or starts the AVD with `-no-snapshot-save -no-window`.
4. Builds the debug APK (`./gradlew :androidApp:assembleDebug`) and installs it.
5. Grants `android.permission.ACCESS_LOCAL_NETWORK` with `adb shell pm grant` (Android 17 and later). Without it, the WebView load of `10.0.2.2` fails with `net::ERR_LOCAL_NETWORK_PERMISSION_MISSING`.
6. Copies `cases/*.json` and `host/serialize.js` into the app (`files/in/`, through `run-as`).
7. Runs the app self-test (`--ez selftest true`): checks of the input parser, the URL rule and the JSON code. The script stops when a check fails.
8. Starts the run (`--ei port 18092 --ei settleMs 1000`) and waits for `files/out/status.json`.
9. Copies `files/out/results/*.json` to `results/android/`, writes `results/android/run.json`, and runs `host/compare.mjs --platform android`.

### How the app reads the DOM

**FACT.** Measured on Android 17 (API 37) with Android System WebView 155.0.8059.43 and androidx.webkit 1.17.1.

- `serialize.js` runs only in an isolated JavaScript world of androidx.webkit (`WebViewCompat.getExecutionWorld`). The app adds one script with `WebViewCompat.addJavaScriptOnEvent` (`INJECTION_EVENT_DOCUMENT_END`) and a message listener with `WebViewCompat.addWebMessageListener` in that world. Page scripts do not share the world.
- View `on`: the lab-world script posts a ready message. After `onPageFinished` and `SETTLE_MS`, the app runs `serialize.js` with `JavaScriptReplyProxy.executeJavaScript` of that message.
- View `off`: the app sets `WebSettings.setJavaScriptEnabled(false)`. Nothing sets it to true.
  - `WebView.evaluateJavascript` does not run: the callback gets `null` in 47 of 47 rows. Chromium runs main-world scripts with the policy `kDoNotExecuteScriptWhenScriptsDisabled` (`third_party/blink/renderer/core/script/script.h`), and `WebContentsAndroid::EvaluateJavaScript` runs in the main world.
  - The androidx.webkit message object is not installed in any world, so there is no reply proxy. Chromium `components/js_injection/renderer/js_communication.cc` (`DidClearWindowObject`) stops when the frame has no main-world script context.
  - The lab-world script runs. Chromium runs isolated-world scripts with `kExecuteScriptWhenScriptsDisabled` (`third_party/blink/renderer/core/script/classic_script.cc`). In that world, `load` listeners, microtasks and `requestAnimationFrame` run; `setTimeout` callbacks do not.
  - So the lab-world script waits for the `load` event, runs `serialize.js`, and sends the result as `console.info` chunks to `WebChromeClient.onConsoleMessage`. Each chunk starts with `rivqen-lab:<nonce>:<index>/<count>/<length>:`. The nonce is random per WebView. The payload is ASCII JSON. The app checks the count and the length.
- Cross-checks in `run.json`: in view `on`, the tree of the console channel at `load` and the tree of `evaluateJavascript` in the page world must be equal to the result tree.

### Request rule

- The app loads only `http://10.0.2.2:<port>`. `shouldInterceptRequest` gives 403 with an empty body for every other request, and `shouldOverrideUrlLoading` blocks every other navigation. `run.json` counts the blocked requests per row (scheme and host only).
- The network security config allows cleartext only for `10.0.2.2`.
- The manifest turns off Safe Browsing look-ups and the WebView metrics upload, so the WebView contacts no other host.

### Known limits

- When `serialize.js` throws, the result has walker `unavailable` and the error text. The app still records `characterSet`, `contentType` and the probe (they are read first).
- The checks of the app run on the device (self-test), not in a JVM test framework: the lab uses no JUnit (third-party.md, WebView lab).
