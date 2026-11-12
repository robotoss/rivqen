# ADR-001: Rust core boundaries

- Status: **Accepted** (human, 2026-11-11, H-139)
- Date: 2026-11-11
- Owner: Architect, WP-05
- Reviewers: architecture lead, security reviewer, Android SDK owner (WP-10), iOS SDK owner (WP-11)

**Status:** <Badge type="info" text="DESIGN" /> for the options and the decision. <Badge type="tip" text="FACT" /> for the WP-05 evidence.

[[toc]]

## Context

<Badge type="info" text="DESIGN" /> The core must give the same behavior on Android and iOS. A core that owns sockets needs its own cookie jar and TLS state, and these can disagree with the WebView ([Android](/engineering/platforms/android), [iOS](/engineering/platforms/ios), [ADR-003](/engineering/architecture/adr/ADR-003-ios-navigation)). The core API is an event to action machine with no hidden threads and no I/O ([core API](/engineering/core/api)); the crate map gives each crate one job ([core](/engineering/core/), rules R-CORE-01 to R-CORE-12).

## Options

1. **Pure logic core.** Events in, actions out; the adapters own HTTP, storage, timers and the WebView. Pros: one behavior on both platforms, tests on the host with fakes, no second cookie jar. Cons: the adapters execute every action; one more boundary crossing per event.
2. **The core owns HTTP.** Pros: one HTTP behavior. Cons: cookies, TLS, proxies and authentication differ from the WebView; a network stack inside the core on two platforms.
3. **Hybrid.** The core owns HTTP only for realtime channels. Pros: fewer crossings for push. Cons: two models; the problems of option 2 for that channel.

## Decision

Option 1: the Rust core is pure logic (sans-I/O); the adapters own all I/O. Option 3 stays an experiment after cookie and authentication correctness is proven (WP-16).

## Consequences

- Easier: every core test runs on a host machine with a fake clock and a fake transport; the same code gives the same result on both platforms.
- Harder: the adapters (WP-10, WP-11) execute the actions in order and report each result as an event (core API §2); the FFI (WP-09, [ADR-002](/engineering/architecture/adr/ADR-002-ffi-technology)) carries events and actions.
- Follow-up: the session envelope of events and actions (generation, request id) comes with WP-08.

## Verification

<Badge type="tip" text="FACT" /> Evidence at the WP-05 merge request (branch `wp/WP-05`):

| Check | Evidence |
|---|---|
| No platform dependency | The five crates `rivqen-model`, `rivqen-integrity`, `rivqen-proto`, `rivqen-testkit`, `rivqen-core` depend only on each other and on `sha2` and `base64` (runtime) and `serde_json`, `proptest` (tests and the test kit only), H-138. `cargo deny check` passes |
| No I/O in the core crates | No `std::net`, `std::fs`, `std::thread` or system clock in the `src/` of the four core crates; time comes in through the `Clock` trait (`rivqen-model::time`). The test kit reads fixture files in tests only |
| All tests on the host with fakes | `cargo test --workspace` runs on the Mac host and in the CI job `core` with `FakeClock` and `FakeTransport` (`rivqen-testkit`), no sleep, no thread; 330 tests at the S2 close |
| Events in, actions out | `rivqen-core` gives `EngineConfig`, `Engine::new`, `Event`, `Action` and `Outcome` (skeleton, no session logic, DL-143) |
| No `unsafe` | `#![forbid(unsafe_code)]` in every crate; `tools/rust-core/check-unsafe.sh` in CI; `rivqen-ffi` (WP-09) is the only planned exception |

Not yet verified: the full event loop on a device (WP-08 to WP-11).
