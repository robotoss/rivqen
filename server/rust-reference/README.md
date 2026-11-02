# Rivqen reference server (oracle)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The reference server of WP-03: an independent implementation of the server side of RQP `rqp/1` and of the legacy profiles of ADR-005 (`legacy-canonical`, `legacy-java`, `legacy-node`, `legacy-php`). The conformance runner tests it by URL. Docs: `docs/engineering/server/conformance.md`; plan: `docs/engineering/plan/sprints/WP-03-S1.md`.

**State:** WP-03 S1. By URL through the runner the release binary passes the RQP suite (141/141), the legacy suite in all four profiles (59, 39, 39, 39) and the malformed suite (22/22). The image (H-115) and the site mode (H-114) come in S2.

## Command line

```text
rivqen-reference --conformance [--listen <ip>] [--port <n>] [--origin <url>] [--lab-peer]
```

| Option | Meaning |
|---|---|
| `--conformance` | Required in S1: the control endpoint of CONTRACT.md §2 (DL-086). An environment variable cannot switch it on (DL-093) |
| `--listen <ip>` | Default `127.0.0.1`. An address that is not loopback needs `--lab-peer` |
| `--port <n>` | Default `8080`; `0` = any free port. The first stdout line is `listening <port>` |
| `--origin <url>` | Default `https://shop.example.com`: `http://` or `https://` and `host[:port]`, nothing else. A case request gets the URL `<origin><request target>` |
| `--lab-peer` | Accept control requests from a peer that is not loopback (a container whose port is published on `127.0.0.1` of the host only; DL-081). `Host` and `Content-Type` are still checked. Publish the same port number on both sides (`-p 127.0.0.1:N:N`), because the `Host` check uses the local port |

Exit codes: `0` `--help`; `1` cannot listen or write stdout; `2` usage error.

## Container image

Build from the repository root (the server depends by path on `tools/rqp-markup/rust`):

```bash
docker build -f server/rust-reference/Dockerfile -t rivqen-reference:local .
```

Builder `rust:1.99.0-alpine3.22` pinned by digest, a static binary (`crt-static`); runtime `FROM scratch` (no digest exists, DL-095), user `65532:65532`, `EXPOSE 18080`, default command `--conformance --listen 0.0.0.0 --port 18080 --lab-peer`; about 1.1 MB. Publish the same port number on both sides (`-p 127.0.0.1:18080:18080`): the `Host` check uses the local port. `sh tools/reference-image/run.sh` builds the image, runs it read-only without capabilities, and runs the runner (RQP, the four legacy profiles, malformed); `IMAGE=<ref>` tests another tag. The build context allow list is `server/rust-reference/Dockerfile.dockerignore`. CI: `.github/workflows/reference-image.yml`.

## Limits

| Item | Limit | Answer |
|---|---|---|
| Request head (request line and field lines) | 64 KiB, 256 field lines | `431` (`414` in the request line) |
| Body | 8 MiB, not read when larger | `413` |
| `Transfer-Encoding` | Not supported | `501` |
| HTTP version | `1.0` and `1.1` | `505` |
| Request target | Origin-form only | `400` |
| Time to read a request, time to write an answer | 10 s each | `408` / connection closed |
| Connections | One at a time, one request per connection, `Connection: close` | — |

Logs (stderr): `<method> <path without query> <status> <ms>ms`, never a field value, a body or a query string.

## Rules

- Written from the spec pages (`docs/engineering/protocol/legacy-wire.md` §10 and §10.1, `negotiation.md`, `manifest-patch.md`, `markup.md`), never from the JavaScript models that generated the fixtures (DL-087).
- Own HTTP/1.1 reader on `std`; dependencies `sha1`, `sha2`, `base64`, `serde_json` and the Rust markup candidate `tools/rqp-markup/rust` (H-113, DL-085). Own workspace and lock file (DL-084).
- The control endpoint of `server/conformance/CONTRACT.md` §2 works only with `--conformance` (DL-086).
- Spec choices of the second implementation: legacy wire §10 notes 5–8 (DL-091), negotiation §4 open inputs 8–18 (DL-092, DL-093).

## Commands

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && RUST_TEST_THREADS=4 cargo test
cargo build --release
node ../conformance/e2e/run.mjs --command target/release/rivqen-reference --arg --conformance --arg --port --arg 0 --suite rqp
```
