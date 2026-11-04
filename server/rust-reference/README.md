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
| `--lab-peer` | Accept control requests also from a private peer: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16` (also IPv4-mapped), `fc00::/7`, `fe80::/10`; any other peer `403` (DL-081, DL-097). At start one stderr warning line. `Host` and `Content-Type` are still checked. Publish the same port number on both sides (`-p 127.0.0.1:N:N`), because the `Host` check uses the local port |

Exit codes: `0` `--help`; `1` cannot listen or write stdout; `2` usage error.

## Site mode

```text
rivqen-reference --site <dir> --protocol rqp|legacy --origin <url> [--profile <name>] [--config <file>]
                 [--listen <ip>] [--port <n>] [--workers <n>]
```

Serves the regular files of `<dir>` as the application response and applies the engine of `--protocol` (H-114, DL-096). `legacy` needs `--profile`; `rqp` takes `--config` (a `server_config` JSON file of at most 64 KiB, CONTRACT.md §2). `--origin` is required: the client checks the manifest origin against the page URL. `--workers` 1 to 64, default 8. Any listen address; there is no control endpoint (its paths are `404`). `--site` excludes `--conformance` and `--lab-peer`.

Mapping: `GET` and `HEAD` only (else `405`); the path must match `^(/[A-Za-z0-9_-][A-Za-z0-9._-]*)*/?$` (no `..`, no hidden file, no percent-encoding); a path that ends with `/` maps to `index.html`; every component must be a real directory or a regular file inside `<dir>` (symbolic links are refused); a fixed table of extensions (`.html`, `.css`, `.js`, `.json`, `.svg`, `.png`, `.jpg`, `.jpeg`, `.webp`, `.ico`, `.txt`); at most 5 MiB for `.html` and 8 MiB for other files. Everything else is `404` with a fixed text body. The file is read on every request; A has `Cache-Control: no-cache`. The log shows the path only for a file of the site.

Exit codes: `2` for a command-line rule; `1` when `<dir>` or the `--config` file cannot be used (one fixed line). Memory: each worker holds at most one file and the engine output (about 3 times the file), so 8 workers use about 200 MiB in the worst case and 64 workers about 1.5 GiB.

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
| Case body (`POST /case`) | 65 536 structural tokens, depth 16, 1 024 `app.headers` lines, only the top-level members of CONTRACT.md §2; checked before the JSON parse | `400 BAD_CASE` |
| `Transfer-Encoding` | Not supported | `501` |
| HTTP version | `1.0` and `1.1` | `505` |
| Request target | Origin-form only | `400` |
| First byte of the head | 5 s | Connection closed, no answer |
| Rest of the head after its first byte | 5 s | `408` |
| Whole request (head and body) | 10 s | `408` |
| Answer | 10 s | Connection closed |
| Connections | A queue of 64 (a full queue blocks `accept`); 1 worker in the conformance mode; one request per connection, `Connection: close` | — |

Logs (stderr): `<method> <path> <status> <ms>ms (<note>)`. The method only when it is a standard method, else `-`; the path only for `/__rivqen/conformance/info` and `/case`, else `-` (L5). Connection lines (accept failure, an I/O error kind, an idle close, a panic) at most once per second for each kind, with `(n similar lines suppressed)`. Never a field value, a body or a query string.

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
