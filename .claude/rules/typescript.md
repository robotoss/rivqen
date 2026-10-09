---
paths:
  - "**/*.ts"
  - "**/*.tsx"
  - "**/*.mts"
  - "**/*.js"
  - "**/*.mjs"
---
# TypeScript / JavaScript checklist

Source: `docs/engineering/standards/typescript.md` (full text; change both together).

- Run: `tsc --noEmit`, `eslint .`, the formatter check, `vitest run` (or `npm test` in examples).
- ES modules. No `any`; external data enters as `unknown` and is parsed at the boundary.
- No `!` non-null assertion, no `as` on external data. Await or handle every promise.
- Throw `Error` subclasses with `code` and `cause`; never strings.
- No `innerHTML`, `eval`, `new Function` with update content. Bridge messages only from the allowed origin.
- Pure decision core, thin framework adapters. No `console.log` in library code; use the logger with allowlisted fields.
