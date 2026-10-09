# Technology landscape

This page compares Rivqen's approach with other ways to make web content load faster. Rivqen does not replace these technologies. It combines with them where they apply.

**Status:** <Badge type="tip" text="FACT" /> for the technologies (with sources); <Badge type="info" text="DESIGN" /> for the "use in Rivqen" column.

[[toc]]

## 1. Comparison

| Technology | What it does | Strengths | Limits for in-app pages | Use in Rivqen |
|---|---|---|---|---|
| **HTTP caching + revalidation** ([RFC 9111](https://www.rfc-editor.org/rfc/rfc9111)) | Store responses; validate with `ETag` / `304` | Universal, standard | Whole document only; WebView start delay remains | Base layer of the cache policy |
| **`stale-while-revalidate`** ([RFC 5861](https://www.rfc-editor.org/rfc/rfc5861)) | Serve stale while fetching fresh | Instant display | Shows old content until next visit | Used by the cache policy |
| **HTTP/2 and HTTP/3** ([RFC 9113](https://www.rfc-editor.org/rfc/rfc9113), [RFC 9114](https://www.rfc-editor.org/rfc/rfc9114)) | Multiplexing; QUIC transport | Lower latency on lossy networks | No reduction in bytes for changed pages | Used through platform stacks |
| **103 Early Hints** ([RFC 8297](https://www.rfc-editor.org/rfc/rfc8297)) | Server hints subresources before the final response | Earlier subresource fetch | Needs browser support for main navigation | Complements subresource prefetch |
| **Compression Dictionary Transport** ([RFC 9842](https://www.rfc-editor.org/rfc/rfc9842), Sept 2025) | A previous response is the compression dictionary for the next | Very small transfers for similar documents, no markup changes | Needs client and server support; WebView support varies | **Evaluate for RQP (ADR-013)** |
| **Delta encoding in HTTP** ([RFC 3229](https://www.rfc-editor.org/rfc/rfc3229)) | Generic deltas with `226 IM Used` | Standardized | Little deployment | Not used; RQP avoids `226` |
| **Service Workers** | Programmable cache in the page | Offline, custom strategies | Needs a first visit; not available or limited in some WebView setups | Not required; Rivqen works without |
| **Speculation Rules / prerender** | Browser prefetches or prerenders likely next pages | Instant navigations in browsers | Browser feature; WebView support differs | Out of scope for the SDK |
| **Edge Side Includes (ESI)** ([W3C Note 2001](https://www.w3.org/TR/esi-lang/)) | Edge assembles a page from fragments | Cache static parts at the edge | Server/CDN side only | Same template/fragment idea, different place |
| **SSR streaming + hydration** | Server streams HTML; client hydrates | Fast first paint | Full payload per visit | Supported by the React adapter |
| **Offline packages (app-bundled web assets)** | Ship HTML/JS/CSS inside the app | No network for assets | App release needed for updates | Planned as versioned offline packages with integrity |
| **Native WebView prefetch / prerender APIs** | Platform warms up pages | Platform-integrated | Platform- and version-specific | Investigated in WP-04 |

## 2. What makes Rivqen different

1. It **starts the main document request in native code** before the WebView is ready (where the platform allows).
2. It **splits documents into template and data** with an explicit, versioned contract between client and server.
3. It has **one deterministic core** for both mobile platforms, verified by shared fixtures.
4. It has a **compatibility mode** for an existing protocol (legacy protocol), so migration does not require a big-bang switch.

## 3. Open question: dictionary compression vs block patches

RFC 9842 can reduce the bytes of a changed page to a size close to a block patch, without markers. Block patches still have advantages: in-place update without reload, typed JSON data for React, and revision checks. ADR-013 compares both on real pages: bytes, CPU, latency, and platform support. <Badge type="warning" text="RESEARCH" />
