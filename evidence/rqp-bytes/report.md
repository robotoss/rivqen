# RQP byte study (WP-17 S6 T-01)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence for WP-17 S6, task T-01 (sprint record `docs/engineering/plan/sprints/WP-17-S6.md`, human decision H-101). Measured on 2026-10-20 on base `2a50d4c`. Tools: `tools/rqp-protocol/study/bytes/` (README with the commands). Machine results: `tools/rqp-protocol/study/bytes/results/bytes.json` and `results/legacy-lab.json`. The tables are the output of `node tools/rqp-protocol/study/bytes/run.mjs --tables tools/rqp-protocol/study/bytes/results/bytes.json`. The study lane could not write this file; the architect wrote it from the lane's report text.

**Marks.** FACT: measured, or read in the cited source on that date. DESIGN: a proposal or an argument. RESEARCH: an inference. NOT FOUND: not verified. No rule changed in this study.

## 0. Summary

1. FACT. Like for like (the same demo page; legacy markers in place of `data-rq-block`; the title as the implicit legacy title block): cold visit RQP 2,707 gzip, legacy 2,232 (+475); data change RQP 526, legacy 316 (+210). The earlier legacy value for the data change (294) is the value of a broken legacy update (§3.1, F-1).
2. FACT. The cold-visit difference is the manifest: +485 gzip. The RQP block markup is 10 gzip smaller than the legacy markers. Most of the manifest cost is the block list (225 gzip), of which the digests are 178. With 64 small blocks the digests are 2,360 of 3,651 gzip.
3. FACT. On the data change, the per-operation hashes cost 42 to 50 gzip each: 159 gzip of the demo delta, 2,292 of 3,053 gzip with 64 changed small blocks. The three revisions in the envelope cost about 41 to 47 gzip each.
4. DESIGN. The client already computes every block digest and the template revision (H-44, H-48). The page revision binds them (H-55 preimage), and `next_revision` binds every operation hash. So these fields add no integrity under SHA-256 collision resistance; removing them loses only diagnostic precision. **Corrected in §9:** this holds for the block digests, the operation hashes and the template revision, not for `base_revision` (D2).
5. FACT. Proposal P1 (§6) removes the redundant fields and keeps the page revision, `next_revision`, `origin` and `path`, with `policy` and `limits` only when not default. Demo: cold 2,357 gzip (legacy 2,232); change 220 gzip (legacy 316); whole visit with HPACK headers: cold 2,591 vs 2,484, change 572 vs 642. With 64 small blocks P1 is at or below legacy in both visits.
6. FACT. Binary forms gain little after compression: the `t1.`/`r1.` prefixes cost 4 to 6 gzip; CBOR is 8 to 24 gzip better than the equal JSON; content outside JSON is within about 20 gzip of the JSON array form; random digest bytes cost about the same in base64url and in binary after gzip.

## 1. Question

H-101: "split the bytes (manifest, digests, revisions, delta envelope) and propose reductions with their security cost, before the wire format is released." The H-82 comparison (`evidence/benchmarks/rqp-bytes-2026-10-19.json`) was not like for like: the two pages had different markup.

## 2. Method

- **Pages.** Rivqen-written only (no corpus page). The demo catalog page of `examples/server-demo` at data steps 0 and 1 (4 blocks: `title`, `price`, `stock`, `cart`; all 4 change; `policy` and `limits` with default values). Synthetic pages: 48 static template lines plus N `div` blocks, N = 1, 4, 16, 64; small (20 B) or large (2,048 B, an HTML list with attribute quotes) content; version B changes block 0 only ("one") or all blocks ("all"); no `policy`, no `limits`.
- **Baseline.** FACT. F, `304` and D come from the reference server step (`tools/rqp-protocol/src/server.mjs` `handle()`); a test checks that the demo values equal the 2026-10-19 evidence.
- **Sizes.** Raw, gzip and Brotli with the Node.js zlib defaults (the method of `measure.js`). Node.js 26.8.2, zlib 1.2.12, Brotli 1.2.0, nghttp2 1.70.0.
- **Raw breakdown.** Exact and additive: each byte of the manifest element or of D is in one row (the code throws otherwise; tested on a hand-computed page, 2,000 random JSON values and all scenarios).
- **Marginal cost.** size(F or D) minus size without the component; not additive. Brotli marginals of components under about 50 raw bytes are unstable (can be negative); rank small components by gzip.
- **Header fields.** HTTP/1.1 bytes and the HPACK header block bytes that Node.js writes on a new HTTP/2 connection (a local proxy counts HEADERS and CONTINUATION fragments). A warm connection makes constant fields about 1 byte.
- **Legacy.** FACT. The unmodified upstream Node.js server (`lab-node`: `sonic-nodejs/middleware/compress.js` at `59936bef`, npm `sonic_differ` 1.0.7) in Docker on 127.0.0.1; requests `accept-diff: true`, `Accept-Encoding: gzip`, then `If-None-Match` and `template-tag` (legacy-wire §2, §3). Each legacy page is the RQP page with each block element (without `data-rq-block`) between `<!--sonicdiff-ID-->` and `<!--sonicdiff-ID-end-->`; the first `title` element has no markers (legacy-markers §2). The clone, the container and the generated pages were deleted after the run.
- **Limits.** One HTML generator and the demo page; zlib defaults only (server settings NOT FOUND); zstd, QPACK, CDNs, devices: not measured (WP-13, WP-19); real distributions of block counts and sizes: NOT FOUND.

## 3. Results

### 3.1 Demo page, RQP and legacy

FACT (raw / gzip / Brotli).

| Visit | RQP | Legacy |
|---|---:|---:|
| Cold | 20,250 / 2,707 / 1,686 | 19,593 / 2,232 / 1,252 |
| Unchanged | `304`, 0 | `304`, 0 |
| Data change | 836 / 526 / 467 | 497 / 316 / 260 |

**F-1. The H-82 conversion breaks the legacy title.** FACT. `measure.js` puts markers around the `<title>`. With these markers the lab gives exactly the 2026-10-19 legacy values (cold 19,641 / 2,244 / 1,248; change 512 / 294 / 253), and the data body has `"{title}":"<!--sonicdiff-title-->{title}<!--sonicdiff-title-end-->"`: the new title text is missing. Cause: `sonic_differ` first replaces the first title element with `{title}` (npm `sonic_differ@1.0.7/index.js:40-43`), then the marker rule (:51-60) overwrites the `{title}` data entry (:82, :89-91). DESIGN: the rebuilt page cannot match `html-sha1`, so a canonical legacy client discards the update and loads the full page. The like-for-like data-change value is 316 gzip.

### 3.2 Full document F: the manifest

Demo manifest (FACT; marginal values on F):

| Component | Raw | Marginal gzip / br |
|---|---:|---:|
| Whole manifest element | 712 | 485 / 470 |
| Script element wrapper | 57 | 20 / 52 |
| JSON structure | 60 | — |
| Member names | 207 | 60 / 66 (vs short names) |
| `protocol` value | 7 | 11 / 38 (member) |
| `origin` and `path` values | 33 | 31 / 101 (members) |
| `template_revision` (prefix and quotes / digest) | 5 / 43 | 52 / 102 (member) |
| `page_revision` (prefix and quotes / digest) | 5 / 43 | 49 / 72 (member) |
| Block ids and formats | 51 | 34 / 47 (members) |
| Block digests | 180 | 178 / 151 (members) |
| `policy` and `limits` values | 21 | 66 / 68 (members) |
| Whole `blocks` member | — | 225 / 229 |

Cold visit across the grid (FACT; legacy is the lab value for the same page):

| Page | F gzip / br | Manifest gzip / br | Digests gzip / br | Legacy gzip / br |
|---|---:|---:|---:|---:|
| demo | 2,707 / 1,686 | 485 / 470 | 178 / 151 | 2,232 / 1,252 |
| 1 small | 677 / 498 | 278 / 256 | 49 / 65 | 409 / 250 |
| 1 large | 1,091 / 754 | 290 / 253 | 50 / 59 | 808 / 511 |
| 4 small | 834 / 618 | 404 / 359 | 161 / 158 | 445 / 283 |
| 4 large | 2,257 / 1,488 | 453 / 399 | 181 / 184 | 1,818 / 1,124 |
| 16 small | 1,409 / 1,139 | 885 / 818 | 607 / 597 | 575 / 343 |
| 16 large | 6,658 / 4,367 | 1,080 / 836 | 737 / 622 | 5,628 / 3,582 |
| 64 small | 3,651 / 3,006 | 2,783 / 2,505 | 2,360 / 2,200 | 1,048 / 597 |
| 64 large | 23,527 / 16,223 | 3,557 / 2,634 | 3,031 / 2,258 | 20,178 / 13,758 |

FACT. Raw manifest with 64 blocks: 5,652 B (digests 2,880, member names 1,346, formats 384, ids 320, JSON structure 526). A digest is 43 random base64url characters, about 33 gzip bytes (about 6 bits per character, the entropy of 32 bytes).

### 3.3 Delta D

Demo delta, 4 operations (FACT; marginal values on D):

| Component | Raw | Marginal gzip / br |
|---|---:|---:|
| Whole envelope | 836 | 526 / 467 |
| JSON structure | 62 | — |
| Member names | 233 | 54 / 24 (vs short names) |
| `type` and `protocol` values | 21 | 27 / 26 (members) |
| `base_revision` | 48 | 41 / 48 (member) |
| `next_revision` | 48 | 43 / 45 (member) |
| `template_revision` | 48 | 47 / 47 (member) |
| `op` and `format` values | 84 | 33 / 25 (members) |
| `block_id` values | 27 | — |
| Content: quotes / bytes / escapes | 8 / 71 / 6 | — |
| Operation hashes (4) | 180 | 159 / 150 (members) |

Data change across the grid (FACT):

| Page | RQP response | RQP gzip / br | Op hashes gzip / br | Legacy gzip / br |
|---|---|---:|---:|---:|
| demo | D, 4 op | 526 / 467 | 159 / 150 | 316 / 260 |
| 1 small | D, 1 op | 330 / 294 | 43 / 28 | 218 / 193 |
| 1 large | D, 1 op | 754 / 591 | 49 / 48 | 617 / 464 |
| 4 small one | D, 1 op | 333 / 298 | 42 / 34 | 262 / 224 |
| 4 small all | D, 4 op | 478 / 429 | 154 / 140 | 263 / 228 |
| 4 large one | D, 1 op | 753 / 590 | 48 / 46 | 1,620 / 1,087 |
| 4 large all | D, 4 op | 1,882 / 1,313 | 178 / 163 | 1,613 / 1,091 |
| 16 small one | D, 1 op | 330 / 296 | 42 / 26 | 409 / 311 |
| 16 small all | D, 16 op | 994 / 910 | 579 / 568 | 407 / 322 |
| 16 large one | D, 1 op | 754 / 602 | 49 / 58 | 5,418 / 3,556 |
| 16 large all | D, 16 op | 6,163 / 4,229 | 748 / 611 | 5,391 / 3,591 |
| 64 small one | D, 1 op | 332 / 295 | 42 / 27 | 967 / 613 |
| 64 small all | D, 64 op | 3,053 / 2,772 | 2,292 / 2,231 | 965 / 619 |
| 64 large one | D, 1 op | 752 / 598 | 50 / 56 | 20,153 / 13,635 |
| 64 large all | F (D not smaller) | 23,613 / 16,333 | — | 20,252 / 13,742 |

FACT. When one block of many changes, RQP sends much less than legacy, because legacy sends every block (64 large: 752 vs 20,153 gzip). When every block changes, RQP sends more; the difference is mostly the operation hashes. JSON escaping: 1,824 raw bytes on 32,768 content bytes (16 large, all changed); compressed, at most about 90 bytes.

### 3.4 Block markup

FACT. Marginal cost against the same page without block markup (raw / gzip / br):

| Page | `data-rq-block` | Legacy markers |
|---|---:|---:|
| demo (4) | 87 / 34 / −65 | 142 / 44 / −29 |
| 4 small | 80 / 23 / 13 | 176 / 38 / 37 |
| 16 small | 320 / 51 / 36 | 704 / 102 / 58 |
| 64 small | 1,280 / 172 / 78 | 2,816 / 352 / 174 |
| 64 large | 1,280 / 252 / 3 | 2,816 / 460 / 172 |

The RQP attribute markup costs about half of the legacy markers. Negative Brotli values are noise (§2).

### 3.5 Header fields

FACT. Each set on its own new HTTP/2 connection; HTTP/1.1 bytes / HPACK bytes. Demo (`Rq-Have` value 98 bytes):

| Set | Request | Response |
|---|---:|---:|
| base (no RQP) | 111 / 83 | 89 / 58 |
| RQP first visit | 158 / 120 | 162 / 114 |
| RQP revisit (`304` or D) | 367 / 275 | 83–113 / 77 |
| Q1: 32-bit prefixes in `Rq-Have` | 347 / 259 | 83 / 77 |
| Legacy first visit | 130 / 97 | 212 / 155 |
| Legacy revisit | 243 / 170 | 46–213 / 46–156 |

With 64 blocks (`Rq-Have` 738 bytes) the RQP revisit request is 1,007 / 790 (Q1: 667 / 516). FACT: the RQP response fields are smaller than the legacy ones (F 114 vs 155 HPACK; D 77 vs 156; legacy sends two 40-character hex values plus `template-change` and `Cache-Offline`). The RQP revisit request is larger (275 vs 170 on the demo) because of `Rq-Have`. The legacy `304` has no `ETag` (RFC 9110 §15.4.5 and RQP-03 require it).

### 3.6 Whole visits

FACT. Request header block + response header block (HPACK, new connection) + gzip body; P1 as in §6.

| Page | Visit | RQP baseline | RQP P1 | Legacy |
|---|---|---:|---:|---:|
| demo | cold | 2,941 | 2,591 | 2,484 |
| demo | unchanged | 352 | 352 | 216 |
| demo | change | 878 | 572 | 642 |
| 4 small all | cold / unchanged / change | 1,068 / 354 / 832 | 819 / 354 / 536 | 696 / 215 / 588 |
| 64 small one | cold / unchanged / change | 3,884 / 866 / 1,199 | 1,264 / 866 / 1,015 | 1,300 / 216 / 1,293 |
| 64 small all | cold / unchanged / change | 3,884 / 866 / 3,920 | 1,264 / 866 / 1,452 | 1,300 / 216 / 1,291 |

## 4. What legacy has and what it lacks

FACT (`docs/engineering/protocol/legacy-wire.md`, LW, and the lab bytes). Both have a page validator (legacy `Etag`: SHA-1 hex, 40 characters; RQP `r1.…`, 46), a template validator (`template-tag` / `template_revision`), a result check (`html-sha1` 42 B / `next_revision` 48 B) and block identity. Legacy lacks: (1) a collision-resistant hash (SHA-1, "not a security mechanism", LW §4; shorter because weaker); (2) a change set ("The server sends all data blocks", LW §5.4; 64 large blocks with 1 change: 20,153 vs 752 gzip); (3) a base binding; (4) a URL binding (H-58); (5) a content check (H-48, H-78); (6) `policy` and `limits` (only `Cache-Offline`); (7) version negotiation; (8) its own media type and `no-store` for the update; (9) an `ETag` on `304`. Legacy spends bytes that RQP does not: in the demo data body of 497 B, marker comments 142, element tags 90, `html-sha1` and `template-tag` 84, placeholder keys 35; content 71.

## 5. Options

Gains in gzip / Brotli on the demo, 4 small, 16 large and 64 small (manifest options on the cold F; delta options on D with all blocks changed).

Manifest (FACT):

| Option | demo | 4 small | 16 large | 64 small |
|---|---:|---:|---:|---:|
| M1 omit default `policy`/`limits` members | 65 / 67 | 0 / 0 | 0 / 0 | 0 / 0 |
| M2 no block digests | 178 / 151 | 161 / 158 | 737 / 622 | 2,360 / 2,200 |
| M3 minimal manifest | 278 / 270 | 249 / 221 | 876 / 716 | 2,620 / 2,375 |
| M4 URL in the revision | 315 / 313 | 283 / 246 | 927 / 776 | 2,655 / 2,411 |
| M5 short member names | 60 / 66 | 34 / 16 | 39 / 35 | 47 / 42 |
| M6 no `protocol` member | 11 / 38 | 13 / 11 | 16 / 3 | 15 / 19 |
| M7 no revision prefix | 4 / 19 | 5 / 3 | 5 / 6 | 5 / 7 |
| M8 truncate to 128 bits | 115 / 132 | 108 / 96 | 362 / 341 | 1,123 / 1,062 |
| M9 column block list | 7 / 68 | 0 / −6 | 9 / 22 | 41 / 56 |
| M10 short manifest type | 8 / −1 | 8 / 8 | 11 / 11 | 8 / 13 |
| MC1 = M1 + M3 | 350 / 390 | 249 / 221 | 876 / 716 | 2,620 / 2,375 |

Delta, all blocks changed (FACT):

| Option | demo | 4 small | 16 large | 64 small |
|---|---:|---:|---:|---:|
| D1 no operation hash | 159 / 150 | 154 / 140 | 748 / 611 | 2,292 / 2,231 |
| D2 no `base_revision` | 41 / 48 | 41 / 40 | 49 / 79 | 41 / 46 |
| D3 no `template_revision` | 47 / 47 | 46 / 40 | 53 / 87 | 48 / 53 |
| D4 no `type`/`protocol` | 27 / 26 | 28 / 25 | 33 / 28 | 33 / 37 |
| D5 no `op`/`format` | 33 / 25 | 25 / 21 | 30 / 47 | 58 / 58 |
| D6 short member names | 54 / 24 | 51 / 19 | 74 / 46 | 57 / 55 |
| D8 truncate to 128 bits | 122 / 126 | 122 / 119 | 395 / 288 | 1,094 / 1,088 |
| DC1 minimal object (D1–D5) | 306 / 271 | 296 / 282 | 948 / 783 | 2,468 / 2,392 |
| DC3 minimal array | 346 / 320 | 344 / 297 | 1,020 / 806 | 2,557 / 2,431 |
| DC4 content outside JSON | 346 / 321 | 341 / 290 | 1,000 / 898 | 2,560 / 2,422 |
| DC5 minimal CBOR | 314 / 295 | 309 / 295 | 947 / 885 | 2,500 / 2,407 |

**Integrity argument for the redundant fields.** DESIGN. Step 7 of manifest-patch §2.4 checks that R′ (from T, the policy line of the snapshot and L′) equals `next_revision`. Equal values mean equal preimages unless SHA-256 has a collision, so T, the policy line and every (id, format, hash) line are the server's values and the patched document is byte for byte the server's original. The same holds for the manifest (§1.8 item 5). A field that the client can compute and that the revision preimage covers adds no integrity; it gives only an earlier and more precise error.

- **M1** (omit default `policy`/`limits` members; the page revision follows the members that remain, H-55 rule 1): no security effect; a server SHOULD, not a format change.
- **M2/M3** (no digests; M3 also no `blocks` and no `template_revision`: `{protocol, origin, path, page_revision[, policy][, limits]}`): no integrity loss; losses: (a) a mismatch no longer names the block; (b) tools that read the manifest without the analysis lose the block list; (c) the text of §1.3 rule 3 must change (after the page revision check the client uses its computed T and digests, also for `Rq-Have`). DESIGN benefit: a client that skips the analysis cannot work, so the MUST of H-48 enforces itself. KAT page revisions do not change.
- **M4** (origin and path in a new revision preimage): about 37 gzip over M3; an algorithm change in 4 languages; the client cannot tell a URL mismatch from a content mismatch. Medium cost.
- **M5, D6** (short names): about 60 + 54 gzip on the demo; readability cost; gzip already makes repeated names cheap.
- **M6, M7, M10, M9** (small): M7 is not recommended (algorithm agility of the prefixes, §1.3 rule 4; the `Rq-Have` `t` value is an SF Token that must start with a letter, RFC 9651 §3.3.4); M10 changes a markup token (M-13, H-28); M9 is moot after M3.
- **M8, D8** (truncate to 128 bits): not recommended: H-2; collision work 2^64 for a party that controls content of two versions gives two pages with one revision (RESEARCH: feasible for a strong attacker).
- **D1** (no operation hash; step 4 computes it, step 5 uses it): integrity stays with `next_revision`; the error becomes `INTEGRITY_ERROR` at step 7 instead of step 4; only localization is lost. Reverses H-10.
- **D2, D3** (no `base_revision`, no `template_revision` in D): steps 1 and 2 (`TEMPLATE_MISMATCH`, `PATCH_BASE_MISMATCH`) merge into `INTEGRITY_ERROR` at step 7; step 8 still checks T; the error catalog changes (H-50). DESIGN for D2: a delta kept by a cache against `no-store` can be replayed to more snapshots; the result is still a genuine earlier server page (step 7); with `base_revision` the same replay works on its base snapshot; neither field gives freshness (TLS and `no-store` do). **Wrong; corrected in §9** (S6 security review F1, H-107).
- **D4, D5** (no `type`/`protocol`, no `op`/`format`): the media type, the "offered" check and step 7 reject other JSON; `format` must equal the snapshot format (§2.3 rule 4); `op` is the only value (H-47). Cost: self-description and extensibility.
- **D7, DC3, DC4, DC5/6** (shape and encoding): D7 ≤ 16 gzip; DC3 40 gzip over DC1 (positional; no duplicate-member risk); DC4 equal to DC3 within about 20 gzip, needs a new framing parser and a UTF-8 check outside JSON (against H-11), not recommended; CBOR 8 to 26 gzip better than JSON, a new codec in 5 languages, not recommended for `rqp/1`.
- **Request fields**: Q1 (32-bit prefixes in `Rq-Have b`): 16 HPACK bytes on the demo, 274 with 64 blocks; a party that controls content can force a prefix collision with about 2^32 work, effect one extra full load (H-44), which the same party can force anyway (H-56); low value. Q2 (no `b`): response grows to every block (64 small, 1 change: 332 → 3,071 gzip); not recommended. Q3 (64-bit `t`): 24 HPACK bytes; not worth it.
- **No manifest, revision only in `ETag`**: lower bound 485 gzip; not acceptable (an intermediary can weaken or remove the ETag, negotiation §5.1; the in-body revision is the server's commitment and carries `policy` and `limits`).
- **WP-18 interaction.** RESEARCH. Keyed revisions (HMAC, §1.3 rule 4) cannot be computed by the client; H-44 already makes the client compute `next_revision` from unkeyed hashes, so WP-18 must keep an unkeyed revision in the body and key only the header values, or define another check. P1 does not block this; it depends on the client computation as H-44 does today.

## 6. Ranking and proposals

DESIGN. Cost classes: none; diagnostic (integrity unchanged, error precision lost); low (readability, extensibility); medium (algorithm change or new parser); high (an attack opens).

| Rank | Option | Demo gzip | Grid gzip | Cost | Decision touched |
|---|---|---:|---:|---|---|
| 1 | D1 no operation hash | 159 | 42–2,292 | Diagnostic | H-10 |
| 2 | M3 minimal manifest | 278 | 249–3,324 | Diagnostic | H-44 text, §1.3 rule 3 |
| 3 | M1 no default members | 65 | 0 | None | H-55 (unchanged) |
| 4 | D3 + D2 | 88 | 81–102 | Diagnostic (2 codes merge). For D2 wrong: medium, an attack opens (§9) | H-46, H-50 |
| 5 | D5 + D4 | 60 | 46–91 | Low | H-47 |
| 6 | Short names M5 + D6 | 114 | 60–120 | Low | — |
| 7 | M4 URL in the revision | 37 over M3 | 34–51 | Medium | H-58, H-8 |
| 8 | DC3 array | 40 over DC1 | 25–89 | Low/medium | H-11 |
| 9 | Q1 32-bit prefixes | 16 HPACK | 3–274 HPACK | Low | H-44 |

- **P1** = M1 + M3 + DC1 (D1 + D2 + D3 + D4 + D5). Envelope: `{"next_revision":"r1.…","operations":[{"block_id":"…","content":"…"}]}`.
- **P2** = P1 + M4 + short names + DC3.

FACT (gzip / Brotli):

| Page | Visit | Baseline | P1 | P2 | Legacy |
|---|---|---:|---:|---:|---:|
| demo | cold | 2,707 / 1,686 | 2,357 / 1,296 | 2,319 / 1,317 | 2,232 / 1,252 |
| demo | change | 526 / 467 | 220 / 196 | 180 / 147 | 316 / 260 |
| 4 small | cold | 834 / 618 | 585 / 397 | 538 / 343 | 445 / 283 |
| 4 small | change | 478 / 429 | 182 / 147 | 134 / 132 | 263 / 228 |
| 16 small | cold | 1,409 / 1,139 | 677 / 438 | 635 / 420 | 575 / 343 |
| 16 small | change | 994 / 910 | 268 / 214 | 214 / 169 | 407 / 322 |
| 16 large | cold | 6,658 / 4,367 | 5,782 / 3,651 | 5,714 / 3,586 | 5,628 / 3,582 |
| 16 large | change | 6,163 / 4,229 | 5,215 / 3,446 | 5,143 / 3,423 | 5,391 / 3,591 |
| 64 small | cold | 3,651 / 3,006 | 1,031 / 631 | 981 / 597 | 1,048 / 597 |
| 64 small | change | 3,053 / 2,772 | 585 / 380 | 496 / 341 | 965 / 619 |
| 64 large | cold | 23,527 / 16,223 | 20,203 / 13,672 | 20,132 / 13,671 | 20,178 / 13,758 |
| 64 large | change | 23,613 / 16,333 | 19,698 / 13,241 | 19,609 / 13,372 | 20,252 / 13,742 |

FACT. With P1, 64 large blocks all changed gets D (140,497 raw) instead of F (142,689 raw). The remaining cold gap on the demo with P1 is 125 gzip: `page_revision` (49), `origin`/`path` (31), the wrapper (20), `protocol` (11) and member names.

## 7. Not found and limits

NOT FOUND: other compression settings (gzip level, Brotli quality); zstd; QPACK; CDN behavior with the smaller forms; device timing; the block count and size distribution of real pages. The legacy side is the Node.js server only.

## 8. Questions for the human

- Q-A manifest content: (1) M3 + M1 (recommended); (2) M2 only; (3) keep, M1 as a server SHOULD.
- Q-B delta envelope: (1) DC1 (recommended); (2) D1 only (reverses H-10); (3) keep (H-10).
- Q-C further compaction: (1) none in `rqp/1` (recommended); (2) short member names; (3) P2 (URL in the revision preimage, array envelope).
- Q-D `Rq-Have` prefix length: (1) keep 64 bits (recommended); (2) 32 bits.

Architect items: fix F-1 in `measure.js` and in the performance page legacy column; change the demo `CONFIG` if M1 is accepted.

## 9. Correction: D2 and the base binding (S6 security review F1, H-107)

Added on 2026-10-20 by task WP-17 S6 T-04a. The sections above stay as the study wrote them; this section corrects them.

**What was wrong.** DESIGN. §0 item 4, §5 (D2) and the rank 4 row of §6 rated D2 (no `base_revision` in the delta) as "diagnostic": only the error code changes. That is wrong. `next_revision` binds only the result of a delta, not its base. Without `base_revision` a delta applies to every snapshot that is equal to its target outside the operations, and a delta with every block (a request without `b`, H-7) applies to every snapshot of the template. FACT. The S6 security review (F1, medium) reproduced four cases with the reference of H-104, through a cache that ignores `no-store` (threat T-05):

| Case | Before H-104 (with `base_revision`) | H-104 (`next_revision` only) | H-107 (`binding`) |
|---|---|---|---|
| Rollback: D(v1 → v2) on a snapshot of v3 | `PATCH_BASE_MISMATCH` | Applied: the client goes back to v2 | `INTEGRITY_ERROR` |
| Stale pinning: D(v1 → v2) on a snapshot of v2 | `PATCH_BASE_MISMATCH` | Applied as a no-op: the client stays on v2 | `INTEGRITY_ERROR` |
| Cross-user: the D of user A on the snapshot of user B (same URL, the operations cover every block that differs) | `PATCH_BASE_MISMATCH` | Applied: the content of A | `INTEGRITY_ERROR` |
| Cross-URL: an all-blocks D of `/p/1` on a snapshot of `/p/2` (same template) | `PATCH_BASE_MISMATCH` when the base pages differ; applied when both URLs had the same base page (no URL in any check) | Applied: the content of `/p/1` | `INTEGRITY_ERROR` (U differs; B too when the base pages differ) |

FACT. Checked on 2026-10-20 (task T-04a) with the reference of `HEAD` `87ab26b` (H-104) and the reference of T-04a (H-107) on the same requests: the four fixtures `FX-RQ-C31-ROLLBACK`, `-STALE-OWN-TARGET`, `-ALL-BLOCKS-OTHER-BASE`, `-ALL-BLOCKS-OTHER-URL` and the five probe scenarios of the review were applied by H-104 and give `INTEGRITY_ERROR` with H-107. The seeded property test `tools/rqp-protocol/test/p1.test.mjs` (H-107 case, 300 random pages) checks that in each such case the page revision R′ alone fits (so H-104 accepted it) and the binding does not.

**What restores it.** Human decision H-107: the envelope member `binding` = `d1.` + B64U(SHA-256(`rqp-d1` LF U LF B LF R′ LF)), where U is the canonical URL of the request, B the base page revision of `If-None-Match` and R′ the next page revision. The client computes all three (`docs/engineering/protocol/manifest-patch.md` §2.2, §2.4 step 7). The binding replaces `next_revision`, so it costs **0 bytes**: the value has the same length, and the member name is 6 bytes shorter. FACT. Demo page, demo URL, reference server, data step 0 → 1: before 293 / 221 / 171, after 287 / 215 / 205 (raw / gzip / Brotli); over the 25 data steps of the demo raw is always −6, the gzip median 215 (before 219), the Brotli median 169 (before 171) with a range of 164 to 205 (before 166 to 203). The page revision preimage, the `ETag` and `Rq-Have` do not change, so H-105 (no URL in the revision preimage) stays.

**What stays.** DESIGN. The binding does not give freshness, and it cannot find a delta that a cache returns to another request with the same base and the same URL (for example two users whose base page is the same). `no-store` stays the defense for that case (manifest-patch §2.2, "Replay").

**Lesson for the cost classes of §6.** DESIGN. A field that the client cannot compute from its own state is not "diagnostic" only because the result check passes: the check must also cover the inputs of the operation (the base and the URL), not only its output.

## 10. The client nonce (S6 security re-review B1 and B2, H-108)

Added on 2026-10-20 by task WP-17 S6 T-06a. Sections 0 to 9 stay as they were written; this section corrects §9 ("What stays") and the rollback row of its table.

**What §9 missed.** FACT. The S6 security re-review (B1 medium, B2 low) reproduced two cases that H-107 accepts, with the reference of `HEAD` `a49ae09` (probe 5 of the review, cases 1 and 2): a rollback A → B → A (the page went v1 → v2 → v3 with the bytes of v1; the client holds v3, which has the page revision of v1, and a cache returns D(v1 → v2)), and a `public` partition replay (a user with credentials gets a personal delta from the `public` snapshot of a page; a cache returns it to a user without credentials with the same `public` snapshot; a user can choose B, because the page revision of a `public` page is the same for every user, and H-65 keeps the result `public` for the user without credentials). In both cases U and B are equal, so the binding of H-107 matches. The rollback row of §9 is true only for a snapshot whose page revision differs from the base of the delta.

| Case | H-107 (`binding` of U, B, R′) | H-108 (`binding` of U, B, N, R′) |
|---|---|---|
| Rollback D(v1 → v2) on v3 ≠ v1 | `INTEGRITY_ERROR` | `INTEGRITY_ERROR` |
| Rollback A → B → A: D(v1 → v2) on v3 = v1 | Applied: the client goes back to v2 | `INTEGRITY_ERROR` (`FX-RQ-C32-NONCE-ROLLBACK-A-B-A`) |
| A later request of the same client from the same snapshot | Applied | `INTEGRITY_ERROR` (`FX-RQ-C32-NONCE-OTHER-BIT`) |
| `public` replay: the personal D of user 1 on the same `public` snapshot of user 2 | Applied, the result stays `public` | `INTEGRITY_ERROR` (`FX-RQ-C32-NONCE-PUBLIC-REPLAY`) |

FACT. Checked on 2026-10-20 (task T-06a) with the reference of T-06a: the review probe 5 gives `INTEGRITY_ERROR` for cases 1 and 2 (the probe does not keep the nonce of a request, so every delta of that probe is rejected); the reference tests repeat both cases with a kept nonce, where the request that got the delta applies it and the other request does not; the seeded property test `tools/rqp-protocol/test/p1.test.mjs` (300 random pages) checks that a delta with the exact U, B and R′ but another nonce, also after A → B → A, gives `INTEGRITY_ERROR`.

**The change.** Human decision H-108: every delta request carries `Rq-Have` member `n`, 16 bytes from a CSPRNG, new for every request; the preimage is `rqp-d1` LF U LF B LF N LF R′ LF, with N the base64url text without padding of the 16 bytes (22 characters); a server sends a delta only for a request with exactly one valid `n`, else F (`docs/engineering/protocol/manifest-patch.md` §2.2, `negotiation.md` §3.4).

**Cost.** FACT. Bodies: 0 bytes (the binding keeps its length). Request: 30 bytes more in HTTP/1.1 form. HPACK header block on a new HTTP/2 connection (method of §3.5, Node.js 26.8.2, 200 random nonces each): the demo revisit request 276 → +23 to +27 bytes (median 25, mean 24.8); with 64 blocks (`Rq-Have` 768 bytes with `n`) +23 to +26 (median 24). On a warm connection the whole `Rq-Have` value is new for each request, so HPACK cannot index it: the second and third revisit of the demo page on one connection send 164 bytes each, before 60. Scripts of the measurement: kept by the task, not in the repository (they import `study/bytes/src/hpack.mjs` unchanged).

**What stays.** DESIGN. The binding with N is true only under three assumptions: an honest server (it can bind any content to any request), TLS (U, B and N travel in the request, so a party that reads the request and writes the response can build any delta; the binding is not a MAC), and a nonce from a CSPRNG that the client never uses again (a repeated or predictable nonce brings the replays back). It gives no freshness and does not protect F (manifest-patch §2.2, "Replay").

**Option for the human, not decided.** RESEARCH. The warm-connection cost comes from the nonce inside `Rq-Have`. A separate field (for example `Rq-Nonce`) would let HPACK index `t` and `b` on a warm connection and cost a few more bytes on a new connection (a field name). H-108 put the nonce in `Rq-Have`; this is a protocol decision of the human.

**Decided: H-109 (2026-10-20).** The human chose the separate field: `Rq-Nonce`, an SF Item whose bare item is a Byte Sequence of exactly 16 bytes (parameters ignored), one field line, sent with every delta request; `Rq-Have` keeps only `t` and `b`, and a member `n` in it is an unknown member that the server ignores. N, the preimage and every binding value stay as in H-108 (`docs/engineering/protocol/negotiation.md` §3.9). Added on 2026-10-20 by task WP-17 S6 T-08a.

FACT. Request header bytes of one revisit, measured on 2026-10-20 (task T-08a) with the method of §3.5: the byte study request (`:method`, `:path`, `:authority`, `accept`, `accept-encoding`, `user-agent`) with the RQP fields of the reference client (`tools/rqp-protocol/src/client.mjs` `clientRequest`) for a snapshot of the demo page (step 0) and of the synthetic page with 64 small blocks; HPACK header block bytes that Node.js 26.8.2 (nghttp2 1.70.0) writes, counted by `study/bytes/src/hpack.mjs` (unchanged); 200 random nonces for each form; "new" is the first request on a new connection, "warm" the second and the third of three revisits on one connection (each with a new nonce). The H-108 form is the H-109 request with `, n=` and the `Rq-Nonce` value appended to `Rq-Have`; the H-107 form has no nonce.

| Request | H-107 | H-108 | H-109 |
|---|---:|---:|---:|
| Demo, `Rq-Have` value | 98 | 128 | 98 |
| Demo, HTTP/1.1 (all fields) | 367 | 397 | 405 |
| Demo, HPACK new (median; range) | 276 | 301 (299–302) | 306 (305–309) |
| Demo, HPACK warm, 2nd and 3rd (median; range) | 60 | 164 (162–166) | 83 (82–86) |
| 64 blocks, `Rq-Have` value | 738 | 768 | 738 |
| 64 blocks, HTTP/1.1 (all fields) | 1,007 | 1,037 | 1,045 |
| 64 blocks, HPACK new (median; range) | 790 | 814 (813–816) | 820 (819–822) |
| 64 blocks, HPACK warm, 2nd and 3rd (median; range) | 58 | 677 (676–679) | 82 (80–83) |

FACT. The H-108 values agree with the H-108 numbers above (the demo revisit 301 median on a new connection, 164 warm). The H-107 demo value is 1 byte more than in §3.5 (275): §3.5 used the page revision of the demo before P1, and HPACK Huffman lengths depend on the characters of the revision and of the prefixes (the study pages with 4 blocks, whose fields have the same lengths, give 277 in `study/bytes/results/bytes.json`). A repeat revisit on a warm connection costs the H-107 value plus 23 (demo) or 24 (64 blocks) bytes: the nonce value is new on every request, and HPACK reuses only the name `rq-nonce` from its dynamic table. Against H-108 the separate field costs 8 bytes more in HTTP/1.1 form and 5 or 6 bytes more on a new connection (the field name), and saves 81 bytes (demo) and 595 bytes (64 blocks) on each warm revisit. The first revisit on a connection costs the same as on a new connection.

FACT. Sending `Rq-Nonce` as a never-indexed literal (RFC 7541 §6.2.3; Node.js `http2.sensitiveHeaders`) costs more: 90 or 91 bytes (demo) and 88 or 89 bytes (64 blocks) on a warm connection, because the name is then sent as a literal every time (100 runs, same method). The default encoding of the HTTP/2 stack is the better choice; the specification does not ask for another one.

Scripts of the measurement: kept by the task, not in the repository (they import `study/bytes/src/hpack.mjs` and `study/bytes/src/pages.mjs` unchanged).
