# Java

Java is used for the Java server SDK (`server/java`): servlet filter and framework adapters.

**Status:** <Badge type="info" text="DESIGN" /> · <Badge type="tip" text="FACT" /> versions checked 2026-10-09; pin them in WP-14

[[toc]]

## 1. References

- [Google Java Style Guide](https://google.github.io/styleguide/javaguide.html)
- [Java server SDK](/engineering/server/java), [Server conformance](/engineering/server/conformance)

## 2. Toolchain

| Tool | Version (2026-10-09) | License | Command |
|---|---|---|---|
| JDK target | 17+ (bytecode `--release 17`); final range in Q-09 | GPL-2.0 + CE | Gradle toolchains |
| google-java-format via Spotless | 1.37.0 / Spotless 8.10.4 | Apache-2.0 | `./gradlew spotlessCheck` |
| Error Prone (`net.ltgt.errorprone` 5.1.1) | 2.50.0 | Apache-2.0 | Runs in `compileJava` |
| NullAway + JSpecify | 0.14.2 | MIT / Apache-2.0 | `error("NullAway")`, `NullAway:OnlyNullMarked=true`, packages `@NullMarked` |
| JUnit | 6.1.3 | EPL-2.0 | `./gradlew test` |
| PIT | 1.30.0 (+ `info.solidsoft.pitest` 1.19.0) | Apache-2.0 | See [Mutation testing](/engineering/quality/mutation) |

::: warning Check at WP-14 kickoff
Error Prone can need a newer JDK to **run** than the bytecode target. Compile with a current JDK and `--release 17`. Verify the minimum JDK for Error Prone 2.50.0 (NOT FOUND in the research).
:::

## 3. Design rules

1. **Pure core.** A framework-free class decides `HTML in → response out`. The servlet filter and the Spring adapter only adapt.
2. **Null safety.** All packages are `@NullMarked`. NullAway errors fail the build.
3. **Immutability.** Records and `final` fields for values. No setters on configuration; use a builder.
4. **Errors.** Checked exceptions only at I/O edges; a `RivqenException` with a stable `code` and a cause. Never catch `Throwable` except at the outermost filter, where the original response passes through ([Errors and logging](/engineering/architecture/errors-logging)).
5. **No streaming body buffering without a limit.** Respect the size limits.
6. **No reflection-based magic** and no static mutable state.
7. **Logging** through SLF4J with allowlisted fields.

## 4. Tests

- JUnit 6 unit tests for the core; servlet tests with an embedded container.
- The cross-language conformance suite runs against the Java server ([Conformance](/engineering/server/conformance)).

## Related

- [Coding standards](/engineering/standards/)
- [Getting started: Java](/guide/getting-started/server-java)
