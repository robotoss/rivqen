---
paths:
  - "**/*.kt"
  - "**/*.kts"
---
# Kotlin checklist

Source: `docs/engineering/standards/kotlin.md` (full text; change both together).

- Run: `./gradlew ktlintCheck detekt lint test`.
- Explicit API mode; KDoc on public declarations.
- Thin SDK: no protocol, cache or diff logic in Kotlin.
- Structured concurrency: engine scope, no `GlobalScope`; never swallow `CancellationException`.
- WebView calls on the main thread only; public suspend functions are main-safe.
- No `!!`; map core errors to the sealed `RivqenError`.
- Events as `Flow`/`StateFlow`; configuration immutable with a DSL builder.
