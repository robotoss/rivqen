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
| `split_rqp(input, limits)` | Untrusted bytes with `data-rq-block` markup, checked with the rules M-01 to M-27 of [RQP markup](/engineering/protocol/markup) (section 6) | `RqpDocument`: template revision `t1.`, blocks in document order (`BlockDescriptor` and content offsets; digests computed by the client, H-103), or `MarkupError` (code and rule) |
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

## 6. RQP markup analysis

<Badge type="tip" text="FACT" /> `split_rqp` is in the module `rivqen_template::rqp`. It is a port of the candidate `tools/rqp-markup/rust` at commit `92c708e1` (DL-161). The rules and the tokenizer are the same code; each ported file names its candidate file in its header comment. The candidate stays unchanged as one of the four parsers of `diff.mjs`.

| Module | Candidate file | Content |
|---|---|---|
| `tokenizer` (private) | `src/tokenizer.rs` | WHATWG tokenizer subset with byte spans (M-03, M-04) |
| `names` (private) | `src/names.rs` | Tag names and the name sets of markup section 2.1 |
| `rules` (private) | `src/rules.rs` | One pass: token stack (M-05), rules M-06 to M-26 |
| `json` (private) | `src/json.rs` | JSON text check of M-25 (depth at most 64) |
| `analysis` (private) | `src/lib.rs` | M-01, M-02, the result (M-27), the template revision |
| `scan` (public) | `src/scan.rs` | Tokens, gaps and raw attributes for WP-06 S2 (manifest locator, head walk, patch content rule) |

<Badge type="tip" text="FACT" /> Changes of the port:

- The blocks are `RqpBlock` values: `BlockDescriptor` (`BlockId`, `BlockFormat`, `Sha256Digest`) and the content offsets `start` and `end`. The template revision is a `TemplateRevision`. No revision is a `String`.
- The page revision needs the policy line of the manifest, so `RqpDocument::page_revision(policy, limits)` computes it. `page_revision(None, None)` is the revision of the markup fixtures.
- The limits come from `TemplateLimits` (DL-163). No CLI and no `result_json`.
- An internal defect (an offset outside the input, blocks out of order) fails closed: `RQP_MARKUP_STRUCTURE` with the rule `internal`. No rule of the markup page causes it.

### 6.1 Limits

| Rule | Limit | Value in the analysis |
|---|---|---|
| M-01 | `maxDocumentBytes` | The smaller of `max_document_bytes` and 5 MiB (`MAX_MARKUP_INPUT_BYTES`). Checked first, before M-02 |
| M-26 | `maxBlocks` | `max_blocks`, at most 256 |
| M-26 | `maxBlockBytes` | `max_block_bytes`, at most 1 MiB, for the content of each block |
| M-26 | `maxOpenElements` | `max_open_elements`, at most 256: the largest value of the depth counter D, also in the pass over a `noscript` content and for implied table parts |
| M-25 | JSON depth | 64 (fixed by the protocol) |

<Badge type="tip" text="FACT" /> A lower value gives `RQP_MARKUP_LIMIT` earlier. A higher value has no effect. `TemplateLimits::default()` gives the values of M-26 (tests `rqp_limits_*`).

<Badge type="tip" text="FACT" /> Resource bounds: the token stack holds at most D elements, and D is at most `max_open_elements`. The work and the memory are linear in the input size: the name table holds one entry per distinct tag name, and the check of a `noscript` content makes one copy of that content. The scan of the module `scan` has no limit of its own: call `split_rqp` first.

<Badge type="tip" text="FACT" /> Errors: `MarkupError` holds the code and the rule id only. `Display` and `Debug` hold no document bytes (test `rqp_error_holds_no_input`).

### 6.2 Tests

| Test | Checks |
|---|---|
| `rqp_markup_fixtures` | All 362 markup fixtures (148 valid, 214 invalid): `valid`, the error code, the blocks (id, format, start, end, sha256), the template revision and `page_revision(None, None)` equal `expected.json` |
| `rqp_rules`, `rqp_api`, `rqp_props` | Ports of the candidate tests of the rules, of the scan API and of the properties |
| `rqp_limits_*` | The limits of section 6.1 at the value and one above, with lower, default and higher values |
| `rqp_prop_round_trip` | The template (the input without every `[start, end)`) plus the block contents gives the input back; the template revision and the block hashes agree |
| `rqp_prop_any_bytes` | Any bytes with any limits: no panic, no internal defect, a valid result within the limits |

<Badge type="info" text="DESIGN" /> Two copies of the markup analysis exist until the candidate retires. A change of the markup rules changes both copies. The fixture test and `diff.mjs` find a drift of either copy (WP-06 S1, risk R-2).
