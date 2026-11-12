# ADR-003: iOS navigation strategy

- Status: **Accepted** (human, 2026-11-09, H-136)
- Date: 2026-11-08
- Owner: Architect, WP-04 (data); WP-11 implements
- Reviewers: architecture lead, security reviewer, iOS SDK owner (WP-11)

This record chooses how the iOS SDK puts a page into `WKWebView` for each page class, with the WP-04 spike data. It is the input for the decision of the human at gate G2.

**Status:** <Badge type="info" text="DESIGN" /> for the options and the recommendation. <Badge type="tip" text="FACT" /> for the spike results. Times are **indicative (H-129)**: iPhone 16 Pro simulator, no device.

[[toc]]

## Context

<Badge type="tip" text="FACT" /> iOS has no public API that intercepts the `https` main-document load of `WKWebView`. `setURLSchemeHandler` raises an exception for a scheme that WebKit handles, such as `https` ([Apple documentation](https://developer.apple.com/documentation/webkit/wkwebviewconfiguration/seturlschemehandler(_:forurlscheme:))). Risk RSK-01 and RSK-02 and question Q-04 come from this ([risks](/engineering/plan/risks), [open questions](/engineering/plan/open-questions)).

WP-04 built one prototype app with three modes ([iOS SDK](/engineering/platforms/ios) §3):

| Prototype | Mechanism |
|---|---|
| IOS-P1 | Native `URLSession` fetch, then `loadSimulatedRequest` (or `loadHTMLString` with a base URL) |
| IOS-P2 | Cached page first, then a real `load(URLRequest)`, then a data update through the bridge |
| IOS-P3 | Own scheme with `WKURLSchemeHandler` |

<Badge type="tip" text="FACT" /> The human decided before this ADR: H-131 (when the SDK may make a native early fetch with the cookies of the store) and H-132 (what "same site" means for H-131). See [HTTP stacks](/engineering/transport/http) §2.

### Evidence

<Badge type="tip" text="FACT" /> Each capability has 3 runs with the same normalized log on iOS 27.0, 26.5 and 18.5. The verdicts are the same on the three versions. Runs: `evidence/spikes/wp-04/ios/<capability>/<os>/` ([folder](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/ios)); catalog: [`capabilities.json`](https://github.com/robotoss/rivqen/blob/main/tools/platform-spikes/capabilities.json).

| ID | Capability | Verdict |
|---|---|---|
| CAP-I-01 | IOS-P1 renders the fetched bytes | Supported |
| CAP-I-02 | IOS-P1 location, origin, relative URLs | Supported |
| CAP-I-03 | IOS-P1 cookies | Conditional |
| CAP-I-04 | IOS-P2 snapshot, navigation, bridge | Supported |
| CAP-I-05 | IOS-P3 streams the response | Supported |
| CAP-I-06 | IOS-P3 origin, cookies, subresources | Conditional |
| CAP-I-07 | `https` main-document streaming | Impossible |
| CAP-I-08 | CSP field and `meta`, nonces | Supported |
| CAP-I-09 | Service Worker | Conditional |
| CAP-I-10 | History, back and forward | Conditional |
| CAP-I-11 | Cookie sync, both directions | Conditional |

<Badge type="tip" text="FACT" /> The conditions in short ([iOS SDK](/engineering/platforms/ios) §2, §3):

1. **Cookies (CAP-I-03, CAP-I-11).** WebKit ignores the `Set-Cookie` field of a simulated response and does not share the `URLSession` cookies. The SDK must copy cookies into `WKHTTPCookieStore` before the load. The copy keeps `HttpOnly`, `Secure` and `SameSite`. `URLSession` does not apply `SameSite`. Copying 10 cookies one after the other took 337 to 358 ms.
2. **CSP (CAP-I-08).** A `Content-Security-Policy` field applies with `loadSimulatedRequest`, `load(URLRequest)` and the own scheme. With `loadHTMLString` it is lost; only a `meta` policy applies.
3. **Service Worker (CAP-I-09).** `navigator.serviceWorker` is missing in a normal app with every prototype. Only an app with `WKAppBoundDomains` and `limitsNavigationsToAppBoundDomains` registers a worker. App-Bound Domains allow at most 10 domains and deny script injection, message handlers and cookie APIs outside them. Simulator only (H-129).
4. **History (CAP-I-10).** Back to a `loadSimulatedRequest` entry loads the URL again from the network; the given data is not reused. `loadHTMLString` makes no entry of its own.
5. **Own scheme (CAP-I-06).** The page origin is the own scheme (`rqlab://…`), `document.cookie` is empty, a script cannot set a cookie, and a script reads a cross-origin `https` response only with CORS for that origin.
6. **Streaming (CAP-I-05, CAP-I-07).** Only the own scheme streams (`didReceive(_:)` for each 2048-byte chunk; the script of a chunk runs before the next chunk exists). IOS-P1 needs the whole body first; IOS-P2 lets WebKit stream, but the app sees no bytes.

<Badge type="tip" text="FACT" /> Times (indicative, H-129; local spike host): the native fetch ends 17 to 30 ms after its start. In a new web view, `loadSimulatedRequest` commits 367 to 732 ms after the load call, and the first contentful paint is 26 to 97 ms after the navigation start (CAP-I-01). With IOS-P2 the snapshot stays on screen until the real navigation commits, 28 to 363 ms later (CAP-I-04). Metrics: `run-1.metrics.json` in each run folder, for example [CAP-I-01 27.0](https://github.com/robotoss/rivqen/blob/main/evidence/spikes/wp-04/ios/CAP-I-01/27.0/run-1.metrics.json).

<Badge type="tip" text="FACT" /> **iOS 18.5 launch.** An app with a deployment target before iOS 18.4, built with Xcode 27.2, that calls a Swift-refined WebKit API, stops at launch on the iOS 18.5 simulator runtime ([WebKit bug 293831](https://bugs.webkit.org/show_bug.cgi?id=293831)). The lab uses a launch workaround for the app process only ([iOS SDK](/engineering/platforms/ios) §3).

### Not measured

| Item | State |
|---|---|
| Redirects of the main document on iOS | NOT FOUND (spike matrix item 1) |
| HTTP authentication challenges, client certificates | NOT FOUND |
| `load(_:mimeType:characterEncodingName:baseURL:)` | NOT FOUND (`loadHTMLString` was measured) |
| IndexedDB, `localStorage`, deep links, iPad | NOT FOUND |
| Spike matrix items 6 and 8 (CORS, video, downloads; process end, memory pressure, offline) | NOT FOUND |
| Devices; the Service Worker entitlement on a device | NOT FOUND (H-129) |
| iOS 17 | NOT FOUND (no runtime, H-128) |

## Options

In every option, `loadHTMLString` is not used for a page of a site: it loses the CSP field (CAP-I-08, INV-03) and the history entry (CAP-I-10). Private API, a global `NSURLProtocol` and origin spoofing are forbidden (INV-10).

### Option A: IOS-P1 for every page class

Native early fetch (or the cache), then `loadSimulatedRequest` with the request URL, the fetched status and fields, and the bytes.

- Pros: early display on the cold and the warm path; the page gets the right URL, origin and CSP field.
- Cons: the SDK owns the cookie handoff for every page (CAP-I-03): a `Set-Cookie` that it does not copy is lost, for example a rotated session cookie. Redirects and authentication challenges are not measured. Back to the first entry loads from the network (CAP-I-10).
- Measured cost: the cookie copy (337 to 358 ms for 10 cookies, one after the other) comes before the load.

### Option B: IOS-P2 for every page class

Show the cached snapshot (by `loadSimulatedRequest`) when one exists, start the real navigation at once, and send data updates through the bridge.

- Pros: WebKit loads the document itself, with its own cookies, `SameSite`, authentication, redirects and history.
- Cons: no gain on the cold path; the SDK does not see the bytes of the real navigation, so a cache fill needs its own native request. The prototype made 3 requests: the native snapshot fetch, the WebKit navigation and the native data fetch (CAP-I-04).
- Measured cost: the snapshot is replaced when the real page commits (28 to 363 ms in the simulator).

### Option C: A prototype for each page class

The table of the [Recommendation](#recommendation).

- Pros: the strongest conditions apply only where the page needs them.
- Cons: three load paths in the SDK, and tests for each.

### Option D: IOS-P3 for every page class

- Rejected: the page gets another origin (CAP-I-06). That breaks cookies, CORS and Service Workers, and showing a site's content under another origin is origin spoofing (INV-10).

## Recommendation

<Badge type="info" text="DESIGN" /> **Option C.** One prototype for each page class:

| Page class | Load path | Main conditions |
|---|---|---|
| Public (`public` partition) | IOS-P1: `loadSimulatedRequest` | Cookie copy before the load; native fetch only by H-131 |
| Authenticated (user partition) | IOS-P2: snapshot, then real navigation | Native requests only by H-131 |
| Offline snapshot (allowed pages) | IOS-P1 with the stored snapshot | The SDK handles back and forward to the entry |
| Own local content | IOS-P3: own scheme | Never for a site's `https` page |

<Badge type="info" text="DESIGN" /> Rules for every class:

1. **No `https` streaming.** CAP-I-07 is impossible with public API. Rivqen does not promise main-document streaming on iOS. It keeps cache, revalidation and data updates.
2. **Cookies (H-131, H-132, CAP-I-11).** Before a native fetch, the SDK copies the cookies of the request URL from `WKHTTPCookieStore`. After it, the SDK writes each `Set-Cookie` into `WKHTTPCookieStore` and waits for the completion before the load. The SDK does not read `sameSitePolicy` to tell `None` from "not set".
3. **CSP.** The SDK gives the CSP field of the fetched or stored response to `loadSimulatedRequest` unchanged.
4. **History.** For a `back_forward` navigation to a `loadSimulatedRequest` entry, WebKit loads from the network. The SDK either lets it, or shows its snapshot again itself (WP-11 decides).
5. **Service Worker.** A page that needs a Service Worker gets it only in an app with App-Bound Domains, and only after a real navigation. The SDK does not turn App-Bound Domains on by itself (it limits the whole app).
6. **Redirects.** Until WP-11 measures them, a native fetch that meets a redirect gives no IOS-P1 load: the SDK starts a real navigation (fail closed, as CL-44 and DL-042).

Why IOS-P2 for authenticated pages: with IOS-P1, the SDK must copy every `Set-Cookie` and handle redirects and authentication challenges that WP-04 did not measure. A missed cookie can end the session of the user. IOS-P2 leaves these to WebKit and keeps the snapshot gain on the warm path. The cost is no early-fetch gain on the cold path of authenticated pages. IOS-P1 for authenticated pages needs a new ADR after WP-11 measures redirects and challenges.

Cost of the recommendation: three load paths; the cold path of authenticated pages is not faster than a plain `WKWebView`.

## Decision

<Badge type="info" text="DESIGN" /> **Accepted (H-136, human, 2026-11-09): one path per page class.** Public pages and offline snapshots: IOS-P1 (`loadSimulatedRequest` after the early fetch). Authenticated pages: IOS-P2 (the cached snapshot first, then a real navigation) until WP-11 measures redirects and authentication challenges on iOS. Own local content: IOS-P3. Never `loadHTMLString` for a page of a site. `https` main-document streaming is impossible with public API.

## Consequences

<Badge type="info" text="DESIGN" /> **Incompatibilities to document publicly (G2).** The [support matrix](/guide/support-matrix) and the `ios-parity-matrix.md` page (generated from the catalog) must state them:

| Difference from Android | Source |
|---|---|
| No main-document streaming for `https` pages | CAP-I-07 |
| Authenticated pages: no early fetch on the cold path | This ADR |
| No Service Worker without App-Bound Domains | CAP-I-09 |
| Back to an early-loaded entry loads from the network | CAP-I-10 |
| Own-scheme pages have another origin and no cookies | CAP-I-06 |

<Badge type="info" text="DESIGN" /> Follow-up work:

- WP-11 builds the three load paths and measures the items under [Not measured](#not-measured) before it closes.
- WP-11 and WP-21 test the SDK sample app on an iOS 18.4 or later runtime with the lowest deployment target (iOS 18.5 launch, above). Data for ADR-015.
- [HTTP stacks](/engineering/transport/http) §2 (iOS row) stays as written; this ADR adds the wait for the `setCookie` completion.

## Verification

<Badge type="danger" text="GATE" /> WP-11 proves the decision on devices:

1. For each page class, the capabilities of its load path pass on the supported iOS versions, on an iPhone and an iPad.
2. Redirects, authentication challenges and the spike matrix items 6 and 8 have runs.
3. The generated `ios-parity-matrix.md` agrees with the catalog (`matrix/gen.mjs --check`).

## Related

- [iOS SDK](/engineering/platforms/ios), [HTTP stacks](/engineering/transport/http)
- [ADR-002](/engineering/architecture/adr/ADR-002-ffi-technology), [ADR-009](/engineering/architecture/adr/ADR-009-streaming-abi), [ADR register](/engineering/architecture/adr/)
- [WP-04 S1](/engineering/plan/sprints/WP-04-S1), [WP-04 S2](/engineering/plan/sprints/WP-04-S2)
- [Risks](/engineering/plan/risks) RSK-01, RSK-02; [open questions](/engineering/plan/open-questions) Q-04, Q-12
