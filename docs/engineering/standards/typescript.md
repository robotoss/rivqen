# TypeScript

TypeScript is used for the web SDK (`sdk/web-core`, `sdk/react`), the Node.js server SDK (`server/node`) and the demos.

**Status:** <Badge type="info" text="DESIGN" /> · <Badge type="tip" text="FACT" /> versions checked 2026-10-09; pin them in WP-12 / WP-13

[[toc]]

## 1. References

- [Google TypeScript Style Guide](https://google.github.io/styleguide/tsguide.html)
- [TypeScript Handbook](https://www.typescriptlang.org/docs/handbook/intro.html)
- [Web and React SDK](/engineering/platforms/web-react), [JS bridge contract](/engineering/platforms/js-bridge), [Node.js server SDK](/engineering/server/node)

## 2. Toolchain

| Tool | Version (2026-10-09) | License | Command |
|---|---|---|---|
| TypeScript | 7.0.2 | Apache-2.0 | `tsc --noEmit` |
| ESLint | 10.12.0 | MIT | `eslint .` |
| typescript-eslint | 8.71.1 | MIT | `strictTypeChecked` + `stylisticTypeChecked`, `projectService: true` |
| Formatter | Prettier — version pinned in WP-12 | MIT | `prettier --check .` <Badge type="warning" text="RESEARCH" /> |
| Vitest | 5.0.3 | MIT | `vitest run --coverage` |
| fast-check | Pinned in WP-12 | MIT | Property tests |
| StrykerJS | 10.0.0 | Apache-2.0 | See [Mutation testing](/engineering/quality/mutation) |

`tsconfig.json` sets these flags explicitly:

```json
{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitOverride": true,
    "noPropertyAccessFromIndexSignature": true,
    "verbatimModuleSyntax": true,
    "isolatedModules": true
  }
}
```

## 3. Design rules

1. **ES modules only.** Node.js code targets the active LTS.
2. **No `any`.** External data enters as `unknown` and is parsed into a typed value at the boundary (a schema validator is chosen in WP-12/WP-13).
3. **No non-null assertion (`!`)** and no type assertion (`as`) on external data.
4. **Errors.** Throw `Error` subclasses with a stable `code` and `cause`. Never throw strings. Every promise is awaited or handled (`no-floating-promises`).
5. **No `innerHTML`, `eval` or `new Function`** with update content. The bridge accepts messages only from the allowed origin ([JS bridge contract](/engineering/platforms/js-bridge)).
6. **Pure core, thin adapters.** The server decision function has no framework imports; Express/Fastify/Next adapters wrap it.
7. **React:** hooks only, no class components; follow the rules of hooks; server-rendered values are the default, Rivqen updates are optional.
8. **Small public API.** Export from one `index.ts`; mark internal modules as such.
9. **Logs** through one logger with allowlisted fields ([Errors and logging](/engineering/architecture/errors-logging)). No `console.log` in library code.

## 4. Tests

- Vitest for unit tests; fake timers instead of sleeps.
- Property tests with fast-check for parsers and the bridge validator.
- Playwright for browser E2E (already used for the docs site).

## Related

- [Coding standards](/engineering/standards/)
- [Examples: Server](/guide/examples/server)
