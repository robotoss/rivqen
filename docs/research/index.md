# Research overview

This section holds the evidence behind Rivqen's design: what the upstream project actually does, which standards and technologies exist today, and what the scientific literature says about faster page loads, caching and WebView security.

| Page | Content |
|---|---|
| [Upstream: VasSonic audit](/research/upstream-vassonic) | Behavioral audit of Tencent VasSonic at a pinned commit: modules, findings, risks |
| [Technology landscape](/research/landscape) | Alternatives and complements: HTTP caching, Early Hints, Service Workers, Speculation Rules, dictionary compression, ESI, prerendering |
| [Standards](/research/standards) | RFCs and W3C specifications Rivqen relies on, with status |
| [Platform baselines (2026)](/research/platform-baselines) | Current versions of Android, iOS, Rust, web and server platforms |
| [Literature review](/research/literature) | Scientific and engineering papers, with a discussion of what they mean for Rivqen |
| [Sources and confidence](/research/sources) | All sources with their confidence level |

## Method

1. **Primary sources first.** Upstream code at a pinned commit, RFCs, W3C specifications, official platform documentation, package registries.
2. **Labels.** Every statement is FACT (with a source), DESIGN or RESEARCH.
3. **Clean room.** Research reads upstream code; implementation reads only the resulting specification. See [Clean-room provenance](/legal/provenance).
4. **Dates.** Version data is valid on the date stated on each page (9 October 2026 for this milestone).
