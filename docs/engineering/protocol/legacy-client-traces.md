# Legacy: client trace report

This page reports the **observed behavior** of the upstream VasSonic clients: Android session runs on an emulator in Quick and Standard mode, and the pure functions of Android and iOS. The results confirm or correct the code reading in [legacy client](/engineering/protocol/legacy-client) and [divergences](/engineering/protocol/legacy-divergences#_2-client-divergences) §2. A trace overrides code reading.

::: warning Legacy mode — temporary
This is evidence for the optional legacy module (deprecated from 1.0, removed in 1.5).
:::

**Status:** <Badge type="tip" text="FACT" /> Captured on 2026-11-05 from upstream code at `59936bef`, compiled and run unmodified (WP-01 S1 T-05). Evidence: `evidence/traces/client/` (`run.json`, `android/CS-*/<mode>/run-<k>.jsonl` and `.http`, `android/functions.jsonl`, `ios/functions.jsonl`, `ios/boolvalue.jsonl`). Check: `node tools/upstream-lab/client/check.mjs evidence/traces/client`.

[[toc]]

## 1. Method

| Part | What it does |
|---|---|
| Two-sided recording (DL-099) | A Rivqen proxy between the device and the upstream server stores each exchange as `.http`. The Rivqen harness app sends one event per client call and callback (load calls, intercepted stream, result codes, page message, cache files with SHA-256) |
| Scripted order (DL-100) | The harness calls the SDK entry points (`createSession`, `bindClient`, `clientReady`, `requestResource`, `getDiffData`, `destroy`) in the order of the scenario. The proxy holds the response head or body, cuts the stream or drops a field when the script says so. A free race is not evidence of a timing row |
| Page message | Each scenario calls `getDiffData` after `request_resource` (the page callback is set) and again before the cache snapshot. The first call shows a result that the session sends at once ("notify"); the second shows what a later call gets |
| Normalization (CONTRACT §6) | Remove `t`, `seq`, worker thread names and the run number of `run.start`; blank `Date` and `X-Android-*-Millis`; drop `local_refresh_time` from the message; compare `.header` files by their normalized text |
| Two comparisons | **Strict** (CONTRACT §6): per step window, harness events in harness order. **Outcome projection** (`outcomeRun` in `tools/upstream-lab/client/src/determinism.mjs`, a T-05 addition): per phase (a phase ends with the step after a `settle`), conductor events in order, main-thread harness events in order, other harness events as a multiset, proxy events per exchange |
| Repeats | 3 runs per scenario and mode; CS-19 (real WebView smoke run) 1 run per mode; CS-10 Quick 6 runs (section 3) |
| Functions (CONTRACT §7) | 214 cases of `tools/upstream-lab/client/functions.json` on both platforms; each platform run twice, byte-equal |

<Badge type="warning" text="RESEARCH" /> The strict rule assumes one order of harness events. The SDK sends callbacks from its session thread, its file thread and the intercept readers while the script goes on. So an event can fall into the next step window, or two harness threads can interleave, with no change of what the client did. The outcome projection compares what the client did. It is a proposal for CONTRACT §6 (architect decision); `run.json` records both results.

## 2. Environment

| Item | Value |
|---|---|
| Android image | `system-images;android-28;default;arm64-v8a` (Android 9, API 28), AVD `rivqen-api28`, cold boot, no snapshots |
| WebView | `com.android.webview` 66.0.3359.158 (AOSP) |
| Emulator, adb | 37.2.13.0; Android Debug Bridge 1.0.41 (platform-tools 37.0.1) |
| Harness build | JDK Zulu 21.0.11, Gradle 9.8.2, AGP 9.5.0; upstream SDK sources compiled unchanged (H-118) |
| Servers | `lab-node` (default) and `lab-java` (CS-08, CS-10) of `tools/upstream-lab/start.sh`; Docker 29.9.2 |
| Conductor | Node.js v26.8.2; proxy `127.0.0.1:18090`, control `127.0.0.1:18091`; the emulator reaches them as `10.0.2.2` |
| iOS runner | macOS 26.6.2 (25G83), macOS SDK 27.0, Apple clang 21.0.0; 14 upstream files built unchanged with `-fno-objc-arc -fobjc-weak` (H-119) |
| Harness runtime choices | Synthetic account, cookie and user agents; resource cache in app storage; the harness waits until the engine is available (section 6) |

Measurements: 116 runs of `run.mjs --all` in 607 s (about 5.2 s per run), then 6 runs of CS-10 Quick. 119 stored runs, 331 files, 1 731 383 bytes.

## 3. Stability

| Result | Strict (CONTRACT §6) | Outcome projection |
|---|---|---|
| Stable scenario modes | 7 of 40 | 40 of 40 |
| Unstable | 33 (window or thread position of SDK events; reasons in `run.json`) | 0 |

<Badge type="tip" text="FACT" /> The 33 strict differences are all of one kind: an SDK event changes places with a step boundary or with an event of another harness thread (for example `proxy.response_head` before or after `step.start bind_client`, or `onSessionFirstLoad` before or after `step.done bind_client`). No client action, code, message or cache file differs.

CS-10 Quick: in the first set of 3 runs, one run had the **setup** first load as Q2 (`loadUrl`, 1000 / 1000) instead of Q1 (data, 1000 / 304). The setup macro (`first_load`) calls `clientReady` before the body ends, so its own order is free. The scenario part was the same. 6 more runs: all equal. The stored evidence has the 6 runs; the variant run is not stored.

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
| CS-13a | Q, S | `loadUrl`; intercept 139 bytes, then `EOFException` | 1000/1000 | 1000/1000 at the result, again | 0; none |
| CS-13b | Q, S | `loadUrl`; intercept `null` | -1/-1 | none | 0; none |
| CS-14 | Q | data(a); intercept `null` | -1/-1 | none | 0; removed |
| CS-14 | S | `loadUrl`; intercept a (cache) | -1/-1 | none | 0; removed |
| CS-15 | Q, S | second `createSession` returns `null`; first session as CS-02 | 1000/1000 | 1000/1000 | 1; 4 files |
| CS-16 | Q, S | new account: first load (no `If-None-Match`); account change while held: the session keeps account 2 and gets 304 | 1000/304; 304/304 | as codes | 2; 8 files (2 ids) |
| CS-17 | Q, S | first load, then 304; request cookie read at `createSession`; `Set-Cookie` of the 304 goes to the runtime | 1000/304; 304/304 | as codes | 2; 4 files |
| CS-18 | Q, S | destroy before the body end: intercept `null`; destroy 0.5 s after the end | -1/-1; 1000/304 | 1000/304 | 0; none |
| CS-19 | Q | WebView: data(b*); the WebView asks for a `data:` URL, gets `null`; page finished | 200/304 | 200/304 | 1; 4 files (b) |
| CS-19 | S | WebView: `loadUrl`; the WebView intercept gets b*; page finished | 200/304 | 200/304 | 1; 4 files (b) |

Notes:

1. In Quick, the script asks for the main document also after a data load. A real WebView asks for a `data:` URL instead (CS-19), so it gets no session stream. The extra intercept adds a stream-close save in CS-01, CS-06, CS-07, CS-16, CS-17 (Quick).
2. CS-08 and CS-10 use `lab-java`. The upstream Node.js and PHP servers replace the application `cache-offline` with `true` (server trace S23). The Java server sends its own `Cache-Offline: true` and the application `cache-offline` as two fields. The Android stack merges them (names compare without case), and the SDK joins the values with `,`. The value `true,http` (or `true,store`) matches no rule of F6, F7 or F12.
3. CS-09 drops `cache-offline` in the proxy: the absent field takes the branch of `false` (F7).
4. CS-11 uses page `b`: with page `a` and the cached tag, the upstream middleware answers `304` before the application status.

## 5. Rows and probable defects

### 5.1 Row results

| Rows | Result |
|---|---|
| Q1, Q2, Q8, Q10, Q12, Q14, Q22; S1, S6, S7, S9, S11, S17 | <Badge type="tip" text="FACT" /> Timing rows confirmed in the scripted order of their scenario (outcome-stable, 3 runs) |
| Q4, Q7, Q15, Q17, Q18, Q20; S2, S3, S5, S8, S13, S14, S15 | <Badge type="tip" text="FACT" /> Code-reading rows confirmed (S8 with "another value": `true,http`, `true,store`) |
| Q3, Q9, Q11, Q13; S10, S12, S16 | <Badge type="warning" text="RESEARCH" /> Not reached: no upstream server sends an empty `200` body or `cache-offline: store` alone; no rebuild failure, no `refresh()` in the catalog |
| Q5, Q6, Q16, Q19, Q21; S4, S18 | Not in the catalog or not reachable (`http` alone); code reading stands |

### 5.2 Probable upstream defects

| Defect (code reading) | Trace result |
|---|---|
| Quick first load as data skips the cache write (Q1 note 1) | **Refuted for the order of CS-01**: the body was complete before `clientReady`, the first-load message waited in the slot, and the 1.5 s save ran (3 of 3 runs). <Badge type="warning" text="RESEARCH" /> The remaining window is `clientReady` between `QSS:628` and `QSS:638` on the session thread; a script cannot place a step there |
| Quick empty `200` body loads nothing (Q3 note 2) | <Badge type="warning" text="RESEARCH" /> Not reached. The upstream servers always send the page, and no lab page is empty. A `cut` at 0 bytes is a read error (Q20), not an empty body |
| Quick data update lost (Q8 note 4) | Not seen: cache first (CS-04) delivered the diff, network first (CS-05) loaded the rebuilt page. <Badge type="warning" text="RESEARCH" /> The window is the preload on the main thread between `QSS:697` and `QSS:723` |
| Standard 304 with the old cache (S7 note 3) | Not seen: in CS-05 the intercept got the rebuilt page. <Badge type="warning" text="RESEARCH" /> The window is an intercept between `STD:470` and `STD:472` |
| iOS local-server data update (D-39) | <Badge type="warning" text="RESEARCH" /> No iOS session run (H-119). The `diff` function confirms only that the merge gives an empty diff when the server map has no local key (FN-DIFF-NEW-04); the full-data fallback is in the cache code, which the runner does not call |

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
- **Upstream servers.** They do not send `cache-offline` values other than `true` (Node.js, PHP) or `true` plus a second field (Java). Rows that need `http`, `false` or `store` alone stay code reading unless the proxy can rewrite a field (proposal for S2).
- **Thread windows.** The windows of the defects in section 5.2 are inside the SDK threads; a script cannot place a step there.

## 9. Reproduce

1. Start the servers: `UPSTREAM=<clone> sh tools/upstream-lab/start.sh`.
2. Build, boot and install: `sh tools/upstream-lab/android/build.sh`, `sh tools/upstream-lab/android/emulator.sh start`, `sh tools/upstream-lab/android/install.sh`.
3. Run the scenarios: `node tools/upstream-lab/client/run.mjs --all --adb <lab adb> --out evidence/traces/client`.
4. Run the functions: `sh tools/upstream-lab/android/functions.sh tools/upstream-lab/client/functions.json evidence/traces/client/android/functions.jsonl`; `sh tools/upstream-lab/ios/build.sh`; `sh tools/upstream-lab/ios/run.sh tools/upstream-lab/client/functions.json evidence/traces/client/ios/functions.jsonl`; `sh tools/upstream-lab/ios/probe/run.sh tools/upstream-lab/client/functions.json evidence/traces/client/ios/boolvalue.jsonl`.
5. Check: `node tools/upstream-lab/client/check.mjs evidence/traces/client --require-functions`.
6. Stop: `sh tools/upstream-lab/android/emulator.sh stop`; `docker rm -f lab-node lab-php lab-java`.

## Related

- [Legacy client behavior](/engineering/protocol/legacy-client)
- [Divergences](/engineering/protocol/legacy-divergences)
- [Server trace report](/engineering/protocol/legacy-traces)
