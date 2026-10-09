# Observability

Rivqen records typed, privacy-safe metrics and traces so that integrators can prove the benefit and diagnose failures. Nothing leaves the device unless the host app exports it.

**Status:** <Badge type="info" text="DESIGN" />

## 1. Principles

1. **Local first.** The SDK records metrics in memory. The host app decides whether to export them.
2. **No secrets.** No URLs with query strings, no headers, no cookies, no HTML, no user IDs in metrics or logs.
3. **Typed.** Every metric has a name, a unit and a fixed set of labels.
4. **Stable.** Metric names are stable after 1.0.

## 2. Session trace

Each navigation produces one trace with these spans:

| Span | Start | End |
|---|---|---|
| `session` | `open` | session end |
| `cache.lookup` | lookup action | `CacheResult` |
| `http.request` | `SendHttpRequest` | `HttpCompleted` / `HttpFailed` |
| `webview.prepare` | prepare call | `WebViewReady` |
| `render.first` | document delivered | `PageRendered` |
| `patch.apply` | `ApplyDataPatch` | web SDK acknowledgment |
| `cache.commit` | `CommitCache` | commit done |

## 3. Metrics

| Name | Unit | Labels |
|---|---|---|
| `rivqen.session.outcome` | count | `outcome`, `protocol`, `platform` |
| `rivqen.session.ttfb` | ms | `protocol`, `network_type` |
| `rivqen.session.first_render` | ms | `outcome` |
| `rivqen.bytes.transferred` | bytes | `kind` (`full`, `delta`, `headers`) |
| `rivqen.cache.size` | bytes | `layer` |
| `rivqen.cache.evictions` | count | `layer`, `reason` |
| `rivqen.fallback` | count | `reason` (error code) |
| `rivqen.security.violation` | count | `code` |
| `rivqen.ffi.copy_bytes` | bytes | — |
| `rivqen.memory.peak` | bytes | `component` |

Labels use closed enumerations. Free-text labels are not allowed.

## 4. Export

| Platform | Export hook |
|---|---|
| Android | `RivqenConfig.metricsSink` callback; optional OpenTelemetry adapter module |
| iOS | `RivqenConfiguration.metricsSink` closure; optional OpenTelemetry adapter |
| Web | `onMetric` callback; optional `PerformanceObserver` integration |
| Servers | Standard logging + optional OpenTelemetry instrumentation |

## 5. Local diagnostics

`engine.diagnostics()` returns: SDK version, core version, ABI version, protocol modes, negotiated capabilities, active limits, cache sizes. This is for the host app owner and support. It is never sent automatically.
