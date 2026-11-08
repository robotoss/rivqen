# URL serialization of the native stacks (Q-24)

This page records how the URL types of the native stacks write the same URL, compared with the WHATWG serialization that the binding uses ([manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields)). It is the platform evidence for the open question Q-24 ([open questions](/engineering/plan/open-questions)). It does **not** close Q-24 (DL-135): the kill-switch threshold, the intermediary test and the nonce test stay with WP-13 and WP-10.

**Status:** <Badge type="tip" text="FACT" /> for each row that has a result file in `evidence/spikes/wp-04/url/`. <Badge type="warning" text="RESEARCH" /> for the reading of the results. Measured on 2026-11-08.

## Method

<Badge type="tip" text="FACT" /> One list of 67 inputs (`tools/platform-spikes/url-interop/urls.json`, a reason for each entry, example hosts only, 4 inputs with a base) goes through each stack. The runners only parse; they open no connection. Each stack writes `evidence/spikes/wp-04/url/<stack>.json`; `compare.json` compares every stack with `whatwg-node` for the host, the path, the query and the full `href` separately.

| Stack | Version | `href` is | host, path, query |
|---|---|---|---|
| `whatwg-node` (reference) | Node.js v26.8.2, ada 4.0.0 | `new URL(input, base).href` | `hostname`, `pathname`, `search` without `?` |
| `okhttp` | OkHttp 5.5.1 (the `okhttp-android` artifact of the Android spike), JDK 21.0.11, Okio 3.18.1 | `HttpUrl.get(input).toString()`; with a base `HttpUrl.get(base).resolve(input)` | `host()`, `encodedPath()`, `encodedQuery()` |
| `java-net-uri` | `java.net.URI`, JDK 21.0.11 | `new URI(input).toASCIIString()`; `toString()` is in `extra` | `getHost()`, `getRawPath()`, `getRawQuery()` |
| `android-net-uri` | `android.net.Uri` of the framework, Android 17 (API 37) emulator `sdk_gphone64_arm64` (the dex ran with `app_process`) | `Uri.parse(input).toString()` | `getHost()`, `getEncodedPath()`, `getEncodedQuery()` |
| `foundation-url-macos`, `foundation-url-ios-sim` | Foundation of macOS 26.6.2 (25G83) and of the iOS 27.0 (24A434) simulator, Swift 6.4.2 | `URL(string:)` (with a base: `URL(string:relativeTo:)?.absoluteURL`), `absoluteString` | `host(percentEncoded: true)`, `path(percentEncoded: true)`, `query(percentEncoded: true)` |
| `foundation-urlcomponents-macos`, `foundation-urlcomponents-ios-sim` | same | `URLComponents(string:)`, `.string`; `url?.absoluteString` is in `extra` | `percentEncodedHost`, `percentEncodedPath`, `percentEncodedQuery` |

The host comparison removes the brackets of an IPv6 host. A query that is empty (`?`) and a missing query are different values. A stack that refuses an input gives the error class in `error`; for Foundation it is "returned nil" (no class exists).

## Result

<Badge type="tip" text="FACT" /> Inputs with the same `href` as WHATWG (of 67). WHATWG refuses 3 inputs (`host-space`, `port-too-big`, `port-non-digit`); the last column counts the inputs that the stack refuses too, and they are not in the first column:

| Stack | Equal `href` | Different `href` | Refuses what WHATWG accepts | Accepts what WHATWG refuses | Both refuse |
|---|---|---|---|---|---|
| OkHttp `HttpUrl` | 57 | 5 | 2 | 0 | 3 |
| `java.net.URI` | 19 | 26 | 19 | 2 | 1 |
| Foundation `URL` (macOS = iOS 27.0 simulator) | 28 | 34 | 2 | 1 | 2 |
| Foundation `URLComponents` (macOS = iOS 27.0 simulator) | 28 | 34 | 2 | 1 | 2 |
| `android.net.Uri` (API 37 emulator) | 22 | 38 | 4 (the 4 inputs with a base: no resolve API) | 3 | 0 |

<Badge type="tip" text="FACT" /> The iOS 27.0 simulator gives the same `href`, host, path and query as macOS 26.6.2 for all 67 inputs and both Foundation types. The iOS 26.5 and iOS 18.5 runtimes were not run: NOT FOUND.

### The Q-24 classes

| Class | OkHttp | `java.net.URI` | Foundation `URL` and `URLComponents` |
|---|---|---|---|
| `'` in the query | writes `%27`, same as WHATWG | keeps `'` (differs) | keeps `'` (differs) |
| `\|` `^` `` ` `` `{` `}` in the query | keeps them, same as WHATWG | refuses (`URISyntaxException`) | writes `%7C %5E %60 %7B %7D` (differs) |
| `[` `]` in the query | keeps them, same | keeps them, same | writes `%5B %5D` (differs) |
| backslash in the query | keeps it, same | refuses | writes `%5C` (differs) |
| invalid escape (`%zz`, `%`) | keeps it, same | refuses | writes `%25zz%25` (differs) |
| trailing-dot host `a.example.` | keeps the dot, same | same | same |
| percent-encoded unreserved in the query (`%41`, `%7e`) | keeps them, same | same | same |
| percent-encoded unreserved in the path | removes the dot segment `%2e%2E` like WHATWG, same | keeps it (differs) | keeps it (differs) |
| backslash in the path | `/`, same | refuses | `%5C` (differs) |
| backslash after the host | `/`, same | refuses | refuses |
| IDN host `faß.example` | `xn--fa-hia.example`, same | writes `fa%C3%9F.example` in `href`, `getHost()` gives `null` (differs) | `href` has `xn--fa-hia.example`, same; the host accessors give the Unicode form |
| user information, port forms | same except: an out-of-range or zero port is refused; keeps `127.1`-style hosts | keeps default ports and empty ports (differs) | keeps default ports, empty port and `:@` (differs) |
| empty path, dot segments | `/` and removed, same | keeps `a.example`, keeps `./` and `../` (differs) | same as `java.net.URI` |

<Badge type="tip" text="FACT" /> Other differences of `HttpUrl`: the path characters `\|` are written as `%7C` (WHATWG keeps `\|`); a space in a fragment stays a space (WHATWG writes `%20`; the fragment is not sent); the hosts `127.1`, `0x7f.0.0.1` and `2130706433` stay as written (WHATWG writes `127.0.0.1`); a host `.` and the port `0` are refused (WHATWG accepts them).

<Badge type="tip" text="FACT" /> Security-relevant row: for the input `https://a.example\@b.example/p`, WHATWG reads the host `a.example` (the backslash ends the authority, the path is `/@b.example/p`). `HttpUrl` gives the same. Foundation `URL` and `URLComponents` give the host `b.example` and the user information `a.example\`. `java.net.URI` refuses the input. `android.net.Uri` gives the host `a.example` (same as WHATWG) but the path `\@b.example/p` (differs). The SDK must not take the origin from a re-parse with Foundation (DL-040 already says so for iOS); this row is the evidence.

### Reading <Badge type="warning" text="RESEARCH" />

- `HttpUrl` is the closest stack to WHATWG for the Q-24 query classes (0 differences for `'`, `|`, `^`, backtick, braces). Its differences are in the host forms (IPv4 shorthand) and in a few path characters.
- `java.net.URI` and the Foundation types would give another `href` for many URLs. If the SDK builds the binding input U from them, a delta navigation for such a URL ends in `INTEGRITY_ERROR` and one full load (see Q-24). The list shows that the SDK needs a WHATWG-conformant serializer of its own (or takes the string from `HttpUrl` and `URL.absoluteString` only where the table says "same"). This is an input for the decision of WP-13, not a decision.
- The URL of the real request on iOS is not the issue of this page: `URLResponse.url` was not run through a network task here (no network). NOT FOUND.
- `android.net.Uri` does not normalize: `toString()` gives the input string as written. Its `href` equals WHATWG only when WHATWG changes nothing. It keeps `'`, a space, quotes, angle brackets and non-ASCII characters in the query, default and empty ports, dot segments, an empty path, an upper-case host, an IDN host as Unicode, and `127.1`-style hosts. Other API levels and the WebView are not part of this run: NOT FOUND.

## Reproduce

```text
node tools/platform-spikes/url-interop/cli.mjs whatwg
sh tools/platform-spikes/url-interop/run-jvm.sh
sh tools/platform-spikes/url-interop/run-swift.sh macos
LAB_SIM_TAG=url sh tools/platform-spikes/url-interop/run-swift.sh ios-sim 27.0
sh tools/platform-spikes/url-interop/run-android.sh build   # then run (emulator slot, CONTRACT section 11)
node tools/platform-spikes/url-interop/cli.mjs compare
cd tools/platform-spikes/url-interop && npm test
```

The JVM jars are the ones of the Android spike in the lab Gradle cache; their sha256 values are in `tools/platform-spikes/url-interop/jars.sha256` and `run-jvm.sh` checks them.
