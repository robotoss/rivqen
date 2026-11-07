# Legacy: client trace report

This page reports the **observed behavior** of the upstream VasSonic clients: Android session runs on an emulator in Quick and Standard mode, and the pure functions of Android and iOS. The results confirm or correct the code reading in [legacy client](/engineering/protocol/legacy-client) and [divergences](/engineering/protocol/legacy-divergences#_2-client-divergences) §2. A trace overrides code reading.

::: warning Legacy mode — temporary
This is evidence for the optional legacy module (deprecated from 1.0, removed in 1.5).
:::

**Status:** <Badge type="tip" text="FACT" /> Captured on 2026-11-05 (WP-01 S1 T-05: CS-01…CS-19, the functions) and on 2026-11-07 (WP-01 S2 T-05: CS-20…CS-27, and CS-13a and CS-13b again) from upstream code at `59936bef`, compiled and run unmodified. Evidence: `evidence/traces/client/` (`run.json`, `android/CS-*/<mode>/run-<k>.jsonl` and `.http`, `android/functions.jsonl`, `ios/functions.jsonl`, `ios/boolvalue.jsonl`). Check: `node tools/upstream-lab/client/check.mjs evidence/traces/client --require-functions`.

[[toc]]

## 1. Method

| Part | What it does |
|---|---|
| Two-sided recording (DL-099) | A Rivqen proxy between the device and the upstream server stores each exchange as `.http`. The Rivqen harness app sends one event per client call and callback (load calls, intercepted stream, result codes, page message, cache files with SHA-256) |
| Scripted order (DL-100) | The harness calls the SDK entry points (`createSession`, `bindClient`, `clientReady`, `requestResource`, `getDiffData`, `refresh`, `destroy`) in the order of the scenario. The proxy holds the response head or body, cuts the stream, drops a field or sets a field (`set_field`, S2) when the script says so. A free race is not evidence of a timing row |
| Lab pages | `a`, `b`, `c` (296, 296 and 315 bytes) and, from S2, `empty` (0 bytes) |
| Setup load | The macro `first_load` makes the cache before the scenario part. S1: free order. S2: it waits for the end of the body at the proxy (`proxy.response_end`) before `client_ready` (section 3) |
| Page message | Each scenario calls `getDiffData` after `request_resource` (the page callback is set) and again before the cache snapshot. The first call shows a result that the session sends at once ("notify"); the second shows what a later call gets |
| Normalization (CONTRACT §6) | Remove `t`, `seq`, worker thread names and the run number of `run.start`; blank `Date` and `X-Android-*-Millis`; drop `local_refresh_time` from the message; compare `.header` files by their normalized text |
| Two comparisons | **Strict** (CONTRACT §6): per step window, harness events in harness order. **Outcome projection** (CONTRACT §6, DL-104; `outcomeRun` in `tools/upstream-lab/client/src/determinism.mjs`): per phase (a phase ends with the step after a `settle`), conductor events in order, main-thread harness events in order, other harness events as a multiset, proxy events per exchange |
| Repeats | 3 runs per scenario and mode; CS-19 (real WebView smoke run) 1 run per mode, so it is not checked; CS-10 Quick 6 runs (section 3) |
| Functions (CONTRACT §7) | 214 cases of `tools/upstream-lab/client/functions.json` on both platforms; each platform run twice, byte-equal (S1; not run again in S2) |

<Badge type="tip" text="FACT" /> The strict rule assumes one order of harness events. The SDK sends callbacks from its session thread, its file thread and the intercept readers while the script goes on. So an event can fall into the next step window, or two harness threads can interleave, with no change of what the client did. The outcome projection compares what the client did (DL-104). A timing row is FACT when its runs are stable in the outcome projection; `run.json` records both results.

## 2. Environment

| Item | Value |
|---|---|
| Android image | `system-images;android-28;default;arm64-v8a` (Android 9, API 28), AVD `rivqen-api28`, cold boot, no snapshots |
| WebView | `com.android.webview` 66.0.3359.158 (AOSP) |
| Emulator, adb | 37.2.13.0; Android Debug Bridge 1.0.41 (platform-tools 37.0.1) |
| Harness build | JDK Zulu 21.0.11, Gradle 9.8.2, AGP 9.5.0; upstream SDK sources compiled unchanged (H-118). S2: the harness with `TallyStream` (one `client.intercept` per stream, under one lock) and the check of the step `seq`; APK 337 414 bytes, SHA-256 `bcabce59…318d92` |
| Servers | `lab-node` (default) and `lab-java` (CS-08, CS-10) of `tools/upstream-lab/start.sh`; Docker 29.9.2. S2: image `node:22-alpine` (Node.js 22.23.4, `sonic_differ` 1.0.7, Koa 2.16.4); `tomcat:9-jdk11` (Tomcat 9.0.123, JDK 11.0.32) |
| Conductor | Node.js v26.8.2; proxy `127.0.0.1:18090`, control `127.0.0.1:18091`; the emulator reaches them as `10.0.2.2` |
| iOS runner | macOS 26.6.2 (25G83), macOS SDK 27.0, Apple clang 21.0.0; 14 upstream files built unchanged with `-fno-objc-arc -fobjc-weak` (H-119) |
| Harness runtime choices | Synthetic account, cookie and user agents; resource cache in app storage; the harness waits until the engine is available (section 6) |

Measurements:

| Run | Result |
|---|---|
| S1 (2026-11-05) | 116 runs of `run.mjs --all` in 607 s (about 5.2 s per run), then 6 runs of CS-10 Quick |
| S2 (2026-11-07) | Lab rebuilt on `BackCat`: `env.sh` about 2.5 min (downloads included), `$LAB_HOME` 5.3 GiB; cold boot 36 s. 72 runs (CS-20…CS-27, CS-13a, CS-13b) in 413 s (about 5.7 s per run), then 2 modes again (3 runs each, section 3). 66 runs of the S1 modes with a setup load, outside the stored tree (section 3) |
| Stored | 179 runs, 505 files, 2 591 019 bytes (`android/CS-*`) |

## 3. Stability

| Result | Strict (CONTRACT §6) | Outcome projection |
|---|---|---|
| Stable scenario modes (58 modes with 3 or more runs) | 7 of 58 | 58 of 58 |
| Unstable | 51 (window or thread position of SDK events; reasons in `run.json`) | 0 |
| Not checked (1 run) | CS-19 Quick and Standard | CS-19 Quick and Standard |

<Badge type="tip" text="FACT" /> The 51 strict differences are all of one kind: an SDK event changes places with a step boundary or with an event of another harness thread (for example `proxy.response_head` before or after `step.start bind_client`, or `onSessionFirstLoad` before or after `step.done bind_client`). No client action, code, message or cache file differs. The 51 differences by kind:

| Events that change places | Modes |
|---|---|
| A proxy event (`proxy.request`, `proxy.response_head`, `proxy.hold`) and a step boundary (`bind_client`; once `wait`) | 32 |
| A session-thread callback or message and the end of `bind_client` on the main thread | 9 |
| `client.intercept` and a callback or the end of `request_resource` | 7 |
| `onSessionLoadLocalCache` (the SDK reads the cache from its constructor, on its thread) and `session.created` | 3 |

<Badge type="tip" text="FACT" /> CS-19 (the real WebView smoke run) has 1 run per mode by plan. Its stability is not checked (`run.json` `summary.not_checked`, S2). No Q or S row depends on CS-19 alone.

### 3.1 Setup loads

The macro `first_load` makes the cache before the scenario part.

| Macro | Setup loads in Quick | Result |
|---|---|---|
| S1 (free order) | CS-10, first set of 3 runs | 1 run had the setup load as Q2 (`loadUrl`, 1000 / 1000) instead of Q1 (data, 1000 / 304). 6 more runs: all equal; the 6 are stored, the variant run is not |
| S2 (`client_ready` after `proxy.response_end`) | 63 (CS-21…CS-27, the CS-24b repeat, the 22 S1 modes of section 3.3) | 62 as Q1. 1 as Q2 (CS-24b, first set, run 3): `client_ready` came at the same millisecond as `proxy.response_end` and 10 ms before `onSessionFirstLoad`. Its scenario part equalled run 1. The mode was run again (3 runs, all Q1); only the second set is stored |

<Badge type="tip" text="FACT" /> The S2 macro makes the Q2 setup rare, but it does not remove it: the end of the body at the proxy is not the end of the read in the SDK. A `settle` after the `wait`, as in the scenarios, would close the gap (proposal for CONTRACT §2).

### 3.2 Destroy of the setup session

<Badge type="tip" text="FACT" /> CS-26 Standard, first set, run 3 failed. At the `destroy` of the setup session the SDK still waited for a cache save, so it kept the session (device log `canDestroy:false … isWaitingForSaveFile=true`; `upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/SonicSession.java:936-942`, `:1437-1438`). Then `createSession` of the scenario returned `null` for the same id (device log "is running now", `upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/SonicEngine.java:273`), as in CS-15. This occurred in 1 of 126 setup loads of S2. The mode was run again (3 runs, all passed); only the second set is stored.

### 3.3 S1 runs kept

<Badge type="tip" text="FACT" /> The S2 lab fixes (`TallyStream`, the step `seq` check, the `first_load` macro, the proxy fixes) could change what a run of an S1 scenario records. So the 22 S1 modes with a setup load (CS-03…CS-11, CS-14, CS-16; 66 runs) were run again with the S2 lab, outside the stored tree. In the outcome projection, the scenario part (the steps after the setup) of every new run equals the stored S1 runs. Only the setup steps differ (the S2 macro). The other S1 modes have no setup load; CS-13a and CS-13b were run again and stored (section 3.4). So the S1 runs stay. A trace keeps the steps that ran, so a client fixture copies the setup steps of its own trace (`fixtures/legacy/README.md`).

### 3.4 Intercept stream (`RecordingStream` fix)

<Badge type="tip" text="FACT" /> In all 179 stored runs, each `request_resource` step has exactly one `client.intercept` event (CS-19: the WebView asks, no step), and no run has a `harness.error`. The length and SHA-256 of each of the 251 streams that are not `null` equal a prefix of a lab page (`a`, `b`, `c` or `empty`; the rebuilt page `b*` equals `b`). CS-13a again with the S2 harness (6 runs): 139 bytes, `java.io.EOFException`, the SHA-256 of the first 139 bytes of page `a`, one event per run, as in S1. CS-13a and CS-13b give the same outcome projection as in S1. A close during a read did not occur in the scripted runs; `tools/upstream-lab/android/check.sh` checks that case (11 checks; pass on 2026-11-07).

## 4. Scenario results

"Action" is what the session did on the client (`s2` = second session; `data(a)` = `loadDataWithBaseUrlAndHeader` with page `a`; `b*` = page `b` rebuilt from the cached template and the new data). "Message" lists the page messages of the two `getDiffData` calls (`src/final`). "Saves" counts `onSessionSaveCache` in the scenario part. Rows: [legacy client](/engineering/protocol/legacy-client) §3.

| ID | Mode | Action | Codes | Message | Saves; cache after |
|---|---|---|---|---|---|
| CS-01 | Q | data(a); intercept a | 1000/304 | 1000/304 twice | 2; 4 files |
| CS-01 | S | `loadUrl`; intercept a | 1000/304 | 1000/304 twice | 2; 4 files |
| CS-02 | Q | `loadUrl`; intercept a (bridge) | 1000/1000 | 1000/1000 at the result, again | 1; 4 files |
| CS-02 | S | `loadUrl`; intercept a | 1000/1000 | as Quick | 1; 4 files |
| CS-03 | Q | data(a); intercept `null` | 304/304 | 304/304 twice | 0; kept |
| CS-03 | S | `loadUrl`; intercept a (cache) | 304/304 | 304/304 twice | 0; kept |
| CS-04 | Q | data(a); intercept `null` | 200/200 | 200/200 with diff at the result, then `{}` | 1; 4 files (b) |
| CS-04 | S | `loadUrl`; intercept a (cache) | 200/200 | as Quick | 1; 4 files (b) |
| CS-05 | Q | data(b*); intercept `null` | 200/304 | 200/304 twice | 1; 4 files (b) |
| CS-05 | S | `loadUrl`; intercept b* | 200/304 | 200/304 twice | 1; 4 files (b) |
| CS-06 | Q | data(a); data(c); intercept c | 2000/2000 | none at the result; 2000/2000 later | 2; 4 files (c) |
| CS-06 | S | `loadUrl`; intercept a; `loadUrl` again | 2000/2000 | 2000/2000 at the result, again | 1; 4 files (c) |
| CS-07 | Q | data(c); intercept c | 2000/304 | 2000/304 twice | 2; 4 files (c) |
| CS-07 | S | `loadUrl`; intercept c | 2000/304 | 2000/304 twice | 2; 4 files (c) |
| CS-08 | Q | data(b*); intercept `null` | 200/304 | 200/304, `cache-offline` `true,http` | 0; removed |
| CS-08 | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 1; html, data, header (no tpl) |
| CS-09 | Q | data(a); intercept `null` | -1/-1 | none | 0; removed |
| CS-09 | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 0; removed |
| CS-10 | Q | data(b*); intercept `null` | 200/304 | 200/304, `cache-offline` `true,store` | 0; removed |
| CS-10 | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 1; html, data, header (no tpl) |
| CS-11 | Q | data(a); intercept `null` | -1/-1 | none; error 500 | 0; kept |
| CS-11 | S | `loadUrl`; intercept a (cache) | -1/-1 | none; error 500 | 0; kept |
| CS-12 | Q, S | `loadUrl`; intercept `null` | -1/-1 | none; error -901 | 0; none |
| CS-13a | Q, S | `loadUrl`; intercept 139 bytes, then `EOFException` (S1 and S2 runs equal) | 1000/1000 | 1000/1000 at the result, again | 0; none |
| CS-13b | Q, S | `loadUrl`; intercept `null` (S1 and S2 runs equal) | -1/-1 | none | 0; none |
| CS-14 | Q | data(a); intercept `null` | -1/-1 | none | 0; removed |
| CS-14 | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 0; removed |
| CS-15 | Q, S | second `createSession` returns `null`; first session as CS-02 | 1000/1000 | 1000/1000 | 1; 4 files |
| CS-16 | Q, S | new account: first load (no `If-None-Match`); account change while held: the session keeps account 2 and gets 304 | 1000/304; 304/304 | as codes | 2; 8 files (2 ids) |
| CS-17 | Q, S | first load, then 304; request cookie read at `createSession`; `Set-Cookie` of the 304 goes to the runtime | 1000/304; 304/304 | as codes | 2; 4 files |
| CS-18 | Q, S | destroy before the body end: intercept `null`; destroy 0.5 s after the end | -1/-1; 1000/304 | 1000/304 | 0; none |
| CS-19 | Q | WebView: data(b*); the WebView asks for a `data:` URL, gets `null`; page finished | 200/304 | 200/304 | 1; 4 files (b) |
| CS-19 | S | WebView: `loadUrl`; the WebView intercept gets b*; page finished | 200/304 | 200/304 | 1; 4 files (b) |
| CS-20 | Q | No load call; the intercept gets an empty stream | -1/-1 | none | 0; none |
| CS-20 | S | `loadUrl`; the intercept gets an empty stream | 1000/1000 | 1000/1000 twice | 0; none |
| CS-21 | Q | data(empty); the intercept gets an empty stream | 2000/304 | 2000/304 twice | 0; kept (a) |
| CS-21 | S | `loadUrl`; the intercept gets an empty stream | -1/-1 | none | 0; kept (a) |
| CS-22 | Q | data(a); intercept `null` | 200/304 | 200/304 at the result, again; no diff | 1; 4 files (b) |
| CS-22 | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 1; 4 files (b) |
| CS-23 | Q | data(a); intercept c | 2000/304 | 2000/304 at the result, again | 2; 4 files (c) |
| CS-23 | S | `loadUrl`; intercept a (cache) | 2000/304 | 2000/304 at the result, again | 1; 4 files (c) |
| CS-24a | Q | data(a); `loadUrl`; intercept `null` | -1/-1 | none | 0; kept (a) |
| CS-24a | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 0; kept (a) |
| CS-24b | Q | data(a); intercept `null` | 200/200 | 200/200 with diff at the result, then `{}` | 0; removed |
| CS-24b | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 1; html (b*), data, header (no tpl) |
| CS-24c | Q | data(a); intercept `null` | 200/200 | 200/200 with diff at the result, then `{}` | 0; removed |
| CS-24c | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 1; html (b*), data, header (no tpl) |
| CS-25 | Q | data(a); `loadUrl`; intercept `null` | -1/-1 | none | 0; kept (a) |
| CS-25 | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 0; kept (a) |
| CS-26 | Q | data(b*); intercept `null` (as CS-05) | 200/304 | 200/304 twice | 1; 4 files (b) |
| CS-26 | S | `loadUrl`; intercept b* (as CS-05) | 200/304 | 200/304 twice | 1; 4 files (b) |
| CS-27 | Q | data(c); intercept c; `refresh`: request with the etag and tag of c, server `304` | 304/304 | 2000/304 at the result; 304/304 with `isReload` `"true"` after the refresh | 1; 4 files (c); error -1005 |
| CS-27 | S | `loadUrl`; intercept c; `refresh` as Quick | 304/304 | as Quick | 1; 4 files (c); error -1005 |

Notes:

1. In Quick, the script asks for the main document also after a data load. A real WebView asks for a `data:` URL instead (CS-19), so it gets no session stream. The extra intercept adds a stream-close save in CS-01, CS-06, CS-07, CS-16, CS-17 (Quick).
2. CS-08 and CS-10 use `lab-java`. The upstream Node.js and PHP servers replace the application `cache-offline` with `true` (server trace S23). The Java server sends its own `Cache-Offline: true` and the application `cache-offline` as two fields. The Android stack merges them (names compare without case), and the SDK joins the values with `,`. The value `true,http` (or `true,store`) matches no rule of F6, F7 or F12.
3. CS-09 drops `cache-offline` in the proxy: the absent field takes the branch of `false` (F7).
4. CS-11 uses page `b`: with page `a` and the cached tag, the upstream middleware answers `304` before the application status.
5. S2 scenarios. CS-20 and CS-21 use the page `empty`: the Node.js server sends a `200` with 25 bytes of gzip and the etag and template tag `da39a3ee…` (the SHA-1 of no bytes). CS-22…CS-25 set `cache-offline` in the proxy (`set_field`; `store`, `HTTP`, `Store`, `TRUE`, `http`) on a data update while the cached page is shown first. The `.http` file of such an exchange has the upstream answer and a `--- delivered ---` section. The upstream answers are chunked (no `Content-Length`), and `set_field` does not change the body, so no length is wrong.
6. CS-26 sets the field `html-sha1` of the head to a wrong value. The result equals CS-05: for a data update the SDK takes `html-sha1` from the JSON body (`upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/QuickSonicSession.java:681`, `upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/StandardSonicSession.java:462`); the head field that it reads is `sonic-html-sha1` (`upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/SonicSessionConnection.java:107`). So Q11 is not reached.
7. CS-27: the scripted intercept reads page `c` to the end before `refresh`, so a stream-close save is pending. The refresh request has the etag and template tag of page `c`, and the server answers `304`. So the refresh has no template change (Q19 and S16 not reached).

## 5. Rows and probable defects

### 5.1 Row results

| Rows | Result |
|---|---|
| Q1, Q2, Q8, Q10, Q12, Q14, Q22; S1, S6, S7, S9, S11, S17 | <Badge type="tip" text="FACT" /> Timing rows confirmed in the scripted order of their scenario (outcome-stable, 3 runs) |
| Q4, Q7, Q15, Q17, Q18, Q20; S2, S3, S5, S8, S13, S14, S15 | <Badge type="tip" text="FACT" /> Code-reading rows confirmed (S8 with "another value": `true,http`, `true,store`) |
| Q3; S12 (S2) | <Badge type="tip" text="FACT" /> Empty body. Q3: no load call, -1 / -1, no cache (CS-20). S12: `loadUrl`, the intercept gets the empty new response, no message, -1 / -1, the cache is kept (CS-21) |
| Q9, Q13; S8, S10 (S2) | <Badge type="tip" text="FACT" /> `cache-offline: store` alone with the cached page shown first. Q9: no load call, 200 / 304 without a diff, the update is written (CS-22). Q13 and S10: no load call, 2000 / 304, the new page is written (CS-23). S8: no message, -1 / -1, the update is written (CS-22) |
| Q16; S4 (S2) | <Badge type="tip" text="FACT" /> `cache-offline: http` (CS-25) and `HTTP` (CS-24a) with a cache: Quick loads the URL once, Standard keeps the cached page as the stream; -1 / -1; the cache is kept |
| Q8; S8 with `TRUE` and `Store` (S2) | <Badge type="tip" text="FACT" /> D-30 (CS-24b, CS-24c): Quick sends the diff (200 / 200) and removes the cache; Standard sends no message (-1 / -1) and writes the cache with no template file |
| Q14 with an empty new page (S2) | <Badge type="tip" text="FACT" /> CS-21 Quick: the empty page is given as data (0 bytes), 2000 / 304, and the cache is kept. The row says `loadUrl` "with no page text"; an empty page takes the data path |
| Q11 | <Badge type="warning" text="RESEARCH" /> Not reached: CS-26 changed the head field `html-sha1`, but the SDK checks the value of the body (section 4, note 6) |
| Q19; S16 | Not reached: in CS-27 the refresh has no template change (section 4, note 7). Q19 code reading stands; S16 stays <Badge type="warning" text="RESEARCH" /> |
| Q5, Q6, Q21; S18 | Not in the catalog; code reading stands |

### 5.2 Probable upstream defects

| Defect | Trace result |
|---|---|
| Quick first load as data skips the cache write (Q1 note 1) | **Refuted for the order of CS-01**: the body was complete before `clientReady`, the first-load message waited in the slot, and the 1.5 s save ran (3 of 3 runs). S2: also not seen in the 62 setup loads that took Q1 (the next session found the cache). <Badge type="warning" text="RESEARCH" /> The remaining window is `clientReady` between `QSS:628` and `QSS:638` on the session thread; a script cannot place a step there |
| Quick empty `200` body loads nothing (Q3 note 2) | <Badge type="tip" text="FACT" /> **Confirmed** (CS-20, 3 of 3 runs): the session makes no load call (no `loadUrl`, no data), the codes stay -1 / -1, no message, no cache. Only the scripted intercept gets an empty stream; a web view that waits for the session gets nothing to load |
| Quick data update lost (Q8 note 4) | Not seen: cache first (CS-04; S2: CS-22, CS-24b, CS-24c) delivered the result, network first (CS-05, CS-26) loaded the rebuilt page. <Badge type="warning" text="RESEARCH" /> The window is the preload on the main thread between `QSS:697` and `QSS:723` |
| Standard 304 with the old cache (S7 note 3) | Not seen: in CS-05 and CS-26 the intercept got the rebuilt page. <Badge type="warning" text="RESEARCH" /> The window is an intercept between `STD:470` and `STD:472` |
| iOS local-server data update (D-39) | <Badge type="warning" text="RESEARCH" /> No iOS session run (H-119). The `diff` function confirms only that the merge gives an empty diff when the server map has no local key (FN-DIFF-NEW-04); the full-data fallback is in the cache code, which the runner does not call |
| **New (S2).** Quick template change with an empty new page | <Badge type="tip" text="FACT" /> CS-21 Quick (3 of 3 runs): the session gives the empty page to the web view as data and reports 2000 / 304, but it keeps the old cache (page `a`) and saves nothing. The page shows nothing while the result says the new page is shown. Standard (S12) keeps the cached page as the stream instead |
| **New (S2).** A refresh before the delayed save of a template change | <Badge type="tip" text="FACT" /> CS-27 (3 of 3 runs per mode): the refresh gets its `304` about 0.3 s after the template change. About 1.5 s after the template change (the delayed save), the session reports error -1005 (`upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/SonicSession.java:1056`) and writes nothing; about 1.5 s after the stream close, one save writes page `c`. <Badge type="warning" text="RESEARCH" /> Quick with a real web view gives the page as data and has no stream close, so the new page would not be stored; not run |

## 6. New findings

<Badge type="tip" text="FACT" /> Not visible from code reading alone, or not stated in the pages before T-05:

1. **First data update after a first load sends every block (Android).** The first-load save stores the data file as `{"data": {…}, "html-sha1", "template-tag"}` (`SV:436-446`); the data update reads it as a flat block map (`SU:251-299`). So no local key is found, and the diff holds all blocks: CS-04 sends `{title}`, `{price}` and `{stock}`, though only `{price}` changed. The update then stores the flat map (`QSS:755`, `STD:507`), so a second update compares correctly.
2. **A second `getDiffData` after a data-update message gives `{}` (Android).** The first send clears the stored diff (`SS:1162`); the next `setResult` with final `200` fails to parse it, the error is caught (`SS:1148`), and the page gets the empty object (CS-04, both modes).
3. **Standard leaves a cache without a template.** With a `cache-offline` value that F12 refuses (`true,http`), Standard removes the cache and then writes the page, data and header files, but no template file (CS-08, CS-10). What the next session does with it is <Badge type="warning" text="RESEARCH" />.
4. **Repeated fields (D-33).** On API 28 two fields that differ only in case become one list, which the SDK joins with `,` (CS-08, CS-10).
5. **Headers given to the WebView.** The intercepted response and the data call keep `content-encoding: gzip` and `transfer-encoding: chunked` of the network answer, but the stream is decoded (CS-01 and later). The AOSP WebView 66 showed the page (CS-19). Other WebViews: <Badge type="warning" text="RESEARCH" />.
6. **Stream cut.** A cut chunked body (128 wire bytes of gzip) gives the WebView 139 decoded bytes and then `java.io.EOFException`; no cache is written (CS-13a).
7. **Page message (D-26).** `isReload` is the string `"false"`; an absent header removes its `extra` key (304 messages have only `cache-offline` and `isReload`); org.json writes `/` as `\/` in `result`; the keys keep insertion order (`local_refresh_time`, `result`, `code`, `srcCode`, `extra`).
8. **Default runtime (trial run, before the harness change).** The default resource cache directory is on external storage. Without the storage permission each session with a cache reported `notifyError` `-1003` at a varying time. The harness now uses app storage.
9. **Engine start (trial run).** Right after the first `SonicEngine.createInstance` on a new database, `createSession` returned `null` ("sonic service is unavailable") while the database upgrade task ran (1 of 40 runs). The harness now waits until the engine is available.

<Badge type="tip" text="FACT" /> Added by the S2 run (2026-11-07):

10. **Empty first load in Standard.** `loadUrl`; the intercept gets an empty stream; 1000 / 1000 at the result and again; no save and no cache (CS-20).
11. **`html-sha1` of a data update.** The SDK takes it from the JSON body, not from a head field; a wrong head field `html-sha1` changes nothing (CS-26 equals CS-05; section 4, note 6).
12. **Refresh request.** After a template change, the `refresh` request has the etag and template tag of the new response, about 1.5 s before any save writes them (CS-27). The Node.js server answers `304` with `Cache-Offline: store`. The page message after the refresh is `{"code":304,"srcCode":304,"extra":{"cache-offline":"store","isReload":"true"}}`: the first trace of `isReload` `"true"` (D-26).
13. **`cache-offline: http` callbacks.** The SDK reports `onSessionUnAvailable`, once in Quick and twice in Standard (CS-24a, CS-25). SDK callbacks are not part of a client fixture (DL-110).
14. **Lab order limits.** A setup load can still take Q2 with the S2 macro, and the SDK can defer the `destroy` of a setup session while a save waits (section 3.1, 3.2).

## 7. Function results

Results per D-row. Case IDs are in `tools/upstream-lab/client/functions.json`. Android: API 28 image; iOS: Foundation of macOS 26.6.2.

| Row | Cases | Result |
|---|---|---|
| D-20 | FN-ID-01…14 | <Badge type="tip" text="FACT" /> Both keep the host case (FN-ID-03 differs from FN-ID-01). Android keeps the port (FN-ID-05) and drops the scheme (FN-ID-04); iOS the reverse. Android: `null` account → `null_`, empty → `_` (FN-ID-08, -09). Android hashes UTF-8 (FN-ID-13 equals the MD5 of the UTF-8 bytes) |
| D-21 | FN-ID-ORD-01…14 | <Badge type="tip" text="FACT" /> Android: sorted names, first value, decoded (`%41%20b` → `A b`, `+` → space; ORD-03, ORD-04). iOS: last value, raw (ORD-03, ORD-04). iOS order on this Foundation: query order (ORD-01, ORD-02), not a contract |
| D-22, D-23 | FN-SPLIT-01…12, -NAME-01…08, -UNNAMED-01…06 | <Badge type="tip" text="FACT" /> Android: upper-case markers give no block; unnamed marker → `{null}`; name from the opening marker. iOS: upper-case markers match, but the key is the whole match cut at `-end-->` (`{<!--SONICDIFF-a-->A<!--SONICDIFF-a}`); unnamed marker → `{end-->}`; name from the closing marker |
| D-24 | FN-SPLIT-TITLE-01…09 | <Badge type="tip" text="FACT" /> Android: the first literal `<title>` element becomes `{title}`, only when a block exists; `<TITLE>` and `<title lang>` are not taken. iOS: no title step. Android returns an empty template when no block is found (FN-SPLIT-11) |
| D-25, D-35 | FN-DIFF-01…12, -NEW-01…04 | <Badge type="tip" text="FACT" /> Android includes new keys unless empty (DIFF-03, -04), compares as strings (DIFF-07, -08). iOS: only existing keys; a number value makes the merge throw (DIFF-07, -08: `NSInvalidArgumentException`) |
| D-34 | FN-DIFF-EMPTY-01…04, -IOS-01…04 | <Badge type="tip" text="FACT" /> The merge functions of both give `{}`; the iOS full-data fallback is in the cache code (not run) |
| D-28 | FN-LEGACY-01…10, -CASE-01…05 | <Badge type="tip" text="FACT" /> iOS needs all four fields, names in any case. Android: `unsupported` (private method) |
| D-29 | FN-TCHANGE-01…25 (`ios/boolvalue.jsonl`) | <Badge type="tip" text="FACT" /> macOS 26.6 `boolValue`: `true` for `true`, `TRUE`, `True`, `yes`, `y`, `t`, `1`, `2`, `-1`, ` true`; `false` for `false`, `False`, `no`, `0`, `00`, `0.0`, `null`, `abc`, empty and space. Both platforms: `template_change` `unsupported` |
| D-31, D-32 | FN-ETAG-01…19, -WEAK-01…12, -KEY-01…06 | <Badge type="tip" text="FACT" /> Android: a value that starts with `w/` (any case) is lowercased and loses every `w/` and `"` (ETAG-15 → `ab`). iOS: exact `W/` and length over 3 (`W/x` stays); outer quotes only as a pair. The custom key (D-32) is not covered: both runners read the default field |
| D-44 | FN-CHARSET-01…17, -PARAM-01…08 | <Badge type="tip" text="FACT" /> Android default `UTF-8`; it keeps quotes and unknown names (CHARSET-09, -16). iOS: an unknown, quoted or cut name gives UTF-8; a following parameter breaks the name (PARAM-02 `gbk; q=1` → UTF-8) |
| D-26, D-30, D-39, D-48 | FN-MSG-*, FN-MSG-OFFLINE-*, FN-MSG-STORE-* | `unsupported` on both (private session state); D-26 is observed in the session runs (section 6) |

Non-public calls: Android `package-access.json` (6 members), iOS 4 calls; both in `run.json` `functions`.

## 8. Limits

- **API 28 only.** The platform libraries (org.json, `android.net.Uri`, the HTTP stack, WebView 66) of other Android versions can differ.
- **macOS Foundation, not old iOS.** The iOS results are for the Foundation of macOS 26.6.2. `NSURL`, `NSDictionary` order and `boolValue` of the iOS versions of 2017–2019 are NOT FOUND. The iOS session flow stays code reading (H-119).
- **Scripted order, not a real WebView.** The harness drives the session; one smoke run per mode uses the real WebView (CS-19). See note 1 of section 4.
- **Proxy buffering.** The proxy reads the whole upstream answer before it delivers it. A partial body reaches the client only through a `hold` at `body` or a `cut`.
- **Upstream servers.** They do not send `cache-offline` values other than `true` (Node.js, PHP) or `true` plus a second field (Java). From S2 the proxy step `set_field` sets the head field (CS-22…CS-25). `false` alone is not in the catalog.
- **Head only.** `set_field` changes the response head, not the body. A row that needs another body (Q11: a wrong `html-sha1` in the JSON of a data update, or a missing template) is not reached.
- **One application response per scenario part.** The `case` is set before the steps; a step cannot change it. A refresh that gets a template change (Q19, S16) needs a first answer without the change and a second answer with it, so it is not reached (CS-27).
- **Setup order.** The S2 `first_load` macro waits for the end of the body at the proxy, not in the SDK (section 3.1), and a setup session can stay alive after `destroy` while a save waits (section 3.2). A failed or variant set is run again; it is not stored.
- **Thread windows.** The windows of the defects in section 5.2 are inside the SDK threads; a script cannot place a step there.

## 9. Reproduce

1. Start the servers: `UPSTREAM=<clone> WORK=<directory outside the repository> sh tools/upstream-lab/start.sh`. Without `WORK`, the script writes `tools/upstream-lab/.work`, which git does not ignore.
2. Create the lab, build, boot and install: `sh tools/upstream-lab/android/env.sh`, `UPSTREAM=<clone> sh tools/upstream-lab/android/build.sh`, `sh tools/upstream-lab/android/emulator.sh start`, `sh tools/upstream-lab/android/install.sh`.
3. Run the scenarios: `node tools/upstream-lab/client/run.mjs --all --adb <lab adb> --out evidence/traces/client`. For one scenario: `--scenario <CS-id>`. A run that fails is stored too; run its scenario and mode again before the check.
4. Run the functions: `sh tools/upstream-lab/android/functions.sh tools/upstream-lab/client/functions.json evidence/traces/client/android/functions.jsonl`; `sh tools/upstream-lab/ios/build.sh`; `sh tools/upstream-lab/ios/run.sh tools/upstream-lab/client/functions.json evidence/traces/client/ios/functions.jsonl`; `sh tools/upstream-lab/ios/probe/run.sh tools/upstream-lab/client/functions.json evidence/traces/client/ios/boolvalue.jsonl`.
5. Check: `node tools/upstream-lab/client/check.mjs evidence/traces/client --require-functions`.
6. Stop: `sh tools/upstream-lab/android/emulator.sh stop`; `docker rm -f lab-node lab-php lab-java`.

## Related

- [Legacy client behavior](/engineering/protocol/legacy-client)
- [Divergences](/engineering/protocol/legacy-divergences)
- [Server trace report](/engineering/protocol/legacy-traces)
