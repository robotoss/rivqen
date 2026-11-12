# Template engine

The template engine (`rivqen-template`) splits HTML into a template and data blocks, rebuilds HTML from them, and computes the legacy hashes. It must reproduce the upstream legacy behavior exactly (canonical profile), and it must be safe on any input.

**Status:** <Badge type="info" text="DESIGN" /> Normative rules: [legacy markers grammar](/engineering/protocol/legacy-markers).

[[toc]]

## 1. Functions

| Function | Input | Output |
|---|---|---|
| `split_v1(html, limits)` | UTF-8 bytes, limits (canonical profile only, CL-41, DL-159) | `Template`, ordered `DataBlocks`, diagnostics |
| `render_v1(template, data)` | Template + data | HTML bytes |
| `etag_v1(html)` | HTML bytes | Lowercase hex SHA-1 |
| `template_tag_v1(template)` | Template bytes | Lowercase hex SHA-1 |
| `split_rqp(html)` | HTML with `data-rq-block` markup (HTML tokenizer, see [RQP markup](/engineering/protocol/markup)) | Template, blocks, block digests (computed by the client, H-103) |
| Manifest locator | Full RQP document | The one manifest element at the insertion point P of the head walk ([manifest and patch](/engineering/protocol/manifest-patch) §1.7), or `MANIFEST_INVALID` (H-142) |
| `render_rqp` | Stored document, block offsets, new block contents | The patched document for the analysis of §2.4 step 8 (H-142) |
| Patch content rule | Content of each patched `html` block | Accept, or `PATCH_CONTENT_REJECTED` ([manifest and patch](/engineering/protocol/manifest-patch) §4, §4.2; §2.4 step 9; H-142) |

<Badge type="info" text="DESIGN" /> The legacy data diff (`diff_v1`) is in `rivqen-diff` (WP-08, [diff engine](/engineering/core/diff-engine) §2; DL-160). The legacy functions are behind the Cargo feature `legacy` (DL-164). The limits come as one `TemplateLimits` value (DL-163).

## 2. Implementation approach

Upstream uses regular expressions over the raw text. Rivqen uses a **hand-written linear scanner** that implements the same language:

1. It finds `<!--sonicdiff` with a byte search (`memchr`-style).
2. It reads the optional `-` and the name (`[A-Za-z0-9_]`).
3. It requires `-->`.
4. It searches for the nearest `<!--sonicdiff…-end-->` after at least one content byte.
5. It records spans; it never copies the content until the output is built.

Why not a regex crate: the scanner gives linear time with explicit limits and exact control of edge cases (empty names, mismatched end names), and it is easy to fuzz. A test compares the scanner with a backtracking matcher of the C-06 and C-07 expressions, written from the spec, on random inputs (differential testing, DL-162). No `memchr` is needed: a plain byte scan takes at most 4.4 ms per 5 MiB on the worst measured input (DL-162).

Why not an HTML parser: legacy is textual. An HTML parser (for example `html5ever` or `lol_html`) would change which markers are seen (inside `<script>`, comments, attributes). A parser is considered only for RQP region markers.

## 3. Limits

| Limit | Value | Source |
|---|---|---|
| Document size | `maxDocumentBytes` | [Resource limits](/engineering/core/resource-limits) |
| Blocks | `maxBlocks` | same |
| Name length | 128 bytes | same |
| Block size | `maxBlockBytes` | same |

Complexity: one pass for markers, one pass for the title rule; O(n) time, O(blocks) extra memory.

## 4. Properties

| ID | Property |
|---|---|
| P-TPL-1 | `render_v1(split_v1(h)) == h` for every `h` the grammar accepts |
| P-TPL-2 | `split_v1` never panics and stays within limits for any byte input |
| P-TPL-3 | `split_v1` equals a test-only backtracking matcher of the C-06 and C-07 expressions (differential, DL-162) |
| P-TPL-4 | `etag_v1` and `template_tag_v1` equal the values in every golden fixture |

## 5. Fuzz targets

- `split_v1`: arbitrary bytes.
- `render_v1`: arbitrary template and data maps.
- `split_rqp`: arbitrary bytes.
- `patch_content`: arbitrary block content.

<Badge type="info" text="DESIGN" /> Acceptance: each target runs 1 h without a crash, one target at a time (H-144). `fuzz_diff_v1` is in WP-08 (DL-160).
