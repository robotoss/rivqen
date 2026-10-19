# WebView lab contract

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

**Status:** DESIGN. Owner: architect (WP-17 S5). The host part (`host/`) and the two apps (`app/WebViewLab/`, a Kotlin Multiplatform project with an Android app and an iOS app) follow this contract. Change it only through the architect.

The lab answers open questions Q-19 (what a WebView does with the `noscript` shapes of markup M-11 when scripting is disabled) and Q-20 (whether a WebView follows the delivery rules H-66 and DL-040: the `Content-Type` of a full document, UTF-8, the byte order mark). It compares a real WebView with parse5 8.0.1 in the same scripting view.

## 1. Parts

| Path | Owner | Content |
|---|---|---|
| `CONTRACT.md` | Architect | This file |
| `cases/<id>.json` | Host lane (generated), architect (list) | One case per file (section 2) |
| `host/build-cases.mjs` | Host lane | Writes `cases/` from the golden fixtures and the case list of section 2.3 |
| `host/server.mjs` | Host lane | HTTP/1.1 server on `127.0.0.1` (section 3) |
| `host/serialize.js` | Host lane | The DOM walker that both apps inject (section 4); plain ES2019, no module syntax, no dependency |
| `host/compare.mjs` | Host lane | Compares `results/<platform>/` with parse5 (section 5) and writes `results/<platform>/summary.md` |
| `app/WebViewLab/` | Human creates (H-93); iOS lane and Android lane | The Kotlin Multiplatform project (package `dev.rivqen.webviewlab`): `iosApp` (WKWebView), `androidApp` (`android.webkit.WebView`), `shared` (Compose Multiplatform). No JUnit 4 (EPL-1.0) |
| `results/ios/`, `results/android/` | iOS lane, Android lane | One result per case and view (section 4), plus `run.json` |

## 2. Cases

### 2.1 Case file

`cases/<id>.json`, UTF-8 JSON:

```json
{
  "id": "NOSCRIPT-SVG",
  "question": "Q-19",
  "source": "fixtures/rqp/markup/FX-RQ-MARKUP-NOSCRIPT-SVG/input.html",
  "headers": [["Content-Type", "text/html; charset=utf-8"]],
  "views": ["on", "off"],
  "probe": null
}
```

- `id`: `[A-Z0-9-]{1,64}`, unique; the file name is `<id>.json`.
- `source`: a repository path of the body bytes. The server sends these bytes unchanged.
- `headers`: the response header lines in order, sent exactly; two lines with the same name stay two lines. The server adds only `Content-Length` and `Connection: close` when the case has no such line. A header name is HTTP token bytes only; a header value is HTAB, 0x20–0x7E and 0x80–0xFF only; a code unit above 0xFF is rejected before the encoding to bytes (security review S5 F6).
- `views`: the scripting views to run: `on` (content JavaScript allowed) and `off` (content JavaScript not allowed).
- `probe`: `null`, or a CSS selector of one element whose `textContent` the result records (Q-20: the text of a non-ASCII marker).

### 2.2 URLs

The server serves case `<id>` at `/c/<id>`. `GET /cases.json` gives the list of the ids in order: a JSON array of strings (the content of `cases/index.json`, which `host/build-cases.mjs` writes), `Content-Type: application/json`. Nothing else is served (404).

### 2.3 Case list

- Q-19: every golden markup fixture whose id has `NOSCRIPT`, and the fixtures `EDGE-E3-*`, `EDGE-E8-*`, `EDGE-COMBO-NOSCRIPT-*`; views `on` and `off`; header `Content-Type: text/html; charset=utf-8`.
- Q-20 (H-66, DL-040; views `on` only), one page with a `<p id="m">` that holds `Grüße – 日本` in UTF-8:
  - `CT-UTF8`: `Content-Type: text/html; charset=utf-8`.
  - `CT-NONE`: no `Content-Type` line.
  - `CT-NO-CHARSET`: `Content-Type: text/html`.
  - `CT-LATIN1`: `Content-Type: text/html; charset=iso-8859-1`.
  - `CT-TWO-LINES`: two lines, `text/html; charset=utf-8` then `text/html; charset=iso-8859-1`.
  - `BOM-LATIN1`: the page with a UTF-8 byte order mark and `charset=iso-8859-1`.
  - `BOM-NONE`: the page with a UTF-8 byte order mark and no `Content-Type` line.
  - `META-LATIN1`: `charset=utf-8` in the header, `<meta charset="iso-8859-1">` in the page.
- Q-20 expectations (`host/lab.mjs`, `Q20_CASES`): `CT-UTF8`, `BOM-LATIN1` and `BOM-NONE` expect UTF-8 and the marker text (the byte order mark wins over the header, DL-040); `META-LATIN1` expects UTF-8 (the header charset wins over `meta`); `CT-LATIN1` expects windows-1252 (the label `iso-8859-1` maps to it) and the marker read as windows-1252. `CT-NONE`, `CT-NO-CHARSET` and `CT-TWO-LINES` are "info": no RQP rule decides the WebView result for them (H-66 makes the SDK send one exact `Content-Type` with UTF-8), so the lab records what the WebView does.
- S5 T-02 adds the `template` and `noscript` block shapes as `S5-*` cases (views `on` and `off`).

## 3. Server

- `node host/server.mjs [--port N]` binds `127.0.0.1` only (default port 18090). The Android emulator reaches it at `10.0.2.2:<port>` (Android 17, API 37: the app declares `android.permission.ACCESS_LOCAL_NETWORK` and the run script grants it with `adb shell pm grant`; the app never asks the user; DL-054); the iOS simulator at `127.0.0.1:<port>`.
- Each response is written to the socket as raw bytes (status line, the header lines of the case, an empty line, the body), so the header lines are exact.
- No other host is contacted. Case pages are test data; the server does not parse them.

## 4. Run and result

For each case and each view, an app:

1. Creates a new WebView. View `off`: iOS `WKWebpagePreferences.allowsContentJavaScript = false`; Android `WebSettings.setJavaScriptEnabled(false)`. View `on`: content JavaScript allowed.
2. Loads `/c/<id>` and waits for the end of the main-frame load (iOS `didFinish`; Android `onPageFinished`), plus at most 2 s.
3. Runs `host/serialize.js` in a context that page scripts do not share (iOS: `evaluateJavaScript` in `WKContentWorld.defaultClient`). When the platform cannot run it in view `off`, the result says so (`"walker": "unavailable"`, the reason in `error`) and the app records the reason in `run.json`. A lane MUST NOT turn content JavaScript on for a view `off` result. Methods in use: iOS runs the walker with `evaluateJavaScript` in `WKContentWorld.defaultClient` (it works with content JavaScript off); Android runs it in an `androidx.webkit` isolated world at the `load` event and sends the tree back through console messages, because `evaluateJavascript` and the web message listener do not work with JavaScript off (S5 T-04).
4. Writes `results/<platform>/<id>.<view>.json`:

```json
{
  "id": "NOSCRIPT-SVG",
  "view": "off",
  "walker": "ok",
  "tree": "| <html>\n|   <head>\n…",
  "characterSet": "UTF-8",
  "contentType": "text/html",
  "probe": null,
  "error": null
}
```

`tree` is the output of `serialize.js`: the html5lib tree-construction test format ("| " and two spaces per depth; `<svg path>` and `<math mi>` for foreign elements; attributes sorted by name on their own lines; `"text"` for text; `<!-- c -->` for comments; `<!DOCTYPE html>`; `content` for the contents of a `template`). In text, comment, attribute and doctype strings and in names, `\` is written `\\`, `"` is written `\"`, a line feed `\n` and a carriage return `\r`, so each node is one line and two different trees never give the same string. `serialize.js` reads every DOM property with the getter of its prototype (`Node.prototype`, `Element.prototype`, `HTMLTemplateElement.prototype`, `CharacterData.prototype`, `Attr.prototype`, `DocumentType.prototype`) called on the node, not with the property of the object (a form with `<input name="childNodes">` hides the properties of the form; security review S5 F7). `run.json` holds the platform, the OS version, the WebView or WebKit version, the device or simulator name, the date, and the walker notes.

## 5. Compare

`node host/compare.mjs --platform ios|android` parses each case body with parse5 8.0.1 (`tools/rqp-markup/crosscheck` dependency), with `scriptingEnabled` true for view `on` and false for view `off`, serializes it in the same format, and compares it with `tree`. For the Q-20 cases it compares `characterSet` and `probe` with the expected values of the case list (UTF-8 and the marker text, except the cases where the rule says another encoding wins). It writes `results/<platform>/summary.md` with one row per case and view: equal, different (with the number of different lines by index, a class list — depth beyond 512, inside script, missing lines, extra lines, changed line — and the first different line; security review S5 F5), or unavailable.

## 6. Rules

- Case pages never load anything from the network; the apps block every request that is not to the lab server.
- No secrets, no personal data.
- The apps are test tools; nothing here ships.
