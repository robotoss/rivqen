# RQP markup parser candidates — contract

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

This contract is fixed by the architect for WP-17 S1. Lanes do not change it. Request a change in the task report.

Normative rules for markup validity and block extraction: `docs/engineering/protocol/markup.md` (rqp/1). This file defines only the interface that the differential runner uses.

## 1. Candidates

| Name | Path | Language | HTML library |
|---|---|---|---|
| `rust` | `tools/rqp-markup/rust/` | Rust | Own tokenizer, no HTML library (DL-012, DL-013) |
| `node` | `tools/rqp-markup/node/` | JavaScript (Node.js ≥ 22, ESM) | Own tokenizer, no runtime dependency; `parse5` 8.0.1 (MIT) is a test oracle only (DL-012, DL-016) |
| `java` | `tools/rqp-markup/java/` | Java 21 | Own tokenizer, no runtime dependency (DL-012, DL-014) |
| `php` | `tools/rqp-markup/php/` | PHP ≥ 8.3 | Own tokenizer, no runtime dependency (H-13, DL-015) |

Each candidate directory has a `README.md` with the build command and the test command. The decisions DL-012…DL-016 are in `docs/engineering/plan/decision-log.md`. DL-012 allows a library tokenizer with exact source positions (Allowed license class) or an own tokenizer; all four candidates use an own tokenizer.

## 2. Command line

Each candidate is a command with two modes:

```text
<cmd> <path-to-input.html>
<cmd> --batch <fixtures-dir>
```

1. Single mode: read the file as bytes. Print one JSON object (section 3) and a line feed to stdout.
2. Batch mode: for each subdirectory of `<fixtures-dir>` in byte order of its name, read `<subdir>/input.html`. Print one JSON line per fixture: the object of section 3 plus the field `"fixture": "<subdir name>"`. Skip a subdirectory without `input.html`.
3. Exit code `0` when results are printed, for valid and invalid documents.
4. Exit code `2` for a usage error or an I/O error. Print the reason to stderr.
5. No input may cause a crash, a hang or an exit code other than `0` in batch mode or single mode. A crash on any input is a defect.
6. Never print document content to stderr.

## 3. Result object

Valid document:

```json
{
  "valid": true,
  "error": null,
  "blocks": [
    { "id": "price", "format": "html", "start": 120, "end": 126, "sha256": "<base64url>" }
  ],
  "template_revision": "t1.<base64url>",
  "page_revision": "r1.<base64url>"
}
```

Invalid document:

```json
{ "valid": false, "error": "RQP_MARKUP_DUPLICATE", "blocks": [], "template_revision": null, "page_revision": null }
```

| Field | Rule |
|---|---|
| `blocks` | Document order (ascending `start`) |
| `start`, `end` | **UTF-8 byte offsets** into the input file. `start` = first byte after the block's start tag. `end` = first byte of the block's end tag. Content = bytes `[start, end)`. |
| `sha256` | base64url without padding (RFC 4648 §5) of SHA-256 over the content bytes. 43 characters. |
| `error` | One code from section 4, or `null` |

## 4. Hashes and revisions (rqp/1)

`B64U(x)` = base64url without padding. `||` = byte concatenation. Strings are UTF-8.

1. `block.sha256 = B64U(SHA-256(content))`
2. `template` = the input bytes with the byte range `[start, end)` of every block removed. The start and end tags stay.
3. `template_revision = "t1." || B64U(SHA-256("rqp-t1\n" || template))`
4. `P = "rqp-r1\n" || template_revision || "\n"`, then for each block in document order: `id || "\t" || format || "\t" || sha256 || "\n"`
5. `page_revision = "r1." || B64U(SHA-256(P))`

A UTF-8 byte order mark at the start of the input is part of the input bytes and of the template.

## 5. Error codes

| Code | Meaning (details in `markup.md`) |
|---|---|
| `RQP_MARKUP_ENCODING` | Input is not valid UTF-8 |
| `RQP_MARKUP_LIMIT` | A limit is exceeded: input > 5 MiB, > 256 blocks, or a block > 1 MiB |
| `RQP_MARKUP_INVALID_ID` | `data-rq-block` value does not match `^[a-z0-9][a-z0-9_-]{0,63}$` |
| `RQP_MARKUP_FORBIDDEN_ELEMENT` | `data-rq-block` on an element that cannot be a block |
| `RQP_MARKUP_DUPLICATE` | Two blocks with the same id |
| `RQP_MARKUP_NESTED` | A block element inside a block |
| `RQP_MARKUP_STRUCTURE` | Block content or context breaks the structure rules (no explicit end tag, implied close, unbalanced tags, forbidden content element) |
| `RQP_MARKUP_BAD_JSON` | A `json` block is not valid JSON (RFC 8259) |
| `RQP_MARKUP_RESERVED` | The input already contains a manifest script (`type="application/rivqen-manifest+json"`) |

When a document has more than one error, a candidate may report any of them. Fixtures with an error contain exactly one error. The differential runner compares the error code only on fixtures.

The codes above are results for a document (exit code `0`). There is no error code for a defect in a candidate.

**Note: exit codes for internal defects.** An exit code other than `0` and `2` means an internal defect of the candidate (section 2, item 5). The differential runner reports every exit code other than `0` as `CRASH` for that candidate. The code value is not part of this contract. The candidates use these values today; both `3` and `70` are accepted:

| Candidate | Exit code for an internal defect |
|---|---|
| `rust` | None of its own. Panics are denied by lints; a panic would end the process with the Rust runtime code `101`. |
| `node` | `70` |
| `java` | `3` |
| `php` | `70` |

A candidate that finds a defect for one input can still print a fail-closed result for it (`node` prints `RQP_MARKUP_STRUCTURE`). The runner reports `CRASH` all the same, because the exit code is not `0`.

## 6. Differential runner

`tools/rqp-markup/diff.mjs` reads `tools/rqp-markup/candidates.json`, runs every candidate in batch mode on `fixtures/rqp/markup/`, and compares each result with `<fixture>/expected.json`:

- `valid`, `error`, `template_revision`, `page_revision`: exact.
- `blocks`: exact, element by element.

It prints a matrix (fixture × candidate) and exits non-zero on any difference.
