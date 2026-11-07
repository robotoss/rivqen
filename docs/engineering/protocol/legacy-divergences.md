# Legacy protocol: divergences and parity

::: warning Legacy mode — temporary
This page describes the **legacy mode** (compatibility with the VasSonic legacy protocol). It ships as an optional module, is **deprecated from 1.0** and is **removed in 1.5**. New integrations use the [Rivqen protocol](/engineering/protocol/markup). See [Modes and legacy deprecation](/engineering/protocol/versioning).
:::

The upstream project has five implementations of legacy protocol (Java, Node.js and PHP servers; Android and iOS clients). They do not behave identically. This page lists every known divergence, the Rivqen decision, and the parity matrix that defines "compatible".

**Status:** <Badge type="tip" text="FACT" /> divergences from code at `59936bef`. The decisions are <Badge type="info" text="DESIGN" />. ADR-005 is accepted (human, 2026-10-09): the "Rivqen canonical" column and the profiles `legacy-java`, `legacy-node`, `legacy-php`.

[[toc]]

## 1. Server divergences

| # | Topic | Java | Node.js | PHP | Rivqen canonical |
|---|---|---|---|---|---|
| D-01 | Legacy request detection | `accept-diff == "true"` gates only `304` and cache headers; data response depends on `template-tag` alone | `accept-diff` present (any value) and gzip/deflate accepted | `accept-diff === "true"`; header name case-sensitive | `accept-diff == "true"` (case-insensitive name) gates all legacy behavior |
| D-02 | Headers on non-legacy responses | Adds `Etag`, `template-tag`, `template-change` to all text responses | Only on legacy path | Only on legacy path | Only on legacy path |
| D-03 | `304` comparison | Case-insensitive | Case-sensitive | Case-sensitive | Case-insensitive, after removing quotes and `W/` |
| D-04 | Title replace | All occurrences, case-sensitive replace | First, case-insensitive | All, case-insensitive | First, case-insensitive |
| D-05 | Empty block name | `{}` | `{auto0}`… | `{}` | Block not extracted |
| D-06 | Page without markers | Data response with title only | Always full HTML, `template-change: true` | Data response with title only | Full HTML on first load; data response with title when the template matches |
| D-07 | `etag` header name | `Etag` | `Etag` (published `sonic_differ` 1.0.7); `sonic-etag-key` only in the unused repo file `common/diff.js` | `Etag` | `etag`; honor `sonic-etag-key` when reading |
| D-08 | JSON key order and escaping | `HashMap` order; Gson HTML-escapes `<`, `>` as `<`… | Insertion order; no HTML escaping | Insertion order; escapes `/` as `\/` and non-ASCII as `\uXXXX` | Semantic JSON equality in tests; Rivqen emits UTF-8, no HTML escaping |
| D-09 | `Cache-Control` on legacy responses | `no-cache` | none | `no-cache` | `no-cache` + `private` |
| D-10 | Compression | Not handled | Gzip/deflate in middleware; legacy processing **skipped** if the client accepts only `identity` | Left to the web server | Independent of `Accept-Encoding` |
| D-11 | Content type of data responses | Unchanged (`text/html`) | Unchanged (`text/html`) | Unchanged | Unchanged (`text/html`): iOS hands the rebuilt page to the web view with these fields (DL-072) |
| D-12 | `Cache-Offline` on `304` | Two fields: `true` **and** `store` | `store` | `store` (replaces earlier `true`) | One field: `store` |
| D-13 | Several `<title>` elements | Value from first match (case-insensitive), replacement case-sensitive → wrong element replaced | First title only | All titles replaced, first value stored | Not applicable in RQP; legacy module: Node.js behavior |
| D-14 | `cache-offline` set by the application | A second field after its own `true` | Its own `true` only (the application value is lost) | Its own `true` only | The application value, one field (C-10, DL-070) |
| D-15 | `Content-Type` on `304` | None | None | Kept | Kept from the application (C-13) |

All server rows are confirmed by captured traces ([server trace report](/engineering/protocol/legacy-traces)). Evidence: Java `upstream:sonic-java/src/main/java/com/github/tencent/SonicFilter.java:60-149`; Node.js `upstream:sonic-nodejs/common/diff.js:12-122`, `upstream:sonic-nodejs/middleware/compress.js:18-83`; PHP `upstream:sonic-php/sdk/sonic.php:50-145`.

### 1.1 Fixtures of each divergence

<Badge type="tip" text="FACT" /> Source: the `divergences` member of the profile `case.json` files in `fixtures/legacy/`. A profile fixture is `FX-LEG-<NAME>--JAVA`, `--NODE` or `--PHP`. Below, `J`, `N` and `P` name the profiles that have a fixture. The fixture IDs omit the prefix `FX-LEG-`.

| # | Fixtures (profiles) |
|---|---|
| D-01 | `DETECT-ACCEPT-DIFF-FALSE` (J, N), `DETECT-ACCEPT-DIFF-YES` (J, N), `DETECT-NAME-CASE` (P) |
| D-02 | `NON-SONIC-CLIENT` (J) |
| D-03 | `304-QUOTED` (J, N, P), `304-UPPERCASE` (N, P) |
| D-04 | No fixture of its own (see the notes below) |
| D-05 | `EMPTY-NAME-DATA` (J, N, P), `EMPTY-NAME-FIRST` (J, N, P) |
| D-06 | `NO-MARKERS-DATA` (N), `NO-MARKERS-FIRST` (N), `MARKER-SPACES-DATA` (N), `MARKER-SPACES-FIRST` (N) |
| D-07 | Not observable on the wire (see the notes below) |
| D-08 | 21 fixtures, J and P; see the list below |
| D-09 | 30 fixtures, J, N and P; see the list below |
| D-10 | `COMPRESSED-IDENTITY` (N) |
| D-11 | Not observable on the wire (see the notes below) |
| D-12 | `304` (J), `304-QUOTED` (J), `304-UPPERCASE` (J) |
| D-13 | `TITLE-SEVERAL-DATA` (J, P), `TITLE-SEVERAL-FIRST` (J, P) |
| D-14 | `OFFLINE-FALSE` (J, N, P), `OFFLINE-HTTP` (J, N, P) |
| D-15 | `304` (J, N), `304-UPPERCASE` (J) |

D-08 (data bodies). `304-QUOTED` (J, P), `304-UPPERCASE` (P), `COMPRESSED-IDENTITY` (J, P), `DATA-CHANGE` (J, P), `DETECT-ACCEPT-DIFF-FALSE` (J), `DETECT-ACCEPT-DIFF-YES` (J), `DETECT-NAME-CASE` (J), `OFFLINE-FALSE` (J, P), `OFFLINE-HTTP` (J, P), `VARY-APP` (J, P), `CDN-FIELDS` (J, P), `DUPLICATE-BLOCKS-DATA`, `EMPTY-NAME-DATA`, `MARKER-MISMATCH-DATA`, `MARKER-NESTED-DATA`, `MARKER-SPACES-DATA`, `MARKER-UPPERCASE-DATA`, `NO-MARKERS-DATA`, `TITLE-ATTRIBUTE-DATA`, `TITLE-SEVERAL-DATA`, `UNICODE-DATA` (all J, P).

D-09 (`Cache-Control`). `304`, `304-QUOTED`, `304-UPPERCASE`, `DATA-CHANGE`, `FIRST-LOAD`, `TEMPLATE-CHANGE`, `OFFLINE-FALSE`, `OFFLINE-HTTP`, `VARY-APP`, `CDN-FIELDS`, `EMPTY-NAME-<DATA|FIRST>`, `MARKER-<MISMATCH|NESTED|SPACES|UPPERCASE>-<DATA|FIRST>`, `NO-MARKERS-<DATA|FIRST>`, `TITLE-ATTRIBUTE-<DATA|FIRST>`, `TITLE-SEVERAL-<DATA|FIRST>`, `UNICODE-<DATA|FIRST>` (all J, N, P); `COMPRESSED-IDENTITY` (J, P); `DETECT-NAME-CASE` (J, N).

Notes:

1. <Badge type="tip" text="FACT" /> D-04 has no fixture of its own. It shows in the same observation as D-13: the title fixtures (`TITLE-ATTRIBUTE-*`, `TITLE-SEVERAL-*`).
2. <Badge type="tip" text="FACT" /> D-07 and D-11 are not observable on the wire. They show as `profiles_same` or as equal `fields` in the `expected.json` files.
3. <Badge type="tip" text="FACT" /> D-08: the runner compares data bodies as JSON values, so the key order and the escaping do not fail a case.
4. <Badge type="tip" text="FACT" /> The four D-13 profile fixtures of Java and PHP (`TITLE-SEVERAL-DATA` and `TITLE-SEVERAL-FIRST`) carry `not_reproduced: "U-11"`. They expect the canonical title rule and keep the other behaviors of the profile (DL-069). The Node.js fixtures of these two IDs list D-09 only.
5. <Badge type="tip" text="FACT" /> The U-13 fixtures `REDIRECT`, `PASS-NOT-FOUND` and `PASS-POST` (J, N, P) and `PASS-HEAD` (J, P) carry `not_reproduced: "U-13"`. They expect the C-01 pass-through (DL-082). `PASS-HEAD` has no Node.js fixture: Node.js passes a `HEAD` through, so `legacy-node` is in `profiles_same` of the canonical fixture.
6. <Badge type="tip" text="FACT" /> D-14 and D-15 come from the live run of 2026-10-31 (traces S22, S23 and S24; [server trace report](/engineering/protocol/legacy-traces#_9-live-run-of-2026-10-31)). D-15 also shows in the older traces S03 and S04.
7. <Badge type="tip" text="FACT" /> The client divergences (section 2) have no fixture. The client part of the fixtures is added before P3 (H-110).

## 2. Client divergences

**Status:** <Badge type="tip" text="FACT" /> code reading of the Android and iOS SDKs at `59936bef`, checked again row by row in WP-01 S1 (T-01). <Badge type="warning" text="RESEARCH" /> marks a point that depends on a platform library (org.json, Foundation, `Uri`, `NSURL`) or a run that is not done. <Badge type="tip" text="FACT" /> The function runs and Android traces of WP-01 S1 T-05 settle the points marked "Trace" ([client trace report](/engineering/protocol/legacy-client-traces) §6, §7): Android on the API 28 image, iOS on the Foundation of macOS 26.6 (not the iOS of 2017–2019). The "Rivqen canonical" values of D-20…D-27 are those of ADR-005 (unchanged). The values of D-28 and later are open until T-06 (H-120).

The tables are short; section 2.2 has the exact rules and the evidence of each row. Session behavior of Android alone (Quick and Standard) is in [legacy client](/engineering/protocol/legacy-client) §2 and §3.

### 2.1 Rows

Session identity, markers and the page message (D-20…D-27):

| # | Topic | Android | iOS | Rivqen canonical |
|---|---|---|---|---|
| D-20 | Session id base and account | `authority + path` (port, no scheme); `<account>_` + MD5, account in clear text | `scheme://host + path` (no port); MD5 of `<account>_<base>` | Not used as cache key; reproduced only per platform profile |
| D-21 | Kept parameters | Sorted names; first value, decoded; no separator | Dictionary order; last value, raw; `name=value&` after `/`; only if the URL contains `sonic_` | Sorted |
| D-22 | Marker regex flags | Case-sensitive | Case-insensitive | Case-insensitive |
| D-23 | Block name source | Opening marker | Closing marker | Opening marker |
| D-24 | Client-side title rule | First literal `<title>` element, if a block exists | None | Same as server canonical rule |
| D-25 | Diff of new keys | Included, unless the value is empty | Ignored | Included |
| D-26 | Page message types | Numbers for codes and age; `isReload` the string `"false"` (trace) | Strings only; `isReload` always `"false"` | Android form: integer codes and age, `isReload` the string `"true"`/`"false"`; shim can emit the iOS form (DL-106) |
| D-27 | Web view integration | No WebView code in the SDK: the app's client loads data or a URL and passes intercepted requests in | `UIWebView` + global `NSURLProtocol` | Public APIs only (see platform pages) |

Headers, cache and update rules (D-28 and later):

| # | Topic | Android | iOS | Rivqen canonical |
|---|---|---|---|---|
| D-28 | Legacy response test | No single test in the flow; any one of 3 fields only turns off the local-server mode | All 4 fields required | All four fields, not empty, on a `200`; else normal load, no cache (DL-105) |
| D-29 | `template-change` value | `false` or `0`, exact: data update | `boolValue` false: data update | Only `false` or `0` is a data update (DL-105) |
| D-30 | `cache-offline` value case | `http`, `false`: any case; `true`, `store`: exact | Exact | ASCII case-insensitive; another value counts as `false` (DL-105) |
| D-31 | Weak etag | `w/`, any case: all lowercase, every `w/` and `"` removed | `W/` exact: prefix and outer quotes removed, case kept | One leading `W/` and outer quotes removed; exact compare (DL-105) |
| D-32 | `sonic-etag-key` | Named field found in any case | Named field found only if lowercase | Case-insensitive names (DL-105) |
| D-33 | Repeated response fields | Joined with `,` | Joined by the platform | Different values on two lines: not a legacy response (DL-105) |
| D-34 | Empty diff | Nothing sent | Full server data sent | No message; final `304` (DL-106) |
| D-35 | Stored data after an update | Full server data | Old keys only; new keys dropped | Full server data; only changed blocks to the page (DL-106) |
| D-36 | Diff age limit | 30 s | None | 30 s (DL-106) |
| D-37 | Callback spacing on reload | At least 2 s | None | None (DL-106) |
| D-39 | Local-server decision | Non-legacy `200` only | Also `304`; data update probably fails | Android rule, opt-in (DL-109) |
| D-41 | Cache-control rules (flag on) | Substring match | Exact match, first load only | RFC 9111; `no-store` wins (DL-108) |
| D-42 | Cache storage | 4 files per id, SQLite | Directory per id, SQLite, plist data | Not reproduced: Rivqen cache (DL-108) |
| D-44 | Charset | Default: platform charset | Default: UTF-8; faulty cut | RFC 9110 parameter; default UTF-8 (DL-105) |
| D-45 | `200` with the request etag | Handled as `304` | Handled as an update | Handled as `304` (DL-105) |
| D-46 | `cache-offline: false`, cached page | Cache removed, no update | Update delivered, then cache removed | Update delivered, then cache removed (DL-107) |
| D-47 | `cache-offline: http`, cached page | No update; cache kept | Update delivered, then cache removed | Cache removed, 6 h off, normal load (DL-107) |
| D-48 | `store` and `code` | Final `304` only on update paths | `code` `"304"` for every result | Android rule (DL-106) |
| D-49 | `html-sha1` of a rebuilt page | Checked before use | Not checked; next read drops it | Checked; mismatch: normal load, cache removed (DL-108) |
| D-50 | First load fails | WebView loads the URL itself | Failure passed to the web view | Normal load (DL-107) |
| D-51 | Headers of a `304` | Stored headers kept | Stored headers replaced | Stored headers updated (RFC 9111 §4.3.4) (DL-108) |
| D-52 | Headers of the cached page | Stored all; some removed for the WebView | Some removed before storage | `Set-Cookie`, hop-by-hop, cache fields never stored (DL-108) |

Checked and **not** divergences (numbers kept so that the kickoff list stays traceable):

| # | Topic | Result |
|---|---|---|
| D-38 | Local-server mode switch | Off by default on both: Android `SUPPORT_LOCAL_SERVER` false (`CFG:78`); iOS `enableLocalServer` has no initializer, and a session made without a configuration reads `NO` (`I:Session/SonicSessionConfiguration.m:26-35`, `I:Engine/SonicEngine.m:180-183`) |
| D-40 | Time off after `cache-offline: http` | 6 h on both (`SC:30`; `I:Engine/SonicConfiguration.m:31`). The handling differs: D-47 |
| D-43 | `sonic-link` preload | Present on both (`SS:704-709`; `I:Session/SonicSession.m:127`, `:354`, `:866-880`) |

### 2.2 Rules and evidence

Paths: `A:` = `upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/`, `I:` = `upstream:sonic-iOS/Sonic/`. Android short names as in [legacy client](/engineering/protocol/legacy-client): `SS` = `A:SonicSession.java`, `QSS` = `A:QuickSonicSession.java`, `STD` = `A:StandardSonicSession.java`, `SV` = `A:SonicServer.java`, `SU` = `A:SonicUtils.java`, `CFG` = `A:SonicSessionConfig.java`, `SC` = `A:SonicConfig.java`. **Test** names the observation: a function case of the client lab (`fn` of `tools/upstream-lab/client/CONTRACT.md` §7), a session trace (scenario), or code reading only.

**D-20 Session id base and account.** <Badge type="tip" text="FACT" />

1. Android: base = `authority` + `path` of the parsed URL; the port stays, the scheme is not used. If parsing fails, the base is the full URL. Hash: MD5 of the default-charset bytes. With `IS_ACCOUNT_RELATED` (default on): `<account>_<md5>`; a `null` account gives `null_`, an empty one gives `_` (`A:SonicRuntime.java:75-113`, `SU:697-707`, `CFG:56`).
2. iOS: base = `scheme://host` + `path`; no port. If `NSURL` cannot parse the URL, the raw string is the base. With a non-empty account: MD5 of the UTF-8 of `<account>_<base>`; else MD5 of the base (`I:Util/SonicUtil.m:31-106`). An empty account cannot be set, so a set account cannot be cleared (`I:Engine/SonicEngine.m:85-98`).
3. <Badge type="tip" text="FACT" /> Trace: both keep the host case (FN-ID-01, FN-ID-03); Android hashes UTF-8 on API 28 (FN-ID-13); Android keeps the port (FN-ID-05) and drops the scheme (FN-ID-04), iOS the reverse; Android `null` account → `null_`, empty → `_` (FN-ID-08, FN-ID-09). The `NSURL` of old iOS is NOT FOUND.
4. Test: `session_id` FN-ID-01…14.

**D-21 Kept parameters.** <Badge type="tip" text="FACT" />

1. Android: names = the unique decoded query names, sorted (`TreeSet`, UTF-16 order). If `sonic_remain_params` has a value, its `;` items are the kept list and the name itself is dropped; if it is empty, it stays and counts as a `sonic_` name. For each name that starts with `sonic_` or is in the list: append name and value, no separator. A name in both is appended once (`A:SonicRuntime.java:81-98`, `:124-157`).
2. iOS: only if the raw URL string contains `sonic_` (host and path too); else the base has no parameters, and `sonic_remain_params` is not read. The query is split on `&` and the first `=`; values stay percent-encoded; the last duplicate wins. For each key in dictionary order: a `sonic_` key appends `key=value&`; a key in the kept list appends it **again**. A non-empty result goes after a `/` (`I:Util/SonicUtil.m:68-139`).
3. <Badge type="tip" text="FACT" /> Trace: Android takes the first value, decoded (`%41%20b` → `A b`, `+` → space; FN-ID-ORD-03, -04); iOS takes the last value, raw (FN-ID-ORD-03, -04). <Badge type="warning" text="RESEARCH" /> The iOS order followed the query on macOS 26.6 (FN-ID-ORD-01, -02), but `NSDictionary` has no order contract: a fixture with two kept names on iOS can only assert the set.
4. Test: `session_id` FN-ID-ORD-01…14.

**D-22 Marker regex flags.** <Badge type="tip" text="FACT" /> Same pattern; Android compiles it with `MULTILINE` only, which has no effect here, so it is case-sensitive (`SU:74`, `SU:484`). iOS uses the case-insensitive flag (`I:Util/SonicUtil.m:185`). <Badge type="tip" text="FACT" /> Trace: Android finds no block in upper-case markers (FN-SPLIT-02, -03); iOS matches them (D-23). Test: `split` FN-SPLIT-01…12.

**D-23 Block name source.** <Badge type="tip" text="FACT" />

1. Android: the name is the text after the first `<!--sonicdiff-` and before the first `-->` of the match, which is the opening marker. The two names are not compared. An unnamed opening marker gives the key `{null}` (`SU:486-497`).
2. iOS: the match is split on the case-sensitive `<!--sonicdiff-`; the last piece is cut at `-end-->`, which is the closing marker. A later block with the same key replaces the earlier one (`I:Util/SonicUtil.m:190-197`).
3. <Badge type="tip" text="FACT" /> Trace (iOS): an unnamed marker gives the key `{end-->}` (FN-SPLIT-06, FN-SPLIT-NAME-02); an upper-case marker gives the whole match cut at `-end-->` as the key (FN-SPLIT-02, -03). Android: `{null}` (FN-SPLIT-UNNAMED-01).
4. Test: `split` FN-SPLIT-NAME-01…08, FN-SPLIT-UNNAMED-01…06.

**D-24 Client-side title rule.** <Badge type="tip" text="FACT" /> Android: after the blocks, the first literal `<title>` up to the next `</title>` in the template becomes `{title}`; the value is the whole element. The step runs only if at least one block was found (`SU:511-521`). iOS: no title step in `sonic-iOS/Sonic`. <Badge type="tip" text="FACT" /> Trace: `<TITLE>` and `<title lang=…>` are not taken (FN-SPLIT-TITLE-03, -04); with no block Android returns an empty template (FN-SPLIT-11). Test: `split` FN-SPLIT-TITLE-01…09.

**D-25 Diff of new keys.** <Badge type="tip" text="FACT" /> Android: every server key whose string value differs from the local one; a missing local value reads as `""`; with no local data file the diff is the full server data (`SU:251-299`). iOS: only keys that exist locally (`I:Util/SonicUtil.m:210-228`); an empty diff becomes the full server data (D-34). <Badge type="tip" text="FACT" /> Trace: FN-DIFF-03, -04, -05, -09, FN-DIFF-NEW-01…04; a number value makes the iOS merge throw (FN-DIFF-07, -08). In an Android session, the first update after a first load holds every block, because the first-load data file has another form (CS-04; [trace report](/engineering/protocol/legacy-client-traces) §6). Test: `diff` FN-DIFF-01…12.

**D-26 Page message types.** <Badge type="tip" text="FACT" />

1. Android: `code`, `srcCode` integers; `result` a string with the diff JSON; `local_refresh_time` a number, the age of the diff in ms, only for final `200` and only if > 0. `extra`: the etag key (default `eTag`, or the `sonic-etag-key` value), `template-tag`, `cache-offline`, all from the current response, and `isReload` (`SS:1086-1185`).
2. iOS: all values are strings. `local_refresh_time` is the cache refresh time (ms since 1970, whole seconds), always present. `extra.eTag` and `extra.template-tag` come from the cached configuration of the session, not from the new response. `extra.cache-offline` is `""` when absent. `isReload` is always `"false"`, because its flag is never set (`I:Session/SonicSession.m:48`, `:652-696`; `I:Cache/SonicCache.m:480-500`).
3. <Badge type="tip" text="FACT" /> Trace (Android sessions): `isReload` is the string `"false"`; an absent header has no `extra` key; `/` is written as `\/`; keys keep insertion order. iOS pretty print, key order and `/` escaping: NOT FOUND (no iOS message run, H-119).
4. Test: session traces (`message` is `unsupported` on both platforms).

**D-27 Web view integration.** <Badge type="tip" text="FACT" /> Android: the SDK has no WebView code; the app implements `loadUrl`, `loadDataWithBaseUrlAndHeader` and the response factory, and passes each intercepted request in (`A:SonicSessionClient.java:94-132`, `SS:1213-1228`, `A:SonicRuntime.java:274`). iOS: see [legacy client](/engineering/protocol/legacy-client#_5-3-ios-integration-q-03) §5.3. Test: code reading only.

**D-28 Legacy response test.** <Badge type="tip" text="FACT" />

1. Android has no single test in the default flow: `cache-offline` (F6, F7), then etag and `template-change` (F8) are checked step by step ([legacy client](/engineering/protocol/legacy-client) §2.2). The three-field test (any one of `cache-offline`, `template-change`, `template-tag`) only decides whether the local-server mode runs (`SV:129`, `SV:189-206`).
2. iOS requires all four: `cache-offline`, the etag field, `template-change`, `template-tag`, each not empty (`I:Network/SonicServer.m:345-364`). Without them a first load is not cached and an update removes the cache (`I:Session/SonicSession.m:466-532`, `:713-719`).
3. <Badge type="tip" text="FACT" /> Trace: iOS needs all four, names in any case (FN-LEGACY-01…10, FN-LEGACY-CASE-01…05); Android `unsupported` (private method); CS-14 (etag dropped) removes the cache on Android.
4. Test: `is_legacy_response`; session CS-14.

**D-29 `template-change` value.** <Badge type="tip" text="FACT" /> Android: exactly `false` or `0` is a data update; any other value is a template change (`SS:771-776`). iOS: `NSString boolValue`; true is a template change (`I:Session/SonicSession.m:907-914`). <Badge type="tip" text="FACT" /> Trace (Foundation probe, macOS 26.6, `ios/boolvalue.jsonl`): `boolValue` is true for `true`, `TRUE`, `True`, `yes`, `y`, `t`, `1`, `2`, `-1` and ` true`; false for `false`, `False`, `no`, `0`, `00`, `0.0`, `null`, `abc`, empty and space. So `False` and `no` are data updates on iOS and template changes on Android. The `boolValue` of old iOS is NOT FOUND. Test: FN-TCHANGE-01…25 (`template_change` is `unsupported` on both: private code).

**D-30 `cache-offline` value case.** <Badge type="tip" text="FACT" /> Android: `http` and `false` compare in any case (`SS:730`, `SS:754`); `true` and `store` compare exactly (`SU:575-603`). So `TRUE` passes the flow; on a data update Quick then delivers the diff and removes the cache, and Standard delivers nothing and writes the cache ([legacy client](/engineering/protocol/legacy-client) M-12). iOS: all four compare exactly (`I:Session/SonicSession.m:474`, `:494`, `:664`, `:730-746`). Test: session trace (no pure function). <Badge type="tip" text="FACT" /> Trace (Android, S2 T-05; the proxy sets the field with `set_field`, a data update with the cached page shown first): `HTTP` takes the `http` branch (CS-24a, the same result as `http` in CS-25); `Store` (CS-24b) and `TRUE` (CS-24c) are other values: Quick sends the diff (200 / 200) and removes the cache, Standard sends no message (-1 / -1) and writes the page, data and header files with no template file. The iOS rule stays code reading (H-119).

**D-31 Weak etag.** <Badge type="tip" text="FACT" /> Android: if the value starts with `w/` in any case, the value is lowercased, every `w/` and every `"` is removed (`SV:114-120`). iOS: if it starts with `W/` (exact) and is longer than 3, the prefix goes; outer quotes go only if both are present; the case stays; the result is stored under `etag` even when `sonic-etag-key` names another field (`I:Network/SonicServer.m:395-404`). A strong quoted etag keeps its quotes on both. <Badge type="tip" text="FACT" /> Trace: FN-ETAG-15 (`W/"a"b"` → Android `ab`, iOS `a"b`), FN-ETAG-11 and FN-ETAG-WEAK-03 (`W/`, `W/x` unchanged on iOS), FN-ETAG-WEAK-09 (one quote stays on iOS). Test: `etag` FN-ETAG-01…19, FN-ETAG-WEAK-01…12.

**D-32 `sonic-etag-key`.** <Badge type="tip" text="FACT" /> Both read the etag from the field named by `sonic-etag-key`, else from `eTag` (Android) or `etag` (iOS). Android lowercases the name before the lookup in its lowercased map (`A:SonicSessionConnection.java:515-522`, `SV:288-302`). iOS lowercases the map but not the name, so a name with an uppercase letter finds no etag (`I:Network/SonicServer.m:285-295`, `:384-393`). Test: session trace (not in the S1 catalog; both `etag` runners read the default field, FN-ETAG-KEY-01…06).

**D-33 Repeated response fields.** Android joins the values of a repeated field with `,` (`SV:288-302`). iOS reads the single value that `NSHTTPURLResponse` gives. <Badge type="tip" text="FACT" /> Trace (Android, API 28): two fields that differ only in case are one list, joined with `,` (`extra.cache-offline` `true,http`, CS-08; `true,store`, CS-10). <Badge type="warning" text="RESEARCH" /> The iOS separator: NOT FOUND without a device run. Test: session traces CS-08, CS-10.

**D-34 Empty diff.** <Badge type="tip" text="FACT" /> Android: an empty diff gets no `local_refresh_time`, so no message reaches the page; the codes still say 200 / 200 (`SU:261-263`, `SS:1111-1118`). iOS: the full server data becomes the diff (`I:Cache/SonicCache.m:366-370`). <Badge type="tip" text="FACT" /> Trace: the merge functions of both give `{}` (FN-DIFF-EMPTY-01…04, FN-DIFF-EMPTY-IOS-01…04); the iOS fallback is in the cache code, which the runner does not call (code reading). Test: `diff`.

**D-35 Stored data after an update.** <Badge type="tip" text="FACT" /> Android stores the full server data as the local data (`QSS:755`, `STD:507`); the rebuilt page replaces only placeholders that exist in the template (`SU:339-358`). iOS changes only existing keys, never adds a key, and rebuilds the page from that data (`I:Util/SonicUtil.m:210-228`, `I:Cache/SonicCache.m:363-386`). <Badge type="tip" text="FACT" /> Trace (Android): after an update the data file is the server block map (CS-04, CS-05); after a first load it is `{"data": …}` with `html-sha1` and `template-tag` (`SV:436-446`). Test: session trace (second update not in the S1 catalog).

**D-36 Diff age limit.** <Badge type="tip" text="FACT" /> Android drops a diff older than 30 s at send time (`SS:1119-1124`). iOS has no age check (`I:Session/SonicSession.m:636-696`). Test: session trace with a late `getDiffData`.

**D-37 Callback spacing on reload.** <Badge type="tip" text="FACT" /> Android delays a callback during `refresh()` so that callbacks are at least 2 s apart (`SS:1163-1183`). iOS has no delay; its reload result path never runs, because the flag is never set (`I:Session/SonicSession.m:48`, `:757-786`). Test: session trace.

**D-39 Local-server decision.** <Badge type="tip" text="FACT" />

1. Android: only a `200` that fails the three-field test, with `SUPPORT_LOCAL_SERVER` on; a `304` never enters. Missing etag: SHA-1 of the body; same etag as the request: `304`. Missing template tag: SHA-1 of the template. The data JSON has `data`, `html-sha1` and `template-tag` (`SV:106-187`, `SV:410-452`).
2. iOS: also a `304` enters, and it changes nothing. The data JSON is the block map plus `html-sha1`, with no `data` member (`I:Network/SonicServer.m:411-416`, `:451-547`).
3. <Badge type="warning" text="RESEARCH" /> iOS reads `data` from that JSON for the update; it finds nothing, the diff is empty, and the full-data fallback inserts a nil value (`I:Cache/SonicCache.m:363-370`). **Probable upstream defect: a local-server data update fails on iOS.**
4. Test: session trace (not in the S1 catalog; iOS sessions are out of scope, H-119). The iOS `diff` function gives `{}` for a server map with no local key (FN-DIFF-NEW-04), which supports item 3 but does not run the cache code.

**D-41 Cache-control rules.** <Badge type="tip" text="FACT" /> Off by default on both (`CFG:72`; iOS: no initializer, see D-38). With the flag on: Android skips the request while the stored expiry is in the future and writes no cache if the first `Cache-Control` value contains `no-cache`, `no-store` or `must-revalidate` (`SS:663-674`, `SU:154-199`, `SU:575-593`). iOS skips the session start while the cache is fresh and skips the first-load write only if the whole value is exactly one of the three (`I:Session/SonicSession.m:181-190`, `:486-491`). The iOS expiry rules (`I:Cache/SonicCache.m:505-555`) are NOT FOUND in detail. Test: code reading only.

**D-42 Cache storage.** <Badge type="tip" text="FACT" /> Android: `<id>.html`, `.tpl`, `.data` (JSON), `.header` in one directory, metadata in SQLite (`A:SonicFileUtils.java:45-60`, `:104-133`; `A:SonicDataHelper.java:89-100`). iOS: one directory per id with `html`, `temp`, `data` (property list) and `rsp` files, metadata in `sonic.db`, a file with the disabled ids, under the app's Documents directory (`I:Cache/SonicCache.m:645`, `:702-716`, `:252`; `I:SonicConstants.h:238-240`). Test: code reading. <Badge type="tip" text="FACT" /> Trace (Android): four files per id; the account is in clear text in the names (CS-16 has `rq-test-account_…` and `rq-test-account-2_…`).

**D-44 Charset.** <Badge type="tip" text="FACT" /> Android: in the lowercased `Content-Type`, the text after `charset` and one more character, spaces removed, cut at `;`; default is the platform default charset (`SU:48`, `SU:450-466`). iOS: the text after `charset=`; if it still contains `;` or a space, only the **last character** is cut, which leaves an invalid name and so UTF-8; default UTF-8 (`I:Network/SonicServer.m:251-267`). iOS decodes the cached page as UTF-8 (`I:Session/SonicSession.m:269-276`). <Badge type="tip" text="FACT" /> Trace: Android default `UTF-8` on API 28 (FN-CHARSET-07, -17); Android keeps quotes and unknown names (FN-CHARSET-09, -16); iOS gives UTF-8 for them and for a name followed by a parameter (FN-CHARSET-PARAM-02, -05). Test: `charset` FN-CHARSET-01…17, FN-CHARSET-PARAM-01…08.

**D-45 `200` with the request etag.** <Badge type="tip" text="FACT" /> Android handles a `200` whose etag equals the request etag as a `304` (`SV:122-127`). iOS does so only in the local-server mode (`I:Network/SonicServer.m:483-494`); otherwise it processes the update. Test: session trace.

**D-46 `cache-offline: false` with a cached page.** <Badge type="tip" text="FACT" /> Android removes the cache and delivers nothing; the page keeps the old content (`SS:753-758`). iOS processes the update (diff to the page, or a reload request on a template change), writes it, and then removes the cache (`I:Session/SonicSession.m:713-748`, `:813-821`; `I:Cache/SonicCache.m:208-222`). Test: session trace CS-09. <Badge type="tip" text="FACT" /> Trace (Android, field absent; the upstream servers do not send `false`): cache removed, no update, the cached page stays (CS-09).

**D-47 `cache-offline: http` with a cached page.** <Badge type="tip" text="FACT" /> Android delivers no update and keeps the cache; Quick loads the URL normally, Standard keeps serving the cache ([legacy client](/engineering/protocol/legacy-client) Q16, S4). iOS processes the update as usual, then removes the cache and starts the 6 h off time (`I:Session/SonicSession.m:713-748`, `I:Cache/SonicCache.m:262-276`). Both refuse new sessions in that time (`A:SonicEngine.java:198-214`; `I:Engine/SonicEngine.m:193-196`). Test: session trace. <Badge type="tip" text="FACT" /> Trace (Android, S2 T-05, `set_field cache-offline: http` on a data update, CS-25; `HTTP`, CS-24a): no update and no message; Quick loads the URL once (`loadUrl`), Standard keeps the cached page as the stream; codes -1 / -1; the cache files are not changed; the SDK reports `onSessionUnAvailable` (Standard twice). The 6 h off time is not observed: no scenario makes a second session after it. CS-08 records the Java form `true,http` (D-33).

**D-48 `store` and `code`.** <Badge type="tip" text="FACT" /> Android changes the final code to `304` only on update paths: Quick data update and template change after the page shows (`QSS:319-322`, `QSS:363-366`), Standard template change (`STD:271-273`); Standard data update sends no message (`STD:442`). A first load with `store` keeps 1000. iOS sets `code` to `"304"` for every result while `cache-offline` is `store`, also a first load (`I:Session/SonicSession.m:662-668`). Test: session trace. <Badge type="tip" text="FACT" /> Trace (Android, S2 T-05, `set_field cache-offline: store`): Quick data update 200 / 304 with no diff (CS-22); template change 2000 / 304 in both modes (CS-23); Standard data update sends no message, -1 / -1 (CS-22). <Badge type="warning" text="RESEARCH" /> A first load with `store` (final code 1000) is not in the catalog. CS-10 records the Java form `true,store`. `message` is `unsupported` on both.

**D-49 `html-sha1` of a rebuilt page.** <Badge type="tip" text="FACT" /> Android compares the rebuilt page with the `html-sha1` of the data response; on a mismatch there is no rebuilt page: Quick loads the URL (Q11) and the cache is removed (`SU:301-330`, `QSS:736-740`). iOS stores the value without a check and uses the page (`I:Cache/SonicCache.m:372-398`); the next cache read compares it, fails, and removes the files (`I:Cache/SonicCache.m:614-628`). Test: session trace CS-28 (Android, S2 T-08, `set_body_member html-sha1`). <Badge type="tip" text="FACT" /> Trace: Quick loads the URL, 200 / 1000, error -1008, the cache is removed (Q11); Standard gives the old cached page to the intercept, sends the diff (200 / 200) and leaves the data and header files with no page and no template file. Both leave the rebuilt page as `<id>.html.tmp`. iOS: no session run (H-119).

**D-50 First load fails.** <Badge type="tip" text="FACT" /> Android: on an error status, a connection error or a read error with no cache, the intercept gets `null` and the WebView requests the URL again itself (Q4, Q20, S2, S3). iOS: the session passes the status, the body or the error of its own request to the web view; there is no second request (`I:Session/SonicSession.m:436-451`, `:578-608`; `I:Network/SonicURLProtocol.m:84-138`). Test: session trace CS-12 (Android). <Badge type="tip" text="FACT" /> Trace: no connection (CS-12) and a cut before the intercept (CS-13b) give the intercept `null`, codes -1 / -1.

**D-51 Headers of a `304`.** <Badge type="tip" text="FACT" /> Android keeps the stored headers (`SS:828-840` writes nothing). iOS replaces the stored response headers with the filtered headers of the `304` (`I:Session/SonicSession.m:703-711`, `I:Cache/SonicCache.m:577-589`). Test: session trace CS-03 (Android). <Badge type="tip" text="FACT" /> Trace: after the `304` the header file still has the fields of the first load (CS-03).

**D-52 Headers of the cached page.** <Badge type="tip" text="FACT" /> Android stores all response headers and removes `Set-Cookie`, `Cache-Control`, `Expires`, `Etag` and multi-valued fields when it gives them to the WebView (`SU:369-391`, `SU:422-443`, `SS:1366-1369`). iOS removes `max-age`, `cache-offline`, `expires`, `template-tag`, `template-change` and every name that contains `cookie` before it stores them, and gives the rest to the web view (`I:Cache/SonicCache.m:792-812`, `I:Session/SonicSession.m:610-620`). Test: Android session trace (`client.load_data` and `client.intercept` headers). <Badge type="tip" text="FACT" /> Trace: the header file keeps `set-cookie` and `etag`; the WebView headers have neither (CS-17, CS-01). They keep `content-encoding: gzip` and `transfer-encoding: chunked` while the stream is decoded.

Evidence of the session behavior: [legacy client behavior](/engineering/protocol/legacy-client). Markers: [markers grammar](/engineering/protocol/legacy-markers).

## 3. Upstream behaviors that Rivqen will not reproduce

These behaviors are documented facts from the code. Rivqen replaces them with safe behavior, even in legacy mode. Each item becomes a `SEC-` ticket and a fixture that proves the safe behavior.

| # | Upstream behavior | Risk | Rivqen behavior | Evidence |
|---|---|---|---|---|
| U-01 | Java filter stores extracted blocks in **static** fields shared by all requests (`TemplateReplace.diffTagNames`) | Under concurrent requests, one user's data blocks can appear in another user's response | Per-request state only | `SonicFilter.java:20-48, 124-130` |
| U-02 | Cache key ignores ordinary query parameters | Different pages (or users, if IDs are in the query) share one cache entry | Full canonical URL + partition | `SonicRuntime.java:75-113` |
| U-03 | Account name in clear text in cache file names | Account disclosure from the file system | Keyed hash partition | `SonicRuntime.java:105-106` |
| U-04 | JS bridge via `addJavascriptInterface` and `javascript:` URL built from a page-supplied callback name | Any frame can call; script injection through the callback name | Origin-checked message bridge | sample `SonicJavaScriptInterface.java:51-71` |
| U-05 | Quick mode loads HTML as data without response security headers | CSP and other headers lost | Security headers are always preserved, or the page loads normally | `QuickSonicSession.java:39-43` |
| U-06 | Subresource cache on external storage `/sdcard/SonicResource/` | Readable and writable by other apps on old Android | App-private storage | `assets/VasSonic3.0_preload.md` §1.3 |
| U-07 | `Cache-Control` ignored unless `SUPPORT_CACHE_CONTROL` is on | `no-store` responses can be cached | `no-store` always wins | `SonicSessionConfig.java:72`; `SonicUtils.java:575-596` |
| U-08 | `sonic-sdk-version` header on every request | SDK fingerprinting | Off by default | `SonicSessionConnection.java:361` |
| U-09 | `Etag` value unquoted (not a valid HTTP entity-tag) | Intermediaries can mishandle it | Rivqen server emits the legacy value unquoted for compatibility, plus an RFC 9110 `ETag` only in RQP | `SonicFilter.java:103` |
| U-11 | Java and PHP corrupt pages with more than one `<title>` element (trace S16) | The page title is copied into another element (for example an SVG title) on rebuild | No implicit title rule in RQP; Node.js rule in the legacy module | `SonicFilter.java:110-112`; `sonic.php:99-104` |
| U-10 | iOS integration with `UIWebView` and global `NSURLProtocol` | `UIWebView` is no longer accepted in the App Store; `NSURLProtocol` does not intercept `WKWebView` through public API | WKWebView with public APIs (ADR-003) | `sonic-iOS/SonicSample/SonicSample/SonicWebViewController.m:71`; `AppDelegate.m:35` |
| U-12 | All three servers send the full document and the data response for the same URL with the same `Etag`, without `Vary`, and keep CDN cache fields of the application | A shared cache that obeys `CDN-Cache-Control` or `Surrogate-Control` stores a data response (JSON) and gives it to every user as the page | `Vary: template-tag`; no CDN cache fields on legacy responses ([legacy wire](/engineering/protocol/legacy-wire#canonical-server) C-13, C-15; H-111); every profile | Traces S06, S07: no `Vary` |
| U-13 | All three servers apply legacy processing to responses that are not a `200` to a `GET`: a `302` with a matching `If-None-Match` becomes a `304` (the redirect is lost), a `404` page becomes a data response, a `POST` response gets the legacy fields; `HEAD` differs per server (traces S24–S27, 2026-10-31) | A legacy client follows no redirect and caches or patches an error page as the page | Legacy processing only for `GET` with status `200` and `text/html` ([legacy wire](/engineering/protocol/legacy-wire#canonical-server) C-01, DL-074); every profile (DL-082) | Traces S24–S27 |

## 4. Parity matrix {#parity-matrix}

The parity matrix lists every upstream feature and how Rivqen provides it. 1.0 requires every row to be **Exact**, **Equivalent**, or a documented **Conditional / Unsupported** with an ADR.

| ID | Upstream feature | Rivqen | Level | Proof |
|---|---|---|---|---|
| PAR-01 | Early HTML fetch in parallel with WebView start | Parallel prepare and fetch in adapters | Exact (Android), Conditional (iOS) | Waterfall timestamps |
| PAR-02 | Bridge stream (cached prefix + network rest) | Bounded stream model | Exact (Android), Conditional (iOS) | First byte vs first paint on devices |
| PAR-03 | Quick mode | `quick` compatibility behavior on Android | Equivalent (security headers kept) | Golden Quick traces |
| PAR-04 | Standard mode | `standard` compatibility behavior | Exact | Golden Standard traces |
| PAR-05 | Instant display of cached page | L1/L2 snapshots | Exact | Airplane-mode test |
| PAR-06 | `304` hit | Validators + `304` | Exact | `FX-LEG-304` |
| PAR-07 | Template/data split by markers | Formal parser, canonical profile | Exact for canonical; profiles for divergences | Marker fixtures |
| PAR-08 | Data update through JS callback | Validated diff through secure bridge (+ shim) | Equivalent | Bridge tests |
| PAR-09 | Template change → full refresh | Atomic replace | Exact | `FX-LEG-TEMPLATE-CHANGE` |
| PAR-10 | `cache-offline` values `true`, `false`, `store`, `http` | Implemented in legacy mode | Exact (`no-store` overrides) | Four fixtures |
| PAR-11 | Local server mode (VasSonic 2.0) | Client-side split for non-legacy servers | Equivalent, opt-in | Fixture with plain server |
| PAR-12 | Subresource preload (`sonic-link`, VasSonic 3.0) | Budgeted prefetch in app-private storage | Equivalent | Prefetch tests |
| PAR-13 | React SSR demo (Next.js, Redux, Koa) | TS core, React hooks, SSR adapters | Equivalent | React SSR suite |
| PAR-14 | Java server | Java SDK (Servlet, Spring Boot) | Exact on canonical profile | Conformance |
| PAR-15 | Node.js server | Node SDK (Fetch, Express, Fastify) | Exact on canonical profile | Conformance |
| PAR-16 | PHP server | PSR-15 middleware | Exact on canonical profile | Conformance |
| PAR-17 | iOS SDK | Swift + WKWebView + Rust core | Conditional (ADR-003) | iOS parity matrix |
| PAR-18 | Android SDK | Kotlin + AndroidX WebKit + Rust core | Exact | Android matrix |
| PAR-19 | HTTP stack | Platform HTTP/1.1–3 | Equivalent | Network suite |
| PAR-20 | Memory and cache limits | Bounded L1/L2, pressure callbacks | Improved | Memory profiles |
| PAR-21 | Statistics callbacks | Typed metrics | Improved | Telemetry tests |
| PAR-22 | Security posture | TLS/CSP/origin/partition/OWASP | Improved (U-01…U-10) | Security gate |
| PAR-23 | Custom request/response headers | Config hooks | Equivalent | Unit tests |
| PAR-24 | IP-direct connection with SNI | Not planned (platform stacks handle DNS) | Unsupported — ADR needed | — |

## Related

- [ADR-005](/engineering/architecture/adr/#adr-005)
- [Security controls](/engineering/security/controls)
