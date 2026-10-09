# Error handling and logging

This page defines how every Rivqen component handles errors and writes logs. The goal has two parts that must both hold:

1. **Nothing is lost.** No error is ignored. Every failure is visible, explainable and debuggable.
2. **Nothing leaks, nothing breaks.** Logs never contain secrets or page content, and a release build never crashes the host app on a recoverable error.

**Status:** <Badge type="info" text="DESIGN" /> Decision record: [ADR-017](/engineering/architecture/adr/#adr-017). Applies to Rust, Kotlin, Swift, TypeScript, Java, PHP and all tools.

[[toc]]

## 1. Rules for all languages

| ID | Rule |
|---|---|
| `ERR-01` | **Never ignore an error.** Every error is handled, returned to the caller, or converted to a typed error with context. An empty `catch`, a discarded `Result`, or `try?` without handling is a defect. |
| `ERR-02` | **Handle once.** The code that decides what to do (fallback, retry, report) logs the error. Lower layers add context and return it; they do not log the same error again. |
| `ERR-03` | **Typed errors with stable codes.** Every error has a category and a code from the [error catalog](/engineering/core/errors). Codes never change their meaning. |
| `ERR-04` | **Context chain.** An error keeps its cause chain (`source` / `cause`) and adds safe context: operation, state, session id, generation, request id, byte counts, durations. |
| `ERR-05` | **Correlation.** Every log line and error carries `session_id` and `request_id` (and the server `trace_id` when present), so one navigation can be followed across Rust, Kotlin/Swift, the page and the server. |
| `ERR-06` | **Safe by construction.** Sensitive values use wrapper types that cannot be printed (see section 4). |
| `ERR-07` | **Fail safe.** A recoverable error in a release build leads to the documented fallback (normal HTTPS load), never to a crash, a white screen or an endless wait. |
| `ERR-08` | **Panic = bug, not control flow.** Panics and crashes are reserved for broken invariants. In release builds they are caught at the boundary, reported as `INTERNAL_ERROR`, and the session falls back. |
| `ERR-09` | **Rate limits.** A repeated error is logged once per window with a counter, so logs cannot flood the device or the server. |
| `ERR-10` | **Testable.** Error paths have tests: each error code has at least one test that triggers it and checks the fallback and the log fields. |

## 2. Rust core

### 2.1 Error types

```rust
// Design sketch
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RivqenError {
    #[error("protocol error {code}: {context}")]
    Protocol { code: ErrorCode, context: Context, #[source] source: Option<BoxError> },
    #[error("cache error {code}: {context}")]
    Cache { code: ErrorCode, context: Context, #[source] source: Option<BoxError> },
    // … one variant per category in the error catalog
}

pub struct Context {           // only safe, structured fields
    pub op: &'static str,      // "split_template", "commit_cache", …
    pub session: SessionId,
    pub generation: u64,
    pub request: Option<RequestId>,
    pub bytes: Option<u64>,
    pub state: Option<&'static str>,
}
```

### 2.2 Lints that enforce "no ignored errors"

Set in the workspace `Cargo.toml` (`[workspace.lints]`) and checked in CI with `-D warnings`:

| Lint | Level | Effect |
|---|---|---|
| `unused_must_use` | deny | A `Result` cannot be dropped silently |
| `clippy::let_underscore_must_use`, `clippy::let_underscore_future` | deny | `let _ = fallible()` is not allowed |
| `clippy::unwrap_used`, `clippy::expect_used` | deny (non-test code) | No hidden panics on errors |
| `clippy::panic`, `clippy::todo`, `clippy::unimplemented` | deny (non-test code) | No placeholder panics in shipped code |
| `clippy::indexing_slicing` | warn → deny in parsers | Use checked access in code that reads untrusted input |
| `clippy::arithmetic_side_effects` | deny in size and limit code | No silent overflow in limits |
| `unsafe_code` | forbid (except `rivqen-ffi`) | See [FFI boundary](/engineering/core/ffi) |

Exceptions use `#[expect(lint, reason = "…")]` with a written reason. A reviewer must approve each exception.

### 2.3 Panics at the FFI boundary

1. The release profile keeps `panic = "unwind"` so that the boundary can catch a panic.
2. Every exported function runs inside `std::panic::catch_unwind`. A caught panic becomes `INTERNAL_ERROR` with the panic location (file and line), never the panic message if it could contain data.
3. The panic hook records the location and a backtrace in debug builds; in release builds it records only the location and the build id.

### 2.4 Logging in Rust

| Item | Design |
|---|---|
| API | `tracing` spans and events, behind a feature; zero cost when disabled |
| Output | The core does not print. It forwards structured events to the host through the FFI log sink. |
| Fields | `code`, `category`, `op`, `session_id`, `request_id`, `generation`, `state`, sizes, durations |
| Never logged | URLs with query strings, headers, cookies, tokens, HTML, block content, user ids |
| URL form | Origin + path template only, for example `https://shop.example.com/catalog` (no query, no fragment) |

## 3. Platforms and servers

| Component | Errors | Logging | Release behavior |
|---|---|---|---|
| **Kotlin (Android)** | Sealed `RivqenException` hierarchy mirrors Rust codes. No empty `catch`; `runCatching` results are always inspected. Coroutines use a `CoroutineExceptionHandler` that maps to codes. | Host-provided `RivqenLogger` sink; default sink writes to Logcat **only in debuggable builds**. | Caught at the SDK boundary; session falls back; app never crashes from SDK errors. |
| **Swift (iOS)** | `RivqenError: Error` enum with codes; `throws`/`async throws`; no `try?` that drops errors; `Result` inspected. | `os.Logger` with privacy annotations: dynamic values `privacy: .private` unless proven safe; host sink available. | Same fallback rule. No `fatalError` in recoverable paths. |
| **TypeScript (web)** | `RivqenError` class with `code`, `cause`. Promise rejections always handled; global handlers report unhandled ones. React error boundary around SDK regions. | `onLog` callback; console output only in development builds. | A bridge or patch error reloads the page from the network; it never leaves a half-updated DOM. |
| **Node.js server** | Errors carry `code` and `cause`; middleware never swallows errors from `render`. | Structured JSON logs (pino-compatible fields) with `request_id`; no bodies, cookies or auth headers. | On SDK error the middleware sends the original HTML unchanged (safe passthrough) and logs `SERVER_PASSTHROUGH`. |
| **Java server** | Checked domain exceptions wrapping causes; no `catch (Exception e) {}`. | SLF4J with MDC `request_id`; redaction filter. | Passthrough of the original response on SDK error. |
| **PHP server** | Exceptions with codes; `JSON_THROW_ON_ERROR`; no `@` error suppression. | PSR-3 logger; redaction processor. | Passthrough of the original response on SDK error. |

## 4. Secret-safe types

| Type | Wraps | `Debug` / `toString` / `description` output |
|---|---|---|
| `Secret<T>` (Rust), `Secret` (Kotlin/Swift/TS) | Tokens, cookies, keys | `Secret(<redacted>)` |
| `SafeUrl` | URL | `origin + path` only |
| `PageBytes` | HTML / block content | `PageBytes(len=12345, sha256=…8 hex)` |
| `PartitionId` | Keyed hash | First 8 hex characters |

CI includes a test that formats every public type with `{:?}` / `toString()` and fails if a known secret marker appears.

## 5. Debug vs release

| Aspect | Debug / development build | Release build |
|---|---|---|
| Log level default | `debug` | `warn` (host can raise to `info`) |
| Backtraces | Full | Location + build id; symbols resolved offline with private symbol files |
| Content in logs | Never (same rules) | Never |
| Assertion failures | Panic / crash to find bugs early | Caught; `INTERNAL_ERROR`; fallback |
| Diagnostics API | Full `diagnostics()` | Full `diagnostics()` (local only, never sent automatically) |
| Extra checks | Invariant checks after every state change | Invariant checks at boundaries only (measured cost) |

## 6. Error report format

Every error that reaches the host is delivered as one structured event:

```json
{
  "event": "rivqen.error",
  "code": "PATCH_BASE_MISMATCH",
  "category": "Protocol",
  "fallback": "NormalLoad",
  "session_id": "s-7f3a…",
  "request_id": 18,
  "generation": 4,
  "op": "apply_patch",
  "url": "https://shop.example.com/catalog",
  "detail": { "expected_base": "rev-041", "received_base": "rev-039" },
  "chain": ["ProtocolError: base mismatch", "caused by: stale patch on channel ws-2"],
  "build": "rivqen-core 0.3.0 (abc1234)"
}
```

`detail` holds only fields from an allowlist per error code. Revisions, sizes and states are allowed; content is not.

## 7. Verification

| Check | How |
|---|---|
| No ignored errors | Lints in CI (Rust, ktlint/detekt rules, SwiftLint `force_try`/`force_unwrapping`, ESLint `no-empty`, `@typescript-eslint/no-floating-promises`, PHPStan strict rules, Error Prone for Java) |
| No secrets in logs | Log-scanning test with planted marker values in cookies, headers and HTML |
| Fallback on every error | Chaos and fault-injection suites assert a normal load for each code |
| No crash in release | Fuzz and fault tests run against release-profile builds |

## Related

- [Errors catalog](/engineering/core/errors)
- [Observability](/engineering/quality/observability)
- [Security control SEC-08](/engineering/security/controls)
