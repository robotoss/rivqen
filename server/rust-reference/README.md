# Rivqen reference server (oracle)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The reference server of WP-03: an independent implementation of the server side of RQP `rqp/1` and of the legacy profiles of ADR-005 (`legacy-canonical`, `legacy-java`, `legacy-node`, `legacy-php`). The conformance runner tests it by URL. Docs: `docs/engineering/server/conformance.md`; plan: `docs/engineering/plan/sprints/WP-03-S1.md`.

**State:** WP-03 S1 in progress. The contract types and the stubs exist; the lanes fill them in.

## Rules

- Written from the spec pages (`docs/engineering/protocol/legacy-wire.md` §10 and §10.1, `negotiation.md`, `manifest-patch.md`, `markup.md`), never from the JavaScript models that generated the fixtures (DL-087).
- Own HTTP/1.1 reader on `std`; dependencies `sha1`, `sha2`, `base64`, `serde_json` and the Rust markup candidate `tools/rqp-markup/rust` (H-113, DL-085). Own workspace and lock file (DL-084).
- The control endpoint of `server/conformance/CONTRACT.md` §2 works only with `--conformance` (DL-086).

## Commands

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && RUST_TEST_THREADS=4 cargo test
cargo build --release
node ../conformance/e2e/run.mjs --command target/release/rivqen-reference --arg --conformance --arg --port --arg 0 --suite rqp
```
