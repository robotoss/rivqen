# Benchmarks

Rivqen does not promise a speed-up without a baseline. This page defines what we measure, how, and what counts as a result.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 1. Baselines

| ID | Baseline | Purpose |
|---|---|---|
| **B0** | Plain WebView / WKWebView without Rivqen | The reference. Rivqen must not be worse. |
| **B1** | Upstream VasSonic, where it still runs safely in a controlled lab | Historical comparison. Never on production devices. |
| **B2** | Rivqen legacy mode and RQP mode | The product |

## 2. Metrics

| Metric | Definition | How to measure | Research-release target |
|---|---|---|---|
| TTFB | Time to first byte of the main HTML | Network instrumentation | Not worse than B0 on the same connection |
| First useful content | Time to meaningful visible content | Native marker + Web Performance API (FCP) | Faster than B0 on the warm path, statistically significant |
| LCP | Largest Contentful Paint, where available | `PerformanceObserver` | Not worse than B0 |
| Time to interactive | Main page usable | Custom interaction probe | No regression from the JS bridge |
| Transferred bytes | Main document + delta + overhead | Per-request counters | Diff < full HTML for suitable pages |
| Cache hit ratio | Correct hits / navigations | Cache metrics | Measured per warm / auth / offline |
| Patch success | Accepted correct deltas / deltas | Validation outcomes | 100 % correctness; failures go to fallback |
| CPU and energy | Load and energy per navigation | Android Battery Historian / Power Profiler; Xcode Energy Organizer | Not worse than B0 without a TTFCP gain |
| Peak memory | Rust + Kotlin/Swift + WebView | Platform profilers | Within the agreed budget |
| Crash-free sessions | Sessions without crash | Aggregated telemetry (host-exported) | Not worse than B0 |
| Fallback recovery | Navigations recovered by normal HTTPS | Chaos suite | 100 % when HTTPS is available |

## 3. Method

1. Fix the content, SSR route, server state, device, OS, WebView version, profile and network for each run.
2. Separate scenarios: cold, warm/304, warm/data diff, template change, offline.
3. Run at least 30 iterations per scenario in the lab before any conclusion. Use more if variance is high.
4. Report p50, p90, p95 with bootstrap 95 % confidence intervals.
5. Exclude runs only by rules written **before** the run (for example, OS backgrounding).
6. Publish raw traces and conditions with every report.
7. Count both sides of the FFI and the web runtime for memory and energy. Moving cost from Rust to the WebView is not an improvement.
8. Report the extra cost of realtime channels and prefetch (bytes and energy) separately.

## 4. First measurements

<Badge type="tip" text="FACT" /> The server demo measures transferred bytes for four visits (cold, unchanged, data changed, template changed). Results are in `evidence/benchmarks/rqp-bytes.json` and on the [Performance](/guide/performance#measured-bytes) page. Timing on devices is not measured yet.

## 5. Acceptance

<Badge type="danger" text="GATE" /> Functional correctness is a hard condition. For performance: no statistically significant regression vs B0 on the cold path, and a measurable improvement on warm and delta paths in supported scenarios. Fixed promises such as "50 % faster everywhere" are not part of the specification.

## Related

- [Performance (Guide)](/guide/performance)
- [Test strategy](/engineering/quality/testing)
- [Literature: benchmarking methodology](/research/literature#measurement-and-benchmarking)
