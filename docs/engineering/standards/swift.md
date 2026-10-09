# Swift

Swift is used for the iOS SDK (`sdk/ios`, Swift package `Rivqen`). The SDK is a thin adapter over the Rust core.

**Status:** <Badge type="info" text="DESIGN" /> · <Badge type="tip" text="FACT" /> versions checked 2026-10-09; pin them in WP-11

[[toc]]

## 1. References

- [Swift API Design Guidelines](https://www.swift.org/documentation/api-design-guidelines/)
- [Swift 6 migration guide](https://www.swift.org/migration/documentation/migrationguide/)
- [iOS SDK design](/engineering/platforms/ios)

## 2. Toolchain

| Tool | Version (2026-10-09) | License | Command |
|---|---|---|---|
| Swift | 6.4.0 | Apache-2.0 | `// swift-tools-version: 6.0` or later (Swift 6 language mode) |
| swift-format | Bundled with the toolchain | Apache-2.0 | `swift format lint --strict -r Sources Tests` |
| SwiftLint | 0.65.1 | MIT | `swiftlint --strict` (semantic rules; formatting is swift-format's job) |
| Swift Testing + XCTest | Bundled | Apache-2.0 | `swift test` |
| Muter | Tag 16 | MIT | Advisory only ([Mutation testing](/engineering/quality/mutation)) |

## 3. Design rules

1. **Swift 6 language mode** with complete data-race safety. No `@unchecked Sendable` without a comment and review.
2. **Actors.** The engine is owned by an `actor`. WebView work runs on `@MainActor`.
3. **Thin SDK.** No protocol, cache or diff logic in Swift.
4. **Errors.** A `RivqenError` enum; prefer typed throws (`throws(RivqenError)`) for the public API. No `try!`, no `fatalError` on input.
5. **No force unwrap (`!`)** in production code.
6. **Value types** (`struct`, `enum`) for configuration and events. Classes only for identity and lifetime.
7. **Events** as `AsyncStream`. Cancel tasks when the session closes.
8. **Naming** follows the API Design Guidelines: clarity at the point of use, argument labels that read as phrases.
9. **No global `URLProtocol`** registration and no private APIs ([Platform hardening](/engineering/security/platform-hardening)).
10. **Privacy manifest** (`PrivacyInfo.xcprivacy`) is kept up to date with each new API use.

## 4. Tests

- Swift Testing (`@Test`, `#expect`) for new tests; XCTest for UI and performance tests.
- Simulator tests in CI; device tests for release.
- Before Swift 6 mode is on in a new target, measure the work with `swift build -Xswiftc -strict-concurrency=complete`.

## Related

- [Coding standards](/engineering/standards/)
- [Getting started: iOS](/guide/getting-started/ios)
