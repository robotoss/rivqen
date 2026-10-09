# Kotlin

Kotlin is used for the Android SDK (`sdk/android`). The SDK is a thin adapter over the Rust core.

**Status:** <Badge type="info" text="DESIGN" /> · <Badge type="tip" text="FACT" /> versions checked 2026-10-09; pin them in WP-10

[[toc]]

## 1. References

- [Kotlin coding conventions](https://kotlinlang.org/docs/coding-conventions.html) and [Kotlin API guidelines for libraries](https://kotlinlang.org/docs/api-guidelines-introduction.html)
- [Android Kotlin style guide](https://developer.android.com/kotlin/style-guide)
- [Android architecture recommendations](https://developer.android.com/topic/architecture/recommendations)
- [Android SDK design](/engineering/platforms/android)

## 2. Toolchain

| Tool | Version (2026-10-09) | License | Command |
|---|---|---|---|
| Kotlin | 2.4.21 | Apache-2.0 | Gradle Kotlin plugin |
| ktlint (via `org.jlleitschuh.gradle.ktlint` 14.2.0) | 1.8.0 | MIT | `./gradlew ktlintCheck` |
| detekt | 1.23.8 stable; 2.0.0-alpha.6 | Apache-2.0 | `./gradlew detekt` |
| Android Lint | With AGP | Apache-2.0 | `./gradlew lint` with `warningsAsErrors = true`, `abortOnError = true` |
| JUnit | 6.1.3 | EPL-2.0 | `./gradlew test` (`useJUnitPlatform()`) |
| Kotest | 6.2.5 | Apache-2.0 | Property tests |
| PIT | 1.30.0 | Apache-2.0 | See [Mutation testing](/engineering/quality/mutation) (advisory, Q-16) |

::: warning Check at WP-10 kickoff
detekt 1.23.8 is older than Kotlin 2.4. Check that it supports the Kotlin version, or use detekt 2.0 when it is stable. This is an architect decision.
:::

## 3. Design rules

1. **Explicit API mode** (`kotlin { explicitApi() }`). Every public declaration has a visibility modifier and KDoc.
2. **Thin SDK.** No protocol, cache or diff logic in Kotlin. Call the core through the generated bindings.
3. **Structured concurrency.** Use the engine's own `CoroutineScope`. No `GlobalScope`. One single-threaded dispatcher per engine for core calls (`limitedParallelism(1)`).
4. **Main safety.** Public suspend functions are safe to call from the main thread. WebView calls run on the main thread only.
5. **Events** as `Flow` / `StateFlow`. No callbacks from background threads into UI code.
6. **Errors.** Map core errors to a sealed `RivqenError` hierarchy. Never swallow `CancellationException`.
7. **No `!!`** in production code. Use explicit checks and errors.
8. **Configuration** through an immutable config object with defaults and a DSL builder.
9. **No reflection, no global singletons.** The app creates and owns the engine.
10. **Binary compatibility:** track the public API (a binary-compatibility validator is chosen in WP-10).

## 4. Tests

- JVM unit tests for all logic that does not need Android. Robolectric only when needed.
- Instrumented tests for WebView behavior on emulators; device farm for release ([Test strategy](/engineering/quality/testing)).
- Fakes for the core bindings in unit tests.

## Related

- [Coding standards](/engineering/standards/)
- [Getting started: Android](/guide/getting-started/android)
