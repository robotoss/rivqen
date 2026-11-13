# Template engine

The template engine (`rivqen-template`) splits HTML into a template and data blocks, rebuilds HTML from them, and computes the legacy hashes. It must reproduce the upstream legacy behavior exactly (canonical profile), and it must be safe on any input.

**Status:** <Badge type="tip" text="FACT" /> Implemented in `crates/rivqen-template` (WP-06 S1, 2026-11-12): the canonical legacy split and rebuild, the legacy SHA-1 values and the RQP markup analysis with its token scanner; (WP-06 S2, 2026-11-13, H-142): the manifest locator with the client checks of a full document (section 7), the patch content rule (section 8) and the build and check of the patched document (section 9). Normative rules: [legacy markers grammar](/engineering/protocol/legacy-markers), [RQP markup](/engineering/protocol/markup).

[[toc]]

## 1. Functions

| Function | Input | Output |
|---|---|---|
| `split_v1(html, limits)` | UTF-8 bytes, limits (canonical profile only, CL-41, DL-159) | `Template`, ordered `DataBlocks`, diagnostics |
| `render_v1(template, data)` | Template + data | HTML bytes |
| `etag_v1(html)` | HTML bytes | Lowercase hex SHA-1 |
| `template_tag_v1(template)` | Template bytes | Lowercase hex SHA-1 |
| `split_rqp(input, limits)` | Untrusted bytes with `data-rq-block` markup, checked with the rules M-01 to M-27 of [RQP markup](/engineering/protocol/markup) (section 6) | `RqpDocument`: template revision `t1.`, blocks in document order (`BlockDescriptor` and content offsets; digests computed by the client, H-103), or `MarkupError` (code and rule) |
| `locate_manifest(full, limits)`, `insertion_point(document, limits)`, `read_full_document(full, origin, path, limits)` (section 7) | Full RQP document | The one manifest element at the insertion point P of the head walk ([manifest and patch](/engineering/protocol/manifest-patch) §1.7), or `MANIFEST_INVALID` (H-142) |
| `render_rqp(original, document, replacements, limits)`, `check_patch(original, stored, meta, delta, limits)` (section 9) | Stored document, block offsets, new block contents | The patched document for the analysis of §2.4 step 8, and the result of steps 8 and 9 (H-142) |
| `check_patch_content(document, analysis, patched, limits)`, `check_attribute(element, foreign, name, raw_value)` (section 8) | Content of each patched `html` block | Accept, or `PATCH_CONTENT_REJECTED` ([manifest and patch](/engineering/protocol/manifest-patch) §4, §4.2; §2.4 step 9; H-142) |

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

<Badge type="tip" text="FACT" /> The result of `render_v1` is not verified (rules 1 and 3; DL-166, DL-167). The caller (WP-08) MUST do these steps before it uses the page (legacy client CL-29):

1. Compute the SHA-1 of the page with `etag_v1`.
2. Compare it with the `html-sha1` member of the response and with the response etag.
3. If a value is different, discard the page and load the page normally.

The doc comment of `render_v1` gives the same rule.

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

<Badge type="tip" text="FACT" /> `TemplateLimits` has private fields and the getters `max_document_bytes()`, `max_blocks()`, `max_block_bytes()`, `max_name_bytes()` and `max_open_elements()` (WP-06 S1 T-06, security review finding S1-4). Only three constructors exist, and each caps every value. So no caller can make a limit larger than its cap (tests `limits_new_caps_every_value`, `limits_try_from_caps_document`).

| Constructor | Values |
|---|---|
| `TemplateLimits::default()` | 5 MiB, 256, 1 MiB, 128, 256 |
| `TemplateLimits::try_from(&ResourceLimits)` | The configured values, capped. A value that does not fit `usize` gives `LimitsError` |
| `TemplateLimits::new` (`const fn`; five values in the order of the getters) | The given values, capped |

| Value | Cap |
|---|---|
| `max_document_bytes` | 32 MiB: the top of the safe range of `maxDocumentBytes` (`Limit::MaxDocumentBytes.range()`) |
| `max_blocks` | 256 (`MAX_BLOCKS`, M-26) |
| `max_block_bytes` | 1 MiB (`MAX_BLOCK_BYTES`, M-26) |
| `max_name_bytes` | 128 (`MAX_LEGACY_NAME_BYTES`, C-07) |
| `max_open_elements` | 256 (`MAX_OPEN_ELEMENTS`, M-26) |

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

<Badge type="tip" text="FACT" /> Resource bounds: the token stack holds at most D elements, and D is at most `max_open_elements`. The work and the memory are linear in the input size. The check of a `noscript` content makes one copy of that content.

<Badge type="tip" text="FACT" /> Name table (WP-06 S2, security review finding S1-3): the table of tag names keeps the names of the rules (static bytes) and another name only while an element with that name is open on the token stack, plus one pending name (the last start tag that pushed nothing). A freed entry is used again for the next new name. So the table holds at most R + S + 1 entries (R: the names of the rules; S: the largest token stack, at most D in the analysis and `max_open_elements` + 2 in the scan). It copies at most the bytes of the open names, which are at most the input size (plus the 11 bytes of the `</noscript>` that M-11 item 8 adds after C). The hash reads the normalized name from the raw bytes, so no buffer grows with a name. The candidate `tools/rqp-markup/rust` has the same table (DL-161). Test: `rqp_names_memory_bound`.

| Input of about 5 MiB | Extra peak heap before | After |
|---|---|---|
| 635 000 distinct self-closing names in `<svg>` | 66.8 MB | 30.5 KB |
| 352 000 distinct names, each opened and closed | 33.4 MB | 30.5 KB |
| One start tag with a name of U+0000 bytes | 32.5 MB | 5.3 MB |
| 256 open elements with names of 20 KiB | 5.3 MB | 5.3 MB |

<Badge type="tip" text="FACT" /> The table gives the peak heap of `split_rqp` above the input, measured on 2026-11-13 with a counting global allocator in a release build (Rust 1.99.0). The scan has the same values within 8 KB. The last two rows are the copies of the open names (one copy of the input bytes at most); before the change a U+0000 name also had a normalized copy and a normalized scratch buffer of three times its size.

<Badge type="tip" text="FACT" /> The scan `scan(input, limits)` takes the same `TemplateLimits` (WP-06 S1 T-06, security review finding S1-2). A limit stop is one error item, the last item of the scan, never a silent cut. The caller must fail closed.

| Limit | Error item |
|---|---|
| Input larger than the smaller of `max_document_bytes()` and 5 MiB | `RQP_MARKUP_LIMIT`, rule `M-01`, before any token |
| A push that makes the token stack larger than `max_open_elements()` + 2 (resource limits: the WHATWG stack is at most D + 2) | `RQP_MARKUP_LIMIT`, rule `M-26`, in place of the start tag |

<Badge type="tip" text="FACT" /> The token stack of the scan applies the implied closure of M-09 (H-85a) with the same steps as `split_rqp` (WP-06 S2, DL-170): a table part start tag in HTML context first closes F while F does not allow the tag (at most three steps; a `template` ends the search for F; when F must be the current node and is not, nothing closes and `split_rqp` rejects the document). Both use one function of the rules module, so the rule tables have one copy. `split_rqp` applies no closure in block content; there a closure step would close only where M-09 rejects the start tag, so the scan does not track blocks. For a document that `split_rqp` accepts, the token stack of the scan is the one of `split_rqp`: it is never larger than D, and each tag has the foreign region of `split_rqp`.

<Badge type="tip" text="FACT" /> Of the 401 HTML inputs of `fixtures/rqp` (185 valid), the scan stops only on `FX-RQ-MARKUP-LIMIT-INPUT-SIZE` (M-01), where `split_rqp` gives `RQP_MARKUP_LIMIT` too. It reads `FX-RQ-MARKUP-TABLE-CLOSE-DEPTH` (300 rows of `<tr><td>a<td>b`, max D 5) to the end; before WP-06 S2 that valid fixture stopped the scan at the stack limit. Tests: `scan_reads_every_valid_markup_fixture`, `scan_stack_applies_implied_closure`, and the property `scan_prop_valid_documents_scan_to_the_end` (for generated documents that the analysis accepts with `max_open_elements` from 0 to 24: no error item, the stack at most D, the foreign region of every tag equal to the one of the analysis).

<Badge type="tip" text="FACT" /> Errors: `MarkupError` holds the code and the rule id only. `Display` and `Debug` hold no document bytes (test `rqp_error_holds_no_input`). The `Debug` output of the public types of the scan (`Token`, `Tag`, `Attribute`, `Attributes`, `Scanner`, `Span`) holds spans, lengths and flags only, never input bytes, tag names, attribute names or values (test `scan_debug_holds_no_input`; [errors and logging](/engineering/architecture/errors-logging) §4).

### 6.2 Tests

| Test | Checks |
|---|---|
| `rqp_markup_fixtures` | All 362 markup fixtures (148 valid, 214 invalid): `valid`, the error code, the blocks (id, format, start, end, sha256), the template revision and `page_revision(None, None)` equal `expected.json` |
| `rqp_rules`, `rqp_api`, `rqp_props` | Ports of the candidate tests of the rules, of the scan API and of the properties |
| `rqp_limits_*` | The limits of section 6.1 at the value and one above, with lower, default and higher values |
| `rqp_scan` (`scan_debug_*`, `scan_limit_*`, `scan_reads_every_valid_markup_fixture`, `scan_stack_applies_implied_closure`) | The scan: no input bytes in `Debug`; the M-01 limit and the token stack limit at the value and one above; the implied closure of M-09 on the token stack; the stops on the HTML inputs of `fixtures/rqp` |
| `scan_prop_valid_documents_scan_to_the_end` (unit test of `scan`) | For a document that the analysis accepts, the scan reads to the end with the token stack of the analysis and its foreign regions |
| `rqp_names_memory_bound` (unit test of `names`) | The name table on pages of distinct names stays at the rule names plus the open names, and copies at most the input size |
| `rqp_prop_round_trip` | The template (the input without every `[start, end)`) plus the block contents gives the input back; the template revision and the block hashes agree |
| `rqp_prop_any_bytes` | Any bytes with any limits: no panic, no internal defect, a valid result within the limits |

<Badge type="info" text="DESIGN" /> Two copies of the markup analysis exist until the candidate retires. A change of the markup rules changes both copies. The fixture test and `diff.mjs` find a drift of either copy (WP-06 S1, risk R-2).

## 7. Full document

<Badge type="tip" text="FACT" /> The client checks of a full RQP document F are in the modules `rivqen_template::rqp::locate` and `rivqen_template::rqp::full` (WP-06 S2 T-01, H-142). They are new code from [manifest and patch](/engineering/protocol/manifest-patch) §1.7 and §1.8 (DL-173). Every error has the code `MANIFEST_INVALID`.

| Function | Input | Output |
|---|---|---|
| `insertion_point(document, limits)` | A document | The insertion point P of the walk "Document head" (§1.7): the byte after the `>` of the head start tag, the `<` of an implied-head start tag, or `None`. `MarkupError` when the scan stops |
| `locate_manifest(full, limits)` | F | `ManifestLocation`: the element range, the range of the manifest text M, and O (F without the element), or `LocateError` |
| `read_full_document(full, origin, path, limits)` | F, the origin and the path of the final URL | `FullDocument`: O, the markup analysis of O and the checked manifest, or `FullDocumentError` |

<Badge type="tip" text="FACT" /> `read_full_document` does the checks in this order. The first failed check is the error.

1. `locate_manifest` (§1.7, client items 1, 4, 5 and 3):
   - Item 1: the first `script` start tag of the scan whose first `type` attribute has the raw value `application/rivqen-manifest+json`, ASCII case-insensitive. Else `NoManifest`.
   - Item 4: M ends at the `<` of the next `script` end tag of the scan. Else `Unterminated`.
   - Item 3: the `<` of the start tag is at P of O. Else `NoHead` (O has no P) or `WrongPosition`.
   - A scan stop before the end of these steps (M-01, the token stack limit) is `Markup`.
2. `read_manifest` of `rivqen-proto` on M (§1.8, items 1, 2 and 4): `Manifest` with `TooLarge`, `Json`, `Schema` or `UrlMismatch`.
3. One copy of O, then `split_rqp` on O (§1.8, item 5): `Markup`.
4. The page revision of O with the members `policy` and `limits` of the manifest (§1.3, H-55) must be equal to `page_revision`: else `PageRevisionDiffers`.

<Badge type="tip" text="FACT" /> Item 2 (exactly one manifest element): the locator reads F up to the first match. A second manifest element is a `script` start tag of O, and M-13 of the analysis of O rejects it (`Markup` with `RQP_MARKUP_RESERVED`; DL-172; test `full_two_manifests_rejects`).

<Badge type="tip" text="FACT" /> Item 3 on O in one pass. `locate_manifest` does not copy O. Before the start tag, the tokens of F are the tokens of O. After the end tag of the element, the tokenizer is in the data state with the token stack of before the start tag, so the tokens of F are the tokens of O, shifted by the length of the element. The walk tests a gap byte by byte, so a gap of O that the element cuts in two gives the same result. The byte order mark rule uses the first 3 bytes of O. The property test `locate_prop_any_bytes` compares the result with the walk on a copy of O for any bytes and for markup-rich inputs. One difference is allowed: when the token stack is full of `html` elements before the start tag, the scan of F stops (`Markup`). The analysis of O rejects such a document too (M-26; test `locate_position_full_stack_before_the_element_is_markup_and_o_is_invalid`).

<Badge type="info" text="DESIGN" /> Token kinds in the walk: a `<![CDATA[` before the head is a comment, because in HTML context the WHATWG tokenizer gives a bogus comment for it (M-03, M-04). A processing instruction is a comment (M-03).

<Badge type="tip" text="FACT" /> Resource bounds: one scan of F up to the end tag of the manifest, plus the walk after the element up to its stop (the next token for an accepted document), plus the passes of `read_manifest` and `split_rqp`. O is allocated once, in `read_full_document`. No input panics: offsets are read with checked access; an offset outside the input is an internal defect and fails closed (`Markup` with the rule `internal`). Section 1.7 items 6 (UTF-8) and 7 (delivery to the WebView) belong to the caller (WP-08).

<Badge type="tip" text="FACT" /> Errors: `LocateError`, `FullDocumentError` and `ManifestLocation` hold no document byte in `Debug` or `Display` (tests `locate_errors_are_manifest_invalid_and_hold_no_bytes`, `full_errors_hold_no_document_bytes`). The derived `Debug` of `FullDocument` (a contract item) prints O and the manifest. Do not log a `FullDocument`.

### 7.1 Tests

| Test | Checks |
|---|---|
| `locate_point_*` (`tests/rqp_locate.rs`) | One test per row of the table of insertion points of §1.7, and the details of items 1 and 2: each white space byte, the byte order mark, `</>`, the end of the input, names in any case, attributes and the self-closing flag, each implied-head name, ignored end tags, the scan stop |
| `locate_type_*` | Item 1: exact value, ASCII case, other values (white space, parameters, character references, other lengths), non-ASCII bytes, only the first `type` attribute, only `script` start tags, no match in comments or raw text, the first match wins |
| `locate_text_*` | Item 4: M, an empty M, end tag forms, the end tag of the tokenizer (also the double-escaped state), no end tag |
| `locate_position_*` | Item 3: white space or a comment after the head start tag, implied head and white space after the element, `<head lang>`, text, body content, `noscript`, the old position, a byte order mark that the element cuts, the scan stops, the fixture bodies `FX-RQ-C21-*` and `FX-RQ-C05-*` |
| `locate_prop_any_bytes`, `locate_prop_soup` | No panic; an accepted element is at P of its O; the result is equal to the walk on a copy of O |
| `full_accepts_*` (`tests/rqp_full.rs`) | The known-answer full documents of §1.3 (`spec/rqp/examples/kat`, with and without the policy line), an implied head, the stored `FX-RQ-C21-*`, `FX-RQ-C05-TYPE-UPPERCASE`, `FX-RQ-C29-MANIFEST-DEFAULT-MEMBERS` |
| `full_order_*` | The order of the checks and the variant of each error |
| `full_page_revision_*` | Another page, the members `policy` and `limits` in and out of the page revision, member order and white space of M |
| `full_two_manifests_rejects`, `full_text_with_lt_rejects` | Item 2 by M-13 of O; a text with `<` never gives an accepted document |
| `full_errors_*` | Every error is `MANIFEST_INVALID`; no document byte in `Debug` or `Display` |
| `full_prop_insert_at_point` | For generated documents with a head or an implied head and a manifest (with and without `policy` and `limits`): the element at P is accepted with `original() == O`; at every other token boundary of O, F is rejected |

## 8. Patch content rule

<Badge type="tip" text="FACT" /> The module `rivqen_template::rqp::content` implements the patch content rule of [manifest and patch](/engineering/protocol/manifest-patch) §4 item 1 and the attribute rule of §4.2 (section 2.4, step 9; WP-06 S2 T-02, H-142). It is new code from the normative text (DL-173). This rule is the XSS boundary of a patch (R-1): it fails closed on every input that it cannot read. Every error is `PATCH_CONTENT_REJECTED`.

| Function | Input | Output |
|---|---|---|
| `check_patch_content(document, analysis, patched, limits)` | The patched document, its markup analysis (step 8), the indices of the blocks with an operation (any order) | `Ok`, or `ContentError { block, violation }` for the first violation in document order |
| `check_attribute(element, foreign, name, raw_value)` | One attribute: the tag name and the attribute name in lower case, the foreign flag, the raw value | `Ok`, or the `ContentViolation` of the first item that rejects |

### 8.1 What the rule reads

<Badge type="tip" text="FACT" /> `check_patch_content` works in this order:

1. Check each index of `patched` (also of a `json` block). An index without a block, a range with `start > end` or with `end` after the document, and two patched `html` ranges that overlap give `Markup` (an internal defect of the caller).
2. Keep the ranges of the patched `html` blocks. A `json` block and a block without an operation are not read. When no patched block is `html`, stop with `Ok` (the document is not scanned).
3. Scan the document from offset 0 with `rqp::scan` and the same `TemplateLimits`, up to the end of the last patched `html` block. The scan reads the content as the analysis reads it: the RCDATA of a `title` block has no start tags, and a `template` of the content does not hide its start tags.
4. For each start tag whose `<` is in `[start, end)` of a patched `html` block, in document order: the name (item 1.1 in HTML context, item 1.2 in a foreign region), then each attribute in source order, also a duplicate that the tokenizer drops (M-03), with `check_attribute`. End tags and the self-closing flag do not change the result. Names are compared in ASCII lower case.

`check_attribute` applies, in order: item 1 (a name that starts with `on`); for `srcset` only item 4.3 (every `srcset` failure is `Srcset`); for every other attribute item 2 (`CharacterReference`), then items 3, 4.1 and 4.2 (`Scheme`). Item 2 and item 4.3 read numeric character references on the raw bytes (markup M-13: decimal or `x`/`X` hexadecimal, all digits, `;` optional, the value capped at `110000`), never on the copy of item 3. So `java&#x73⏎cript:x` rejects (the trap of the audit).

### 8.2 Fail closed

| Case | Result |
|---|---|
| Patched index without a block, range outside the document, overlapping patched `html` ranges | `Markup` (rule `internal`), with that block index |
| Scan stop (M-01 input limit, token stack limit, internal defect) before the end of the last patched `html` block | `Markup` with the error of the scan and the first patched `html` block that the scan did not read to its end |
| Scan stop after the end of the last patched `html` block | No effect: the rule has read all patched content |
| Attribute walk with an internal defect | `Markup` |
| `data:` type part longer than the longest raster type (24 bytes) | `Scheme` (not exactly a raster type) |

<Badge type="tip" text="FACT" /> Resource bounds: one scan pass up to the end of the last patched `html` block; each attribute value is read a constant number of times, the `srcset` split included (tests `content_attr_srcset_is_linear`, `content_scope_large_content_is_read_in_one_pass`). Memory: the list of the patched `html` ranges and two name buffers, which hold a lower-case copy of a name with an ASCII upper-case letter only. `check_attribute` allocates nothing. No input panics (workspace lints: no indexing, no unchecked arithmetic). `ContentError` and `ContentViolation` hold the block index, the kind and, for `Markup`, the code and rule id of the scan: no content byte, tag name or attribute value in `Debug` or `Display` (test `content_error_holds_no_content`).

### 8.3 Tests

| Test | Checks |
|---|---|
| `content_scope_*` | What the rule reads: `json` blocks and blocks without an operation are not read; tags outside the content, the block start tag and a start tag at `end` are not content; a `title` block is RCDATA; the fail-closed cases of §8.2; the first violation in document order; the analysis of `split_rqp`; the pages `page-b-*.html` of `fixtures/rqp/protocol/pages` (block `price`) |
| `content_forbidden_*` | Item 1.1: each `PATCH_FORBIDDEN` name, upper case, self-closing, in a `template`; end tags do not matter; other names pass |
| `content_foreign_*` | Item 1.2: all 70 `FOREIGN_ALLOWED` names pass in `svg` and `math`; the names of §4 that are not in the set reject; the region root is in HTML context; the region ends with its root |
| `content_attr_*` | §4.2: event handlers (also a dropped duplicate), item 2 (every numeric value 00 to 100 in three forms, the cap, the six named references, the decision byte, the trap of the audit), items 3 and 4.1, item 4.2 (all raster types in the five allowed places, every rejected place and spelling), item 4.3 (separator references, the WHATWG split, every element) |
| `content_table_rows` | Each row of the result table of §4.2, also each tag of a row alone |
| `content_error_holds_no_content` | No content in `Debug` and `Display`; the code `PATCH_CONTENT_REJECTED` |
| `content_prop_no_missed_scheme` | P-CONTENT-1: a test-only model (character references, then the scheme steps of the URL Standard, and the WHATWG `srcset` split on the decoded value) finds a `javascript:`, `vbscript:` or forbidden `data:` URL only in values that `check_attribute` rejects (20 000 cases in 12 places; a guard test checks that the generator reaches such values) |
| `content_prop_accepted_has_no_forbidden` | P-CONTENT-2: any content (pieces of tags or any bytes, any stack limit): the result equals an independent reading of the scan tokens; an accepted content has no forbidden start tag and no `on*` attribute |
| `content_prop_any_ranges_never_panic`, `content_prop_attribute_any_bytes` | Any document, ranges, indices and attribute bytes: no panic |

## 9. Patched document

<Badge type="tip" text="FACT" /> `rqp::patch` builds and checks the patched document: steps 8 and 9 of [manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply), checks 12 and 13 of the [diff engine](/engineering/core/diff-engine). It runs after `rivqen_proto::delta::verify_delta` (steps 3 to 7).

| Function | Content |
|---|---|
| `render_rqp(original, analysis, replacements, limits)` | The stored original with the content range `start..end` of each named block replaced. Blocks must be in strictly increasing order and their ranges must lie in the original, else `BadReplacement` (`PATCH_INVALID`). The output size is computed with checked arithmetic before the one allocation. Over `max_document_bytes`: `TooLarge` (`PATCH_TOO_LARGE`). The limit passes at its value and fails one byte over. No partial result on an error |
| `check_patch(original, stored, meta, delta, limits)` | The checks below, in this order. The result is a `PatchedDocument` (document, analysis, R′) or a `PatchError` for the first failed check |
| `compute::page_revision_of` (`rivqen-proto`) | The page revision from descriptors taken by reference from an iterator. Same value as `page_revision`; `RqpDocument::page_revision` uses it, so it does not clone the descriptors (DL-175) |

Order of the checks in `check_patch`:

| # | Check | Error | Code |
|---|---|---|---|
| 1 | `stored` is the analysis of the snapshot of `meta` (same template revision, same descriptors in order) | `Snapshot` | `INTEGRITY_ERROR` |
| 2 | Build the patched document; operation i replaces block `delta.patched()[i].block` with the UTF-8 bytes of its content | `TooLarge` | `PATCH_TOO_LARGE` |
| 3 | `split_rqp` of the result | `Markup` | `PATCH_CONTENT_REJECTED` |
| 4 | Template revision equal to `meta.template_revision` | `TemplateChanged` | `PATCH_CONTENT_REJECTED` |
| 5 | Same blocks (`id`, `format`, order) | `BlocksChanged` | `PATCH_CONTENT_REJECTED` |
| 6 | Page revision of the result with `meta.policy` and `meta.limits` equals R′ of the delta | `PageRevisionDiffers` | `INTEGRITY_ERROR` |
| 7 | `check_patch_content` on the block indices of `delta.patched()` (step 9) | `Content` | `PATCH_CONTENT_REJECTED` |

A delta that does not fit the stored document (a block index without a block) gives `Snapshot`. The id and the format of a block are in its start tag, so a changed block list with an equal template revision cannot occur; check 5 is a second guard. A content that closes its block early changes the template and gives `TemplateChanged`. No error holds a document byte.

| Test (`rqp_patch`) | Checks |
|---|---|
| `patch_render_*` | Replacement, order, missing block, range outside the original, size limit at the value and one over, error codes |
| `patch_check_accepts_json_patch`, `patch_check_uses_operation_content`, `patch_check_applies_each_operation_to_its_own_block` | The accept path and the content bytes |
| `patch_check_order_*`, `patch_check_codes`, `patch_check_page_revision_*`, `patch_check_early_close_changes_the_template` | Order of the checks and the codes |
| `patch_check_markup_*` | Cases of [§4.1](/engineering/protocol/manifest-patch#_4-1-markup-analysis-of-the-patched-document) rejected in step 8 |
| `patch_check_html_operation_reaches_the_content_rule` | Step 9 wiring |
| `patch_errors_hold_no_content` | No content byte in `Display` or `Debug` of an error |
| `patch_prop_render_keeps_template`, `patch_prop_page_revision_of_equals_page_revision` | Render keeps the template and the block ids; each patched block has the hash of its new content; `page_revision_of` equals `page_revision` |
