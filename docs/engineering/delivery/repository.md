# Repository layout

Rivqen is a monorepo. One repository keeps the core, the SDKs, the servers, the fixtures and the documentation in sync. This page shows the planned layout.

**Status:** <Badge type="info" text="DESIGN" /> Today `docs/`, the root files, `.claude/`, `examples/server-demo/`, `tools/` and `evidence/` exist. Each directory is created by the work package listed next to it.

## 1. Tree

```text
rivqen/
├── README.md  LICENSE  NOTICE  THIRD_PARTY_NOTICES.md
├── SECURITY.md  CONTRIBUTING.md  CODE_OF_CONDUCT.md
├── AGENTS.md  CLAUDE.md          # rules for AI coding agents      (WP-24)
├── .claude/                      # agents, skills, rules, settings (WP-24)
├── Cargo.toml                    # Rust workspace                 (WP-05)
├── rust-toolchain.toml           # pinned stable toolchain        (WP-05)
├── deny.toml                     # cargo-deny policy              (WP-20)
├── crates/                                                        (WP-05…09)
│   ├── rivqen-core/
│   ├── rivqen-session/
│   ├── rivqen-proto-legacy/
│   ├── rivqen-proto/
│   ├── rivqen-template/
│   ├── rivqen-diff/
│   ├── rivqen-cache/
│   ├── rivqen-stream/
│   ├── rivqen-integrity/
│   ├── rivqen-ffi/
│   ├── rivqen-telemetry/
│   └── rivqen-testkit/
├── sdk/
│   ├── android/                  # Kotlin, Gradle, AAR             (WP-10)
│   ├── ios/                      # Swift package, XCFramework      (WP-11)
│   ├── web-core/                 # TypeScript                      (WP-12)
│   └── react/                    # React adapter                   (WP-12)
├── server/
│   ├── rust-reference/           # oracle server                   (WP-03)
│   ├── node/                     #                                 (WP-13)
│   ├── java/                     #                                 (WP-14)
│   ├── php/                      #                                 (WP-15)
│   └── conformance/              # runner for all servers          (WP-02)
├── fixtures/
│   ├── legacy/                   # golden fixtures, legacy mode    (WP-02)
│   ├── rqp/                      # golden fixtures, Rivqen protocol (WP-17)
│   └── malformed/                # negative and fuzz seeds         (WP-02)
├── examples/
│   ├── android-demo/  ios-demo/  react-ssr-demo/  server-demo/    (WP-22)
├── tests/
│   ├── e2e/  chaos/  security/  benchmarks/                       (WP-19)
├── evidence/
│   ├── source-manifest.lock      # pinned upstream SHA + file hashes (WP-01)
│   └── traces/                   # captured upstream behavior        (WP-01)
├── docs/                         # this site                         (WP-00)
├── tools/
│   ├── upstream-lab/             # traces of upstream servers        (WP-01)
│   └── mutation/                 # diff → mutation tool arguments    (WP-24)
├── scripts/
└── .github/workflows/
```

## 2. Rules

1. One workspace per ecosystem: Cargo workspace, Gradle build, Swift package, npm workspaces, Maven/Gradle for Java, Composer for PHP.
2. Each package has its own `README.md` with a link to its docs page.
3. Generated bindings (UniFFI) are generated in CI. They are committed only if the release process needs it (decided in WP-09).
4. Fixtures are data. They change only through a `spec` commit with protocol-owner review.
5. `evidence/` holds references, hashes and captured traces from upstream. It never holds copied upstream source code.

## 3. Naming

| Ecosystem | Package names (working) |
|---|---|
| Rust | `rivqen-*` crates (published only when the API is intentionally stable) |
| Android | `dev.rivqen:rivqen-android` (Maven group to be confirmed by `BRAND-001`) |
| iOS | Swift package `Rivqen` |
| npm | `@rivqen/web-core`, `@rivqen/react`, `@rivqen/ssr`, `@rivqen/next`, `@rivqen/server` |
| Java | `dev.rivqen:rivqen-server`, `rivqen-spring-boot-starter` |
| PHP | `rivqen/rivqen-server` |

Package names are reserved only after the brand check (`LEGAL-001`). See [Name and brand](/legal/brand).
