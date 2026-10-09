# Dependencies

This page lists candidate dependencies for the Rust core with their license and role. No dependency is approved until WP-05 adds it with a benchmark or a clear need. Fewer dependencies is better.

**Status:** <Badge type="tip" text="FACT" /> versions and licenses from crates.io on 9 October 2026. <Badge type="info" text="DESIGN" /> recommendations.

[[toc]]

## 1. Rust crates

| Crate | Version | License (SPDX) | Role | Recommendation |
|---|---|---|---|---|
| `uniffi` | 0.32.2 | **MPL-2.0** | Kotlin/Swift bindings | Use (see license note) |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | Serialization | Use |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | JSON | Use |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | Error types | Use |
| `sha2` | 0.11.0 | MIT OR Apache-2.0 | SHA-256 | Use |
| `sha1` (RustCrypto) | 0.11.0 | MIT OR Apache-2.0 | SHA-1 for legacy compatibility only | Use |
| `bytes` | 1.12.1 | MIT | Ref-counted buffers | Use if FFI design benefits |
| `rusqlite` (+ bundled SQLite) | 0.40.2 | MIT (SQLite: public domain) | Cache metadata | Use (ADR-004) |
| `tracing` | 0.1.44 | MIT | Instrumentation | Use, behind a feature |
| `zeroize` | 1.9.1 | Apache-2.0 OR MIT | Key buffers | Use |
| `aes-gcm` / `chacha20poly1305` | 0.11.x | Apache-2.0 OR MIT | AEAD | Candidates for ADR-011 |
| `ring` | 0.17.14 | Apache-2.0 AND ISC | Crypto | Candidate for ADR-011 |
| `aws-lc-rs` | 1.18.1 | ISC AND (Apache-2.0 OR ISC) | Crypto (FIPS option) | Candidate; larger build |
| `sfv` | 0.16.0 | MIT / Apache-2.0 | Structured Field Values (RQP headers) | Candidate for WP-17 |
| `moka` | 0.12.16 | (MIT OR Apache-2.0) AND Apache-2.0 | Concurrent memory cache | Compare with simple LRU |
| `smallvec` | 1.16.2 | MIT OR Apache-2.0 | Small vectors | Only if profiled |
| `bumpalo` | 3.20.3 | MIT OR Apache-2.0 | Arenas | Only if profiled |
| `memmap2` | 0.9.11 | MIT OR Apache-2.0 | Memory-mapped reads | Not in MVP |
| `blake3` | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | Fast hash | Not needed (SHA-256 for interop) |
| `tokio` | 1.53.2 | MIT | Async runtime | **Avoid in core** (adapters own the executor) |
| `lol_html` | 3.0.1 | BSD-3-Clause | Streaming HTML rewriter | Candidate for RQP regions only |
| `html5ever` | 0.40.1 | MIT OR Apache-2.0 | HTML parser | Not needed for legacy (textual grammar) |

Dev and test only:

| Crate | Version | License | Role |
|---|---|---|---|
| `proptest` | 1.11.0 | MIT OR Apache-2.0 | Property tests |
| `criterion` | 0.8.2 | Apache-2.0 OR MIT | Benchmarks |
| `libfuzzer-sys` (via `cargo-fuzz`) | 0.4.13 | (MIT OR Apache-2.0) AND NCSA | Fuzzing |

## 2. License note: UniFFI is MPL-2.0

<Badge type="tip" text="FACT" /> The `uniffi` crate is licensed under the Mozilla Public License 2.0 ([repository](https://github.com/mozilla/uniffi-rs)). MPL-2.0 is a **file-level** copyleft license.

What this means for Rivqen (general information, not legal advice):

1. Rivqen can depend on UniFFI and distribute binaries that include it.
2. If Rivqen **modifies** MPL-licensed files of UniFFI, those files must stay under MPL-2.0 and their source must be available.
3. Rivqen's own files keep Apache-2.0.
4. UniFFI also generates Kotlin and Swift binding code. The license status of generated code must be confirmed in WP-09 (check UniFFI's documentation and templates). Record the result in `THIRD_PARTY_NOTICES.md`.
5. The `cargo-deny` policy explicitly allows MPL-2.0 **only** for `uniffi*` crates.

## 3. Policy

| Rule | Detail |
|---|---|
| Allowed licenses | MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, CC0-1.0, 0BSD, NCSA |
| Allowed by exception | MPL-2.0 (listed crates only) |
| Not allowed | GPL, LGPL, AGPL, SSPL, BUSL, "non-commercial" licenses |
| Enforcement | `cargo deny check licenses` in CI |
| Size | Report the binary size impact of each new dependency |
| Maintenance | Prefer crates with recent releases and a security policy |

## Related

- [Third-party dependencies](/legal/third-party)
- [Technology stack](/engineering/architecture/technology-stack)
