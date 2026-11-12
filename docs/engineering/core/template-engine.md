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

Upstream uses regular expressions over the raw text. Rivqen uses a **hand-written linear scanner** that gives the same matches as the expressions of [legacy wire](/engineering/protocol/legacy-wire#canonical-server) C-06 and C-07 (C-16, DL-077).

<Badge type="tip" text="FACT" /> `split_v1` (module `rivqen_template::legacy`, private modules `title`, `blocks`, `text`) works in this order:

1. If the body is larger than `max_document_bytes`, it stops with `DOCUMENT_TOO_LARGE`. If the body is not valid UTF-8, the split is not applied (`InvalidUtf8`, C-03).
2. Title rule C-06. The scanner tries each `<title` opener from left to right (ASCII case-insensitive). An opener matches when the first `</title>` after it has no LF before it; a CR does not stop a match. The positions of the next `</title>` and of the next LF are kept from one opener to the next, so each byte is read a bounded number of times.
3. Block rule C-07 on the result of C-06. The result is not copied: it is read as three borrowed parts (text before the title, `{title}`, text after the title).
   - A marker is `<!--sonicdiff`, then `-?(\w*)`, then `-->` (opener) or `-end-->` (end marker). `\w` does not match `-`, so the name is the whole word run. The scanner tries the form with `-` first, then the form without it, as a backtracking engine does. Example: `<!--sonicdiff-end-end-->` is an end marker with the name `end`.
   - The search for the end marker starts one byte after the opener (content of at least one byte). The first end marker ends the match (lazy content). The end name is not checked; a different end name (case-sensitive) gives `Diagnostic::MismatchedEndName`.
   - If no end marker follows an opener, the scan stops: a later opener needs an end marker even later.
4. For each match, in document order: an empty name or a name over `max_name_bytes` stays in the template with no key; a repeated name (case-sensitive) gives `NotApplied::DuplicateName`; the name `title` gives `NotApplied::BlockNamedTitle`; then the limits of §3 apply. The first failure in document order is the result.
5. The scanner records spans only. It copies text once, when it builds the template and the values (C-08: `{title}` first, then the blocks in document order).

<Badge type="tip" text="FACT" /> `render_v1` gives the canonical rebuild of [legacy markers](/engineering/protocol/legacy-markers#_5-rebuild-rule-client) §5 with these exact rules (WP-06 S1 T-01):

1. The template is text: nothing tells a placeholder apart from the same text in the page. The placeholder of a data entry is the **first** occurrence of its key in the template. Keys cannot overlap, so the result does not depend on the order of the data. A page with the key text before its block rebuilds to another page, and the `html-sha1` check of the caller rejects it (CL-29; fixture `FX-LEG-MAL-PLACEHOLDER-TEXT`; `split.rebuild_sha1` in `fixtures/legacy/README.md`).
2. The entry `{title}` with an empty value has no placeholder: C-06 gives an empty value only when no title matches, so a `{title}` in the template is page text (for example `{{title}}`).
3. A value is copied as it is. Key text inside a value is not replaced. So a title inside a block (function fixture `FN-SPLIT-TITLE-06`) cannot be rebuilt: the `{title}` placeholder is in the value of the block, not in the template.
4. `RenderError::Mismatch`: a key that is not `{title}` or `{name}` (1 to `max_name_bytes` bytes of `[A-Za-z0-9_]`), a repeated key, no `{title}` entry, or an entry (other than the empty `{title}`) without a placeholder.
5. The scanner reads the template once. At each `{` it reads at most the length of the longest key, then looks the token up among the keys that have no placeholder yet.

<Badge type="tip" text="FACT" /> Measured on 2026-11-12 on the Mac host, release build, best of three runs (`legacy_linear_time`): `split_v1` takes 0.24 ms to 1.1 ms on each of nine adversarial 256 KiB inputs (title openers without LF, title openers with an LF before the only close, unclosed block openers, end marker prefixes, 500-byte names without a tail, all `<`, and others); `render_v1` takes 1.9 ms on a 256 KiB template of key-like text with 256 keys. The C-16 budget is 100 ms.

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

<Badge type="tip" text="FACT" /> The functions take one `TemplateLimits` value (DL-163). Each limit passes at its value and fails one above it (`legacy_limits_*`). Every `LimitError` has the code `DOCUMENT_TOO_LARGE`.

| Check | `split_v1` | `render_v1` |
|---|---|---|
| `max_document_bytes` | Input over it: `DocumentTooLarge`, before the UTF-8 check | Template over it, or output over it: `Limit(DocumentTooLarge)`. The output size is computed before the output is allocated |
| `max_blocks` | More keyed blocks: `TooManyBlocks`. `{title}` and blocks without a key do not count | More than `max_blocks + 1` entries: `Limit(TooManyBlocks)` |
| `max_block_bytes` | A keyed block value (markers included) over it: `BlockTooLarge`. The `{title}` value is not a block | A value other than `{title}` over it: `Limit(BlockTooLarge)` |
| `max_name_bytes` | A longer name stays in the template with no key, so it cannot be a repeat or `title` | A longer name in a key: `Mismatch` |

The template of a split is never longer than the input, so `split_v1` has no output check. Memory: the spans and names of at most `max_blocks` blocks, then the template and the values.

## 4. Properties

| ID | Property |
|---|---|
| P-TPL-1 | `render_v1(split_v1(h)) == h` for every `h` the grammar accepts |
| P-TPL-2 | `split_v1` never panics and stays within limits for any byte input |
| P-TPL-3 | `split_v1` equals a test-only backtracking matcher of the C-06 and C-07 expressions (differential, DL-162) |
| P-TPL-4 | `etag_v1` and `template_tag_v1` equal the values in every golden fixture |

<Badge type="tip" text="FACT" /> P-TPL-1 holds exactly when each placeholder is the first occurrence of its key in the template and the `{title}` placeholder is in the template (§2, rebuild rules 1 and 3). The pages `{a}<block a>` and `<block a>{a}` give the same template and data, so no rebuild rule can give both back. The tests (`crates/rivqen-template/tests/`, fixed seeds, `PROPTEST_CASES` raises the count):

| Property | Test | Input |
|---|---|---|
| P-TPL-1 | `legacy_prop_round_trip` | Pages from parts: unique block names, title outside the blocks, no `{`. The rebuild must give the page back |
| P-TPL-1 | `legacy_prop_round_trip_iff_first_occurrence` | Marker-rich text. The rebuild gives the page back if and only if the condition above holds (placeholder positions from the reference matcher); else the error is `Mismatch` or the page differs |
| P-TPL-2 | `legacy_prop_any_bytes`, `legacy_prop_render_any_input` | Any bytes, marker-rich text, small and default limits; any template and data for the rebuild |
| P-TPL-3 | `legacy_prop_equals_expressions` | Marker-rich text (each part of the expressions in several cases, whole markers, LF, CR, key text, non-ASCII), any bytes, one inserted byte; small and default limits. The reference is `tests/legacy_oracle`: a backtracking matcher that compiles the expression text of C-06 and C-07 |
| C-16 | `legacy_linear_time` (ignored; run in release with `--ignored`) | Nine adversarial 256 KiB inputs and one rebuild; each under 100 ms |

## 5. Fuzz targets

- `split_v1`: arbitrary bytes.
- `render_v1`: arbitrary template and data maps.
- `split_rqp`: arbitrary bytes.
- `patch_content`: arbitrary block content.

<Badge type="info" text="DESIGN" /> Acceptance: each target runs 1 h without a crash, one target at a time (H-144). `fuzz_diff_v1` is in WP-08 (DL-160).
