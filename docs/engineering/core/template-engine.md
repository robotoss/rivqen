# Template engine

The template engine (`rivqen-template`) splits HTML into a template and data blocks, rebuilds HTML from them, and computes the legacy hashes. It must reproduce the upstream legacy behavior exactly (canonical profile), and it must be safe on any input.

**Status:** <Badge type="info" text="DESIGN" /> Normative rules: [legacy markers grammar](/engineering/protocol/legacy-markers).

[[toc]]

## 1. Functions

| Function | Input | Output |
|---|---|---|
| `split_v1(html, profile)` | UTF-8 bytes, profile | `Template`, ordered `DataBlocks`, diagnostics |
| `render_v1(template, data)` | Template + data | HTML bytes |
| `etag_v1(html)` | HTML bytes | Lowercase hex SHA-1 |
| `template_tag_v1(template)` | Template bytes | Lowercase hex SHA-1 |
| `diff_v1(local, server)` | Two data maps | Changed keys (canonical: new keys included) |
| `split_rqp(html)` | HTML with `data-rq-block` markup (HTML tokenizer, see [RQP markup](/engineering/protocol/markup)) | Template, blocks, manifest hashes |

## 2. Implementation approach

Upstream uses regular expressions over the raw text. Rivqen uses a **hand-written linear scanner** that implements the same language:

1. It finds `<!--sonicdiff` with a byte search (`memchr`-style).
2. It reads the optional `-` and the name (`[A-Za-z0-9_]`).
3. It requires `-->`.
4. It searches for the nearest `<!--sonicdiff…-end-->` after at least one content byte.
5. It records spans; it never copies the content until the output is built.

Why not a regex crate: the scanner gives linear time with explicit limits and exact control of edge cases (empty names, mismatched end names), and it is easy to fuzz. A test compares the scanner with the upstream regular expression on a large random corpus (differential testing) for every profile.

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
| P-TPL-3 | For each profile, `split_v1` equals the reference regex implementation of that profile (differential) |
| P-TPL-4 | `etag_v1` and `template_tag_v1` equal the values in every golden fixture |

## 5. Fuzz targets

- `fuzz_split_v1`: arbitrary bytes, each profile.
- `fuzz_render_v1`: arbitrary template and data maps.
- `fuzz_diff_v1`: arbitrary JSON maps within depth limits.
