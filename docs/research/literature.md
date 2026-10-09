# Literature review

This page reviews scientific and engineering work that is relevant to Rivqen. For each group of papers, it says what the work found and what it means for Rivqen's design.

**Status:** <Badge type="tip" text="FACT" /> for bibliographic data. The "Meaning for Rivqen" sections are <Badge type="info" text="DESIGN" /> interpretation.

[[toc]]

## Delta encoding and partial updates

| Work | Venue | Key point |
|---|---|---|
| J. C. Mogul, F. Douglis, A. Feldmann, B. Krishnamurthy. *Potential benefits of delta encoding and data compression for HTTP.* [doi:10.1145/263105.263162](https://doi.org/10.1145/263105.263162) | ACM SIGCOMM 1997 | Trace study: many web responses are small changes of earlier versions; deltas plus compression save large fractions of bytes and latency. |
| G. Banga, F. Douglis, M. Rabinovich. *Optimistic Deltas for WWW Latency Reduction.* | USENIX Annual Technical Conference 1997 | A proxy sends a cached (possibly stale) version first, then a delta — the same "show cache, then patch" idea as Sonic. |
| J. Mogul, B. Krishnamurthy, F. Douglis, et al. *Delta encoding in HTTP.* [RFC 3229](https://www.rfc-editor.org/rfc/rfc3229) | IETF 2002 | Standard framework for deltas (`A-IM`, `226 IM Used`); rarely deployed. |
| M. Tsimelzon et al. *ESI Language Specification 1.0.* [W3C Note](https://www.w3.org/TR/esi-lang/) | W3C 2001 | Pages assembled from cacheable fragments at the edge — the template/fragment split at the CDN. |
| P. Meenan, Y. Weiss (eds.). *Compression Dictionary Transport.* [RFC 9842](https://www.rfc-editor.org/rfc/rfc9842) | IETF 2025 | Previous responses act as compression dictionaries; delta-like savings without application markup. |

**Meaning for Rivqen.** The benefit of sending only changes is well established since 1997. The open problem is deployment: generic HTTP deltas (RFC 3229) never became common. Sonic avoided this by using an application-level contract (markers + JSON). RFC 9842 is the first widely implemented standard path to similar savings. Rivqen keeps the application contract (needed for in-place updates) and evaluates dictionary compression for full documents (ADR-013).

## Page load performance

| Work | Venue | Key point |
|---|---|---|
| X. S. Wang, A. Balasubramanian, A. Krishnamurthy, D. Wetherall. *Demystifying Page Load Performance with WProf.* | USENIX NSDI 2013 | Dependency graphs of page loads; the critical path is dominated by network and computation dependencies, not by any single factor. |
| X. S. Wang, A. Balasubramanian, A. Krishnamurthy, D. Wetherall. *How Speedy is SPDY?* | USENIX NSDI 2014 | Multiplexing helps less than expected because of dependencies and browser computation. |
| M. Butkiewicz, D. Wang, Z. Wu, H. V. Madhyastha, V. Sekar. *Klotski: Reprioritizing Web Content to Improve User Experience on Mobile Devices.* | USENIX NSDI 2015 | Prioritizing content that matters to the user improves perceived load time on mobile. |
| R. Netravali, A. Goyal, J. Mickens, H. Balakrishnan. *Polaris: Faster Page Loads Using Fine-grained Dependency Tracking.* | USENIX NSDI 2016 | Scheduling with fine-grained dependencies reduces load time. |
| X. S. Wang, A. Krishnamurthy, D. Wetherall. *Speeding up Web Page Loads with Shandian.* | USENIX NSDI 2016 | A proxy computes the initial page state and ships it; the client renders faster. |
| V. Ruamviboonsuk, R. Netravali, M. Uluyol, H. V. Madhyastha. *Vroom: Accelerating the Mobile Web with Server-Aided Dependency Resolution.* | ACM SIGCOMM 2017 | Servers tell clients which resources to fetch, earlier than discovery by parsing. |
| R. Netravali, J. Mickens. *Prophecy: Accelerating Mobile Page Loads Using Final-state Write Logs.* | USENIX NSDI 2018 | Shipping the final page state reduces mobile load time and bytes. |
| Z. Wang, F. X. Lin, L. Zhong, M. Chishtie. *Why Are Web Browsers Slow on Smartphones?* | ACM HotMobile 2011 | Resource loading, not computation, dominates mobile page-load delay on the studied devices. |

**Meaning for Rivqen.** Two lessons. (1) Starting the critical request earlier and shipping a ready state (Shandian, Prophecy) gives large gains; Rivqen's early native fetch and cached snapshots apply this to in-app WebViews. (2) Transport upgrades alone (SPDY/HTTP/2) give less than expected; Rivqen therefore treats HTTP/3 as a measured option, not a promise. Subresource hints (Vroom) support Rivqen's prefetch design, within browser policies.

## Mobile caching

| Work | Venue | Key point |
|---|---|---|
| F. Qian, K. S. Quah, J. Huang, J. Erman, A. Gerber, Z. M. Mao, S. Sen, O. Spatscheck. *Web Caching on Smartphones: Ideal vs. Reality.* | ACM MobiSys 2012 | Many mobile apps and libraries implement HTTP caching incorrectly; a large share of traffic is redundant. |

**Meaning for Rivqen.** Correct HTTP caching is not a given. Rivqen implements RFC 9111 rules explicitly and tests them (header matrix), and it does not let legacy `cache-offline` override `no-store`.

## Transport

| Work | Venue | Key point |
|---|---|---|
| A. Langley et al. *The QUIC Transport Protocol: Design and Internet-Scale Deployment.* [doi:10.1145/3098822.3098842](https://doi.org/10.1145/3098822.3098842) | ACM SIGCOMM 2017 | QUIC reduces latency, most on poor networks. |
| A. M. Kakhki, S. Jero, D. Choffnes, C. Nita-Rotaru, A. Mislove. *Taking a Long Look at QUIC.* | ACM IMC 2017 | Gains depend strongly on conditions; QUIC can be worse in some settings (for example, reordering, some mobile devices). |

**Meaning for Rivqen.** HTTP/3 must be measured per scenario (B0/B2), and the platform stack decides the version. No universal speed-up is claimed.

## WebView and hybrid app security {#webview-and-hybrid-app-security}

| Work | Venue | Key point |
|---|---|---|
| T. Luo, H. Hao, W. Du, Y. Wang, H. Yin. *Attacks on WebView in the Android System.* | ACSAC 2011 | JS interfaces and WebView hooks expose apps to attacks from web content. |
| M. Georgiev, S. Jana, V. Shmatikov. *Breaking and Fixing Origin-Based Access Control in Hybrid Web/Mobile Application Frameworks.* | NDSS 2014 | Bridges in hybrid frameworks often ignore the real origin of the calling frame; foreign-origin content can use native capabilities. |
| P. Mutchler, A. Doupé, J. Mitchell, C. Kruegel, G. Vigna. *A Large-Scale Study of Mobile Web App Security.* | IEEE MoST 2015 | Large share of apps with WebViews have vulnerabilities (unsafe navigation, exposed interfaces, ignored TLS errors). |
| OWASP. [MASWE-0035 WebViews loading untrusted content](https://mas.owasp.org/MASWE/MASVS-PLATFORM/MASWE-0035/), [MASTG-BEST-0034](https://mas.owasp.org/MASTG/best-practices/MASTG-BEST-0034/) | OWASP MAS | Current weakness and best-practice entries for WebViews. |

**Meaning for Rivqen.** These works describe exactly the pattern that upstream used in its sample (`addJavascriptInterface`, page-controlled callback names). Rivqen's bridge checks the **real frame origin** through platform APIs (Georgiev et al.), exposes a closed list of message types, and never builds `javascript:` URLs from page data.

## Cache privacy

| Work | Venue | Key point |
|---|---|---|
| E. W. Felten, M. A. Schneider. *Timing Attacks on Web Privacy.* [doi:10.1145/352600.352606](https://doi.org/10.1145/352600.352606) | ACM CCS 2000 | Shared caches leak browsing history through timing. |

**Meaning for Rivqen.** Browsers moved to partitioned HTTP caches for this reason. Rivqen partitions its cache per account and per environment, and never shares entries across partitions.

## Measurement and benchmarking {#measurement-and-benchmarking}

| Work | Venue | Key point |
|---|---|---|
| J. Dean, L. A. Barroso. *The Tail at Scale.* [doi:10.1145/2408776.2408794](https://doi.org/10.1145/2408776.2408794) | CACM 2013 | Tail latency (p95/p99) matters for user experience at scale. |
| T. Kalibera, R. Jones. *Rigorous Benchmarking in Reasonable Time.* | ACM ISMM 2013 | Use repetition at the right levels and confidence intervals; single runs mislead. |
| T. Mytkowicz, A. Diwan, M. Hauswirth, P. F. Sweeney. *Producing Wrong Data Without Doing Anything Obviously Wrong!* | ACM ASPLOS 2009 | Hidden factors (environment size, link order) bias measurements. |

**Meaning for Rivqen.** The benchmark method requires fixed conditions, ≥ 30 repetitions, p50/p90/p95 with bootstrap confidence intervals, pre-registered exclusion rules and raw data publication ([Benchmarks](/engineering/quality/benchmarks)).

## Testing

| Work | Venue | Key point |
|---|---|---|
| K. Claessen, J. Hughes. *QuickCheck: A Lightweight Tool for Random Testing of Haskell Programs.* [doi:10.1145/351240.351266](https://doi.org/10.1145/351240.351266) | ACM ICFP 2000 | Property-based testing finds edge cases that example tests miss. |

**Meaning for Rivqen.** The core's invariants (round-trip, atomic commit, partition isolation, cancel) are property tests, and parsers are fuzzed.

## Upstream material

| Work | Note |
|---|---|
| Tencent VAS team. *QQ移动页面框架优化实践* (QQ mobile page framework optimization practice), PDF in `upstream:assets/` | Historical design talk; claims are not independently verified. Not redistributed by Rivqen. |
