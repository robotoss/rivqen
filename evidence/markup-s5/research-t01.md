# S5 research: `template` and `noscript` in block content (T-01)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence for WP-17 S5, task T-01 (sprint record `docs/engineering/plan/sprints/WP-17-S5.md`). Sources checked on 2026-10-19. The research lane returned its report as text; the architect wrote this file from it. Spec pages were read through a summarizing fetch tool: step numbers come from that output, not from a full read. Marks: FACT (source given), RESEARCH (an inference), NOT FOUND.

Spec bases: P = WHATWG HTML §13 Parsing (`https://html.spec.whatwg.org/multipage/parsing.html`); D = Dynamic markup insertion (`…/dynamic-markup-insertion.html`); S = Scripting (`…/scripting.html`).

## 0. Main finding: the scripting model of the parser changed

- FACT (P §13.2.4.5 "Other parsing state flags"): the parser has a **scripting mode** with four values: Normal, Disabled, Inert, Fragment. It is no longer one boolean flag.
  - Normal: scripts run on insertion. Disabled: `noscript` can hold markup. Inert: scripts are marked "already started" and do not run (the default of the fragment parser). Fragment: scripts run when inserted.
- RESEARCH: the two "scripting views" of RQP (markup §2) are the full-document modes Normal and Disabled. In Normal and Inert, `noscript` is RAWTEXT; only Disabled parses its content as markup.

## 1. `template` in body and in block content

| Statement | Mark | Source |
|---|---|---|
| The contents are a DocumentFragment whose node document is the "appropriate template contents owner document", an inert document with a null browsing context; scripts do not run | FACT | S §4.12.3 |
| Adoption adopts the contents too; a clone with the subtree flag clones the contents | FACT | S §4.12.3 |
| Image loads and form association inside the contents | NOT FOUND | — |
| Start tag `template` in "in head": insert a marker into the list of active formatting elements, frameset-ok "not ok", switch to and push "in template" | FACT | P §13.2.6.4.4 |
| "in body" sends `template` start and end tags to "in head" | FACT | P §13.2.6.4.7 |
| Start tag `form` in "in body": the form element pointer blocks it only when the parser is not "parsing template contents" | FACT | P §13.2.6.4.7 |
| "in template": characters, comments, DOCTYPE use "in body"; `base`, `link`, `meta`, `style`, `template` and other head start tags use "in head"; any other start tag pops the template mode, pushes the matching mode ("in body" for most names) and reprocesses; other end tags are ignored | FACT | P §13.2.6.4.16 |
| "in template", end of file: pop up to a `template`, clear the active formatting elements to the last marker, pop the template mode, reset the insertion mode, reprocess | FACT | P §13.2.6.4.16 |
| "reset the insertion mode" step 10: a `template` node gives the current template insertion mode | FACT | P §13.2.4.1 |
| The marker keeps formatting elements opened before the template from reopening inside it | RESEARCH | from the marker rule |
| Foster parenting with a `template` in a table; implied end tags and the form pointer in full | NOT FOUND | P §13.2.6.1 not read |

## 2. Declarative shadow DOM (`shadowrootmode`)

| Statement | Mark | Source |
|---|---|---|
| "in head", `template` start tag: reads `shadowrootmode`, `shadowrootdelegatesfocus`, `shadowrootserializable`, `shadowrootclonable` and the slot assignment | FACT | P §13.2.6.4.4 |
| It inserts a normal `template` when declarative shadow roots are not allowed, when the host is the topmost element of the stack (`hostIsRoot`), when the host already is a shadow host, when it has an insertion-target redirection, or when the attach throws | FACT | P §13.2.6.4.4 |
| Else it attaches a shadow root; the template contents become the shadow root; only the first such template per host takes effect | FACT | P §13.2.6.4.4; MDN `template` |
| Whether scripts run and images load in the shadow content | NOT FOUND | — |
| The fragment algorithm defaults to "declarative shadow roots not allowed"; `innerHTML` and `insertAdjacentHTML` keep it off; `setHTMLUnsafe` and `parseHTMLUnsafe` turn it on | FACT | P §13.4; D |
| `DOMParser` and `createContextualFragment`: the DSD value | NOT FOUND | D |
| Support (MDN browser-compat-data): `shadowrootmode` in Chrome and WebView Android 111, Safari and Safari iOS 16.4, Firefox 123; `setHTMLUnsafe` and `parseHTMLUnsafe` in Chrome and WebView Android 124, Safari 26 (partial from 17.4) | FACT | `mdn/browser-compat-data` `html/elements/template.json`, `api/Element.json`, `api/Document.json` |

## 3. `template for` (content patching, H-43)

| Statement | Mark | Source |
|---|---|---|
| WHATWG "in head", `template` start tag: shadowrootmode first, then `for` ("prepare content patching"), then normal insertion | FACT | P §13.2.6.4.4 |
| Chrome and Edge ship it from version 150 (an older page says 148 behind a flag); Firefox and Safari: not supported | FACT | `developer.chrome.com/blog/declarative-partial-updates` (2026-05-19, updated 2026-09-08) |
| With `innerHTML` or `setHTML`, the parent of the template is an intermediate fragment, so it cannot change the existing DOM; `streamHTMLUnsafe` has no such fragment | FACT | same |
| The WebKit position; the first Android System WebView version with it | NOT FOUND | — |

## 4. `noscript` and scripting

| Statement | Mark | Source |
|---|---|---|
| "in head" and "in body": a `noscript` start tag is RAWTEXT unless the scripting mode is Disabled; in Disabled, "in head" switches to "in head noscript" | FACT | P §13.2.6.4.4, §13.2.6.4.5, §13.2.6.4.7 |
| Fragment parsing step 8: the mode is Disabled when the context document has scripting disabled; else the default Inert. So `innerHTML` in a document with scripting on keeps `noscript` content as text | FACT | P §13.4; D |
| `parseHTMLUnsafe` and `DOMParser.parseFromString` parse with scripting Disabled (no browsing context), so `noscript` content becomes elements | FACT | D |
| Chromium: the parser scripting flag comes from the frame (`CanExecuteScripts`, then `ScriptEnabled()`) | FACT | Chromium `third_party/blink/renderer/core/html/parser/html_parser_options.cc`, `core/frame/local_dom_window.cc` |
| WebKit: the scripting flag needs a frame whose script controller allows execution and content JavaScript allowed | FACT | WebKit `Source/WebCore/html/parser/HTMLParserOptions.cpp` |
| So `allowsContentJavaScript = false` (iOS) and `setJavaScriptEnabled(false)` (Android) very likely make the document parser use Disabled | RESEARCH | the two sources above |
| `evaluateJavaScript` in a `WKContentWorld`: DOM changes are visible in all worlds; it works with content JavaScript off (measured in T-03, iOS 26.5) | FACT | Apple `evaluateJavaScript(_:in:in:completionHandler:)`; `tools/webview-lab/results/ios/run.json` |
| Android: how an app reads the DOM with JavaScript off | NOT FOUND at T-01; measured in T-04: `evaluateJavascript` does not run and the web message listener is not installed with JavaScript off; an `androidx.webkit` isolated-world script runs (Chromium `ClassicScript::RunScriptInIsolatedWorldAndReturnValue` uses `kExecuteScriptWhenScriptsDisabled`) | `tools/webview-lab/README.md` (Android) |

## 5. How an SDK can apply a block

| Method | Scripting mode | New `script` runs | DSD attached | `noscript` content becomes elements |
|---|---|---|---|---|
| `innerHTML`, `insertAdjacentHTML` | Inert, or Disabled in a document with scripting off | No (FACT) | No (FACT) | No with scripting on; yes with scripting off |
| `Range.createContextualFragment` | `runScripts` true, likely Fragment (RESEARCH) | Likely yes on insertion (RESEARCH) | NOT FOUND | No with scripting on (RESEARCH) |
| `setHTMLUnsafe` | Inert by default | No by default | Yes (FACT) | As `innerHTML` |
| `parseHTMLUnsafe` or `DOMParser`, then move the nodes | Disabled | No in the parse | Yes | **Yes** |

## 6. Known mutation XSS classes

| Statement | Mark | Source |
|---|---|---|
| `noscript` mXSS: a Disabled parse (`DOMParser`) reads `noscript` content as markup, a scripting-enabled document reads it as text up to the first `</noscript>`; a sanitizer and the browser then see different trees | FACT | `jorianwoltjer.com/blog/p/research/mutation-xss` |
| CVE-2024-52595 (`lxml_html_clean`, GHSA-5jfw-gq64-q45f) uses that mismatch | FACT | same |
| An mXSS class specific to the HTML `template` element | NOT FOUND | — |

## 7. Gaps

P §13.2.6.1 (foster parenting with `template`); the WebKit and WebView status of `template for`; `evaluateJavaScript` with content JavaScript off and DOM reading on Android with JavaScript off (the WebView lab, T-03 and T-04, measures them); the `createContextualFragment` mode in engine source.
