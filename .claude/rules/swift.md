---
paths:
  - "**/*.swift"
  - "**/Package.swift"
---
# Swift checklist

Source: `docs/engineering/standards/swift.md` (full text; change both together).

- Run: `swift format lint --strict -r Sources Tests`, `swiftlint --strict`, `swift test`.
- Swift 6 language mode, complete data-race safety; no `@unchecked Sendable` without a reviewed comment.
- Engine in an `actor`; WebView work on `@MainActor`.
- No `!` force unwrap, no `try!`, no `fatalError` on input. Typed `RivqenError`.
- Thin SDK: no protocol, cache or diff logic in Swift. No global `URLProtocol`, no private API.
- New tests with Swift Testing (`@Test`, `#expect`).
