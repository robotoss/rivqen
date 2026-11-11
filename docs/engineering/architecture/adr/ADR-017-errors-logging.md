# ADR-017: Error handling and logging

- Status: **Proposed** (to the human with the WP-05 merge request, DL-149)
- Date: 2026-11-11
- Owner: Architect, WP-05
- Reviewers: architecture lead, security reviewer

**Status:** <Badge type="info" text="DESIGN" /> for the options and the decision. <Badge type="tip" text="FACT" /> for the WP-05 evidence.

[[toc]]

## Context

<Badge type="info" text="DESIGN" /> The core reads untrusted input (HTML, header fields, JSON, patches) and runs inside apps with user data. An ignored error can show a wrong page; a crash in a release build breaks the app; a log line with a cookie, a token or page content leaks user data. Every failure must end in a typed decision: a fallback to a normal HTTPS load or a stop ([errors](/engineering/core/errors), [error handling and logging](/engineering/architecture/errors-logging)).

## Options

1. **Typed errors with stable codes and redacted output.** Each module has its own error type; each maps to one stable code of the catalog with a category and a fallback hint; `Display` and `Debug` print no secret and no content; lints forbid the panic paths. Pros: every failure has a code and a decision; logs are safe by construction. Cons: more types; tests for each text.
2. **Strings and a generic error type.** Pros: less code. Cons: no stable code for the adapters; content can enter messages.
3. **Panics with a catch at the FFI boundary.** Pros: short code. Cons: an abort cannot be caught (for example a stack overflow on drop, DL-152 S-1); state can stay half changed.

## Decision

Option 1: no ignored error; every error is typed, carries a stable code with a category and a fallback hint, and goes to a place that decides; output of errors and values is redacted; release builds never crash on a recoverable error.

## Consequences

- Easier: the adapters map a code to one behavior ([errors](/engineering/core/errors) §2); logs and metrics hold codes, not data.
- Harder: each new type with data needs a hand-written `Debug` and a marker test; a source chain can still hold data, so a log sink must not walk `Error::source` (WP-08).
- Follow-up: the structured log sink and the URL form in logs (WP-08, DL-152 S-4); `RivqenError` is large, so the context moves into a box before WP-08 (S2 §5).

## Verification

<Badge type="tip" text="FACT" /> Evidence at the WP-05 merge request (branch `wp/WP-05`):

| Check | Evidence |
|---|---|
| Catalog in code | `rivqen-model::error`: `ErrorCode` (18 codes with `as_str`, `category`, `fallback`), `Category`, `FallbackHint`, trait `HasErrorCode`; every error type of `rivqen-proto` and `rivqen-core` implements it |
| Context without data | `RivqenError` with a safe `Context` (operation name, no value); `Engine::new` names the limit, never the value (DL-155) |
| Panic paths forbidden | Workspace lints deny `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `indexing_slicing`, `arithmetic_side_effects`, `let_underscore_must_use`, `print_stdout`, `print_stderr`; `unused_must_use` is an error |
| Redaction | Hand-written `Debug` for `FieldLine`, `Nonce`, `PatchOp`, digests and identifiers (8-character prefix), JSON values and events, `ResponseHead` errors; marker scan tests plant secrets and check that no `Debug` or `Display` shows them (`model_debug.rs`, `core_types.rs` and the text tests of `rivqen-proto`) |
| No crash on untrusted input | Property tests for every parser (256 cases or more); fuzz targets for the field parser, the JSON reader, the manifest and the envelope (DL-154) |

Not yet verified: the log sink and the release fallback path on a device (WP-08 to WP-11).
