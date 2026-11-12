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
| P-TPL-4 | `etag_v1` and `template_tag_v1` equal the values in every golden fixture. Tests: `legacy_fixtures_pages`, `legacy_fixtures_malformed`, `legacy_fixtures_functions` (see below) |

<Badge type="tip" text="FACT" /> The golden tests (`crates/rivqen-template/tests/legacy_fixtures.rs`) read the fixtures as data through `rivqen-testkit` (`fixtures`, `functions`). They check `split_v1`, `render_v1`, `etag_v1` and `template_tag_v1` against: 59 canonical server fixtures of `fixtures/legacy` with a `split` member, the 22 cases of `fixtures/malformed/legacy`, and the 35 canonical `split` function fixtures (34 results and FN-SPLIT-09, `duplicate-block-name`). A fixture with `split.applied = false` has one of two reasons. Scope rule (C-01, C-03: HEAD or POST, status other than 200, content type other than one `text/html; charset=utf-8`): the test checks that the condition is in the fixture data and that `split_v1` applies on the body. Body rule (invalid UTF-8, repeated block name, block named `title`): `split_v1` must give the same reason. Server profile fixtures, the Android and iOS results (DL-159), and the function fixtures for `diff` and `etag` (DL-160) are not tested here. The function test compares split results only: FN-SPLIT-TITLE-06 cannot be rebuilt (DL-167).

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
