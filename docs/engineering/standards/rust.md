# Rust

Rust is used for the core (`crates/`), the FFI layer and the reference server.

**Status:** <Badge type="info" text="DESIGN" /> · <Badge type="tip" text="FACT" /> versions checked 2026-10-09; pin them in WP-05

[[toc]]

## 1. References

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Rust Style Guide](https://doc.rust-lang.org/style-guide/)
- [Errors and logging](/engineering/architecture/errors-logging), [Invariants](/engineering/architecture/invariants), [Resource limits](/engineering/core/resource-limits)

## 2. Toolchain

| Tool | Version (2026-10-09) | License | Command |
|---|---|---|---|
| Rust stable | 1.99.0 | MIT / Apache-2.0 | Pinned in `rust-toolchain.toml` |
| rustfmt, clippy | With the toolchain | MIT / Apache-2.0 | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| cargo-nextest | 0.9.148 | MIT / Apache-2.0 | `cargo nextest run --workspace` + `cargo test --doc` |
| proptest | 1.11.0 | MIT / Apache-2.0 | Property tests |
| cargo-fuzz | 0.13.2 (nightly, libFuzzer) | MIT / Apache-2.0 | `cargo +nightly fuzz run <target>` |
| Miri | Nightly component | MIT / Apache-2.0 | `cargo +nightly miri test -p <crate>` for `unsafe` logic |
| cargo-deny | 0.20.2 | MIT / Apache-2.0 | `cargo deny check` (advisories, bans, licenses, sources) |
| cargo-mutants | 27.1.0 | MIT | See [Mutation testing](/engineering/quality/mutation) |

We use `cargo-deny` for advisories, so `cargo-audit` is not needed.

## 3. Lints

Set the lints once in the workspace `Cargo.toml`:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"            # rivqen-ffi overrides to "deny" + reviewed allows
unsafe_op_in_unsafe_fn = "deny"
unused_must_use = "deny"
missing_docs = "warn"

[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
let_underscore_must_use = "deny"
let_underscore_future = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
```

Parser crates (`rivqen-template`, `rivqen-proto*`, `rivqen-diff`) add `indexing_slicing = "deny"`; size and limit code adds `arithmetic_side_effects = "deny"`. The full list and reasons are in [Errors and logging](/engineering/architecture/errors-logging).

Tests may use `unwrap`/`expect` (`#[cfg(test)]` with `#[allow(...)]` at module level).

## 4. Design rules

1. **Sans-I/O core.** Domain crates have no network, file, clock or thread calls. They take events and return actions. No async runtime in the core.
2. **Errors.** Libraries use typed error enums (`thiserror`-style) with context. No `anyhow` in library crates. Never drop an error; never `let _ = result`.
3. **No panic on input.** Untrusted input returns an error. Use checked arithmetic and bounded collections ([Resource limits](/engineering/core/resource-limits)).
4. **Newtypes** for IDs, revisions and sizes. Parse untrusted strings into them at the boundary.
5. **State machines** as `enum` + a pure `fn step(state, event) -> (state, actions)`.
6. **`unsafe`** only in `rivqen-ffi` and reviewed places. Each block has a `// SAFETY:` comment, a test, and a Miri run where possible.
7. **FFI** catches panics at the boundary (`catch_unwind`) and maps them to an error ([FFI boundary](/engineering/core/ffi)).
8. **Logging** with structured fields and redaction ([Errors and logging](/engineering/architecture/errors-logging)). No `println!`.
9. **Public API:** follow the API Guidelines naming (`as_`, `to_`, `into_`), implement common traits (`Debug`, `Clone`, `PartialEq` where it makes sense), mark enums `#[non_exhaustive]` when they can grow.
10. **Dependencies:** add to `[workspace.dependencies]` only (architect-owned). License must pass `cargo deny`.

## 5. Tests

- Unit tests next to the code (`mod tests`). Integration tests in `tests/`.
- Property tests for the invariants in [Test strategy §3](/engineering/quality/testing#_3-property-based-invariants).
- A fuzz target for each parser of untrusted input.
- Use `rivqen-testkit` fakes (clock, transport, store). No sleeps.

## Related

- [Coding standards](/engineering/standards/)
- [Crate map](/engineering/core/)
