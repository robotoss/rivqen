# Client lab contract (WP-01 S1)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

This file fixes the interfaces between the parts of the client lab: the conductor (`tools/upstream-lab/client/`), the Android harness (`tools/upstream-lab/android/`), the iOS function runner (`tools/upstream-lab/ios/`) and the stored evidence (`evidence/traces/client/`). The architect owns this file. A lane that needs a change writes a shared-file request.

Decisions: H-118 (Rivqen harness around the unmodified Android SDK sources), H-119 (iOS pure functions only), DL-099 (two-sided recording), DL-100 (scripted order), DL-102 (environment), DL-103 (offline checks). Sprint record: `docs/engineering/plan/sprints/WP-01-S1.md`.

> **Clean-room.** Upstream code is read from a read-only clone outside the repository and compiled unchanged. Nothing of upstream is copied here: no code, no comments, no test code, no sample code or HTML. The lab parts are written from the Rivqen pages (`docs/engineering/protocol/legacy-client.md`) and from the public entry points of the SDK (class and method names are interface facts, `docs/legal/provenance.md` CR-03). Do not read `sonic-android/sample/`, `sonic-android/sdk/src/androidTest/` or `sonic-iOS/SonicSample/`.

## 1. Topology

```text
emulator (API 28)                        host (127.0.0.1)
┌───────────────────┐   10.0.2.2:18090   ┌──────────────────────────────┐   127.0.0.1:18081
│ Android harness   │ ─────────────────▶ │ conductor: recording proxy   │ ───────────────▶ lab-node (upstream sonic_differ)
│ (upstream SDK +   │                    │            + step control    │   127.0.0.1:18081/__rivqen/conformance/case
│  Rivqen runtime/  │ ◀─── 18091 ──────▶ │            + scenario runner │ ───────────────▶ (sets the application response)
│  client classes)  │   steps / events   └──────────────────────────────┘
└───────────────────┘
```

| Port (host) | Listener | Use |
|---|---|---|
| 18081…18083 | `lab-node`, `lab-php`, `lab-java` (`start.sh`) | Upstream servers; the default origin of a scenario is `lab-node` |
| 18090 | Conductor proxy | The only origin the harness talks to. The page URL of every scenario is `http://10.0.2.2:18090/catalog` (the emulator maps `10.0.2.2` to the host loopback) |
| 18091 | Conductor control | Step and event channel of the harness (section 4) |

All listeners bind `127.0.0.1` only.

## 2. Scenario file

`tools/upstream-lab/client/scenarios.json` (owned by the conductor lane) is an array of scenarios:

```json
{
  "id": "CS-05",
  "title": "Data update; the network answer is complete before the cached page is loaded",
  "modes": ["quick", "standard"],
  "origin": "lab-node",
  "rows": ["Q10", "S7"],
  "setup": [ { "case": { "page": "a" } }, "first_load" ],
  "case": { "page": "b" },
  "config": { "account": "rq-test-account", "account_related": true, "reload_in_bad_network": false },
  "steps": [
    { "do": "create_session" },
    { "do": "bind_client" },
    { "do": "wait", "for": "proxy.response_end" },
    { "do": "settle", "ms": 300 },
    { "do": "client_ready" },
    { "do": "request_resource" },
    { "do": "settle", "ms": 2500 },
    { "do": "snapshot_cache" },
    { "do": "destroy" }
  ],
  "repeat": 3
}
```

| Member | Rule |
|---|---|
| `id` | `CS-<NN>` with an optional letter (`CS-13a`); unique |
| `modes` | Subset of `quick`, `standard`. One run per mode and repeat |
| `origin` | `lab-node`, `lab-php`, `lab-java` |
| `rows` | Row IDs of the Q-02 tables in `docs/engineering/protocol/legacy-client.md` that the scenario observes (`Q1`…, `S1`…) |
| `setup` | Steps that run first with an empty app state (`pm clear` before each run). `"first_load"` is a macro: a full first load with `client_ready`, `request_resource`, the end of the body, 2500 ms settle and `destroy` |
| `case` | The application response that the conductor posts to the control endpoint of `origin` before `steps` (`page`, optional `status`, `headers` as in `fixtures/legacy/*/case.json` `app`) |
| `config` | Session and SDK settings the harness applies. Account values are synthetic (`rq-test-account`, `rq-test-account-2`) |
| `steps` | The scripted order (DL-100); vocabulary in section 3 |
| `repeat` | Number of runs per mode; at least 3 for a timing row |
| `url` | Optional page URL; must start with `http://10.0.2.2:18090/`; default `http://10.0.2.2:18090/catalog` |
| `webview` | Optional, default `false`. `true`: a real WebView drives the session (smoke run, DL-100); the harness then refuses `request_resource` and `page_finished` |

The `config` keys that the harness accepts are listed in `tools/upstream-lab/android/README.md`; an unknown key is an error.

## 3. Step vocabulary

| Step | Who executes | Effect |
|---|---|---|
| `create_session` | Harness | Create the session for the page URL through the engine with the scenario `config` and mode. Logs `session.created` with `null` when the engine refuses |
| `bind_client` | Harness | Bind the recording client to the session |
| `client_ready` | Harness | Tell the session that the web view is ready |
| `request_resource` | Harness | Ask the session for the main document as the web view would (intercepted request). The harness reads the returned stream to the end on a worker thread and logs it |
| `page_finished` | Harness | Report that the page finished loading |
| `get_diff` | Harness | Ask for the page message (the JS bridge call) and log the answer |
| `refresh` | Harness | Reload the session |
| `destroy` | Harness | Destroy the session |
| `set_account` | Harness | Change the account that the runtime returns (`value`) |
| `set_cookie` | Harness | Change the cookie that the runtime returns for the page URL (`value`, synthetic only) |
| `set_network` | Harness | Change the network validity that the runtime reports (`valid: true/false`) |
| `hold` | Conductor | Hold the next proxy exchange at `point`: `request` (before forwarding), `head` (after the response head), `body` (after `bytes` body bytes) |
| `release` | Conductor | Release the current hold |
| `cut` | Conductor | Close the client connection of the current exchange after `bytes` body bytes (stream cut) |
| `drop_field` | Conductor | Remove the response field `name` before delivery (fault injection; the trace keeps both forms) |
| `server_down` | Conductor | Refuse connections on 18090 until `server_up` |
| `server_up` | Conductor | Accept connections again |
| `wait` | Conductor | Wait for an event (`for`: an event name of section 5, optional `timeout_ms`, default 30000) |
| `settle` | Conductor | Wait `ms` milliseconds |
| `snapshot_cache` | Harness | Log the cache directory (`cache.snapshot`) |

Optional members of a harness step: `session` (1-based index of the session the step acts on; default the last created) and, for `request_resource`, `wait_ms` (default 1000).

A step runs only after the step before it is acknowledged. Harness steps are acknowledged when the harness reports `step.done`. `request_resource` is acknowledged when the call returns or after `wait_ms`, whichever is first (a Quick session can block the call until the network answers; the conductor must be able to send `release` meanwhile). The stream is read to the end on a worker thread after that.

## 4. Control channel (port 18091)

HTTP/1.1, JSON bodies, `Content-Type: application/json`.

| Request | Answer |
|---|---|
| `GET /run/next?run=<run-id>` | Long poll (max 30 s). `200` with the next harness step `{ "seq": n, "do": "...", ... }`, `200` with `{ "do": "end" }` at the end of the run, `204` on poll timeout |
| `POST /run/event` | One event (section 5). `204` |
| `GET /run/scenario?run=<run-id>` | `200` with `{ "run": "<run-id>", "scenario": <scenario>, "mode": "quick" }` |

The runner starts the harness for one run with an `adb shell am instrument` or `am start` command that passes only `run=<run-id>` and the control URL `http://10.0.2.2:18091`. The harness gets everything else from the control channel.

## 5. Event log

One JSON Lines file per run: `evidence/traces/client/android/<scenario>/<mode>/run-<k>.jsonl`. One object per line, in the order the conductor received them; keys sorted; UTF-8; LF.

```json
{"ev":"client.load_data","seq":7,"src":"harness","t":412,"thread":"main","base_url":"http://10.0.2.2:18090/catalog","mime":"text/html","encoding":"utf-8","history_url":"http://10.0.2.2:18090/catalog","headers":[["content-type","text/html; charset=utf-8"]],"body_len":1342,"body_sha256":"…"}
```

Common members: `ev` (name), `seq` (conductor sequence number), `src` (`harness`, `proxy`, `conductor`), `t` (milliseconds since the run start, conductor clock), `thread` (harness only: `main` or a worker name), `hseq` (harness only: the order in which the harness emitted the event), `session` (on `session.*` and `client.*` events: the 1-based session index).

| Event | Members |
|---|---|
| `run.start` | `scenario`, `mode`, `repeat`, `origin`, `upstream_sha` (`src`: `conductor`) |
| `step.start`, `step.done` | `step` (the step object), `error` (on failure) |
| `session.created` | `session_id` (the upstream id; synthetic account only) or `null` |
| `client.load_url` | `url`, `extras` (keys only) |
| `client.load_data` | `base_url`, `mime`, `encoding`, `history_url`, `headers`, `body_len`, `body_sha256` |
| `client.intercept` | `status`, `reason`, `mime`, `encoding`, `headers`, `body_len`, `body_sha256`, `complete` (`true` when the stream ended without error), `error` (exception class name) or `null` stream (`"null": true`) |
| `session.result` | `src_code`, `final_code` (from the session callback) |
| `session.message` | `json` (the page message string exactly as the SDK built it) |
| `session.callback` | `name` (session callback method name), `args` (codes or URL only) |
| `runtime.cookie_get`, `runtime.cookie_set` | `url`, `value` (synthetic values only) |
| `cache.snapshot` | `files`: `[{"name","size","sha256"}]`, sorted by name; a `*.header` file also has `text` (its content: synthetic header lines of the lab); `db`: `null` (the harness does not read the SDK database) |
| `proxy.request` | `exchange`, `method`, `target`, `fields` |
| `proxy.response_head` | `exchange`, `status`, `fields` |
| `proxy.response_end` | `exchange`, `body_len`, `body_sha256`, `complete` |
| `proxy.cut`, `proxy.drop_field`, `proxy.hold`, `proxy.release` | `exchange`, the step members |
| `client.clear_history` | — |
| `runtime.notify_error` | `url`, `code` |
| `runtime.toast` | `text`, `duration` |
| `webview.intercept_request`, `webview.page_started`, `webview.page_finished` | `url` (WebView runs only) |
| `harness.error` | `error` |
| `run.end` | `ok`, `error` (`src`: `conductor`; the harness sends `run.end` with `ok: false` only when it aborts) |

Each proxy exchange is also stored as `run-<k>-x<exchange>.http` in the format of `evidence/traces/server/` (request, `--- response ---`, decoded body). A `drop_field` or `cut` exchange stores the upstream response and adds a `--- delivered ---` section.

Never log: real hosts, real accounts, real cookies, tokens, device identifiers. The harness writes no device log except under the tag `RivqenLab`.

## 6. Determinism

A run is stable when its normalized event log equals the other runs of the same scenario and mode.

Normalization: remove `t`, `seq` and the `thread` of worker threads; blank the values of the fields `Date`, `X-Android-Sent-Millis` and `X-Android-Received-Millis` (the Android HTTP stack adds the last two); remove `local_refresh_time` from the JSON of `session.message`; for a `*.header` cache file compare its `text` with the same fields blanked instead of its `sha256` and `size`.

Comparison: split the log into step windows (from one `step.start` to the next). Inside a window compare the harness events in `hseq` order and the proxy events in `exchange` order, each source on its own. The interleaving of the sources inside a window is not compared: it races (for example `runtime.cookie_get` and `proxy.request`). `check.mjs` (conductor lane) reports every scenario and mode whose runs differ. A timing row is FACT only when its 3 runs are stable.

## 7. Function cases

Pure functions run without a session: Android in the harness (instrumentation mode `functions`), iOS in the macOS runner. Input: `tools/upstream-lab/client/functions.json` (owned by the conductor lane), an array of cases:

```json
{ "id": "FN-ID-07", "fn": "session_id", "input": { "url": "http://Shop.example.com:8080/p?sonic_a=1&sonic_a=2", "account": "rq-test-account", "account_related": true }, "divergences": ["D-20", "D-21"] }
```

| `fn` | Input | Output |
|---|---|---|
| `session_id` | `url`, `account`, `account_related` | `id` (string) |
| `split` | `html` | `template`, `data` (object, key order as returned) |
| `diff` | `local` (object or `null`), `server` (object) | `diff` (object) |
| `message` | `src_code`, `final_code`, `diff`, `fields` | `json` (string, exact bytes) |
| `is_legacy_response` | `fields` | `value` (boolean) |
| `template_change` | `value` | `value` (boolean: template change) |
| `etag` | `value` | `value` (normalized string) |
| `charset` | `content_type` | `value` |

Output: one JSON Lines file per platform, `evidence/traces/client/<android|ios>/functions.jsonl`, one object per case: `{ "id", "fn", "platform", "result" }` or `{ "id", "fn", "platform", "error" }`, `"unsupported": "<reason>"` when the platform has no callable function for `fn`. A runner never changes upstream files or visibility to reach a function (no reflection that changes access), and never writes a field of an upstream object. A harness class in the same package may call a package-private or protected member; the runner lists such calls in `run.json`. A function that needs session state set by hand (for example the page message) is `unsupported`: the page message is observed only in session runs (`session.message`).

Android transport: `tools/upstream-lab/android/functions.sh` copies the case file into the app and the result file out. `fields` is an object or a list of `[name, value]` pairs. A function that is not reachable this way is `unsupported` and stays code reading.

## 8. Evidence layout

```text
evidence/traces/client/
├── android/
│   ├── functions.jsonl
│   └── CS-<NN>/<quick|standard>/run-<k>.jsonl, run-<k>-x<n>.http
├── ios/
│   └── functions.jsonl
└── run.json            # date, tool versions, emulator image, upstream SHA, scenario list, stable/unstable summary
```
