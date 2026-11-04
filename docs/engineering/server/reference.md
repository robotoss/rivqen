# Reference server (oracle)

`server/rust-reference` is the Rivqen reference server of WP-03: an independent implementation of the server side of RQP `rqp/1` and of the four legacy profiles of ADR-005. The conformance runner tests it by URL; it is the oracle of [gate G1](/engineering/plan/gates) and the server for client tests. It is a test and development tool, not a production server.

**Status:** <Badge type="tip" text="FACT" /> WP-03 [S1](/engineering/plan/sprints/WP-03-S1) and [S2](/engineering/plan/sprints/WP-03-S2). Command line and limits: `server/rust-reference/README.md`.

[[toc]]

## 1. What it passes

<Badge type="tip" text="FACT" /> By URL through `server/conformance` (`tools/fulltest.sh rust`, and in the container `tools/reference-image/run.sh`), 2026-11-03:

| Suite | Result |
|---|---|
| RQP server cases `FX-RQ-P*` | 141/141, no SHOULD failure |
| `legacy-canonical` | 59/59 |
| `legacy-java`, `legacy-node`, `legacy-php` | 39/39 each |
| Malformed legacy cases | 22/22 |

The JavaScript models generated the fixtures; the reference server was written from the spec pages only (DL-087), so a fixture that both pass is confirmed by two independent implementations. The 29 RQP cases of WP-03 S2 (`P-44` to `P-53`) passed the reference server on its first run.

## 2. Design

| Part | Choice | Decision |
|---|---|---|
| HTTP | Own HTTP/1.1 reader and writer on `std`; exact request names and response field lines; one request per connection | H-113 |
| Dependencies | `sha1`, `sha2`, `base64`, `serde_json`, and the Rust markup candidate `tools/rqp-markup/rust` by path (its token API `rqp_markup::scan`) | H-113, DL-085 |
| Crate | Own workspace and lock file; never depends on `crates/*` | DL-084 |
| Legacy engine | Legacy wire §10 (C-01…C-16) and §10.1 (P-01…P-11); forward scans, linear time | DL-088, DL-091 |
| RQP engine | Negotiation §4 with the open inputs 1–18; manifest, revisions and delta of manifest and patch | DL-089, DL-092, DL-093 |
| Modes | `--conformance` (the control endpoint of CONTRACT.md §2) and `--site <dir>` (static files with legacy or RQP); no proxy | H-114, DL-086, DL-096 |
| Image | Builder pinned by digest, static musl binary, `FROM scratch`, user `65532`, read-only, no capabilities | H-115, DL-095 |

## 3. Security boundaries

- **Control endpoint:** off without `--conformance`; loopback peer, `Host` and `Content-Type` checks; with `--lab-peer` only private peers and a per-run token (`Rivqen-Conformance-Token`, H-117); a case body is checked before the JSON parse (DL-097).
- **Site mode:** a path pattern without dots or percent-encoding, no symbolic links, a canonical path check, `O_NONBLOCK` and, on Linux, a `/proc/self/fd` check after `open`; `421` for another `Host` (DL-096, DL-098). Accepted residuals: a local user with write access to the site directory on a Unix other than Linux (a race between the walk and `open`), hard links inside the directory, no per-peer connection cap.
- **Robustness:** no panic on input (crate lints, `catch_unwind` per request); workers with per-phase deadlines; logs without field values, bodies, query strings, the token or unmapped paths; repeated lines rate-limited.

Two security reviews (S1 and S2) found no critical or high finding; their results are in the sprint records.

## Related

- [Conformance](/engineering/server/conformance)
- [Legacy wire §10](/engineering/protocol/legacy-wire#canonical-server)
- [Negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server)
