---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---
# Rust checklist

Source: `docs/engineering/standards/rust.md` (full text; change both together).

- Run: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run`, `cargo test --doc`.
- Core crates are sans-I/O: no network, file, clock, threads or async runtime.
- No `unwrap`/`expect`/`panic!`/`todo!` outside tests. No `let _ = fallible()`. Propagate typed errors with context.
- Untrusted input never panics: checked indexing and arithmetic, bounded sizes.
- `unsafe` only in reviewed places, each with `// SAFETY:` and a test.
- Newtypes for IDs and revisions; `#[non_exhaustive]` on public enums that can grow.
- New dependencies go to `[workspace.dependencies]` through the architect; `cargo deny check` must pass.
- Mutation: `cargo mutants --in-diff <diff>`; critical crates ≥ 80 % killed.
