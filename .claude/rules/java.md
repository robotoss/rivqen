---
paths:
  - "**/*.java"
---
# Java checklist

Source: `docs/engineering/standards/java.md` (full text; change both together).

- Run: `./gradlew spotlessCheck build` (Error Prone + NullAway run in the compile).
- Packages are `@NullMarked`; NullAway errors fail the build.
- Pure decision core; the servlet filter and framework adapters only adapt.
- Records / `final` fields; builders for configuration; no static mutable state.
- `RivqenException` with a stable code and cause. On internal error, pass the original response through.
- SLF4J with allowlisted fields.
