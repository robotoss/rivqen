# Diff engine

The diff engine (`rivqen-diff`) computes which data blocks changed, validates incoming patches, and prepares an atomic apply plan. It never applies a patch that it did not validate.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 1. Two diff paths

| Path | Protocol | Who computes the diff | What the client receives |
|---|---|---|---|
| **legacy data diff** | Legacy protocol | Server sends **all** data blocks; client compares them with cached blocks | JSON map of block key → HTML (see [legacy wire contract](/engineering/protocol/legacy-wire)) |
| **RQP patch** | Rivqen protocol | Server sends only changed blocks in a `PatchEnvelope` | `binding` and operations (`block_id`, `content`); the client sends `Rq-Have` digest prefixes and checks `binding` with the next page revision that it computes from the hashes, its snapshot revision, the final URL and the nonce that its request sent (H-44, H-47, H-104, H-107, H-108) |

## 2. legacy data diff

<Badge type="tip" text="FACT" /> In upstream legacy, a data-only response carries the data blocks, and the client computes which blocks differ from its cache before it sends the changed blocks to the page. The exact behavior and edge cases are in [legacy client behavior](/engineering/protocol/legacy-client).

Normative rules: [legacy client](/engineering/protocol/legacy-client#_9-5-outcomes) CL-27 (data update), CL-28 (empty diff), CL-29 (rebuild check) and CL-42 (diff); DL-160, DL-169 (S1-5).

<Badge type="info" text="DESIGN" /> Rivqen procedure for a data update. The session (WP-08 S2) runs steps 1, 3 and 6; the module `rivqen_diff::legacy` (feature `legacy`) runs steps 2, 4 and 5 in `Rebuilt::new`, which does all three checks of CL-29. The first failure stops the procedure.

1. The response passed the order of the tests of CL-13: a legacy response, a visible entry, and `template-change` `false` or `0`. Another `template-change` value is a template change (CL-31), not a data update.
2. `Rebuilt::new` first compares the first `template-tag` line of the response with the stored template tag (CL-29). The stored tag is the value of the cache record; the session checked it at the commit (CL-25), and `Rebuilt::new` does not compute it from the template. The comparison is exact text equality, and an empty tag never matches. Else the update fails (step 7).
3. The body is a JSON object with a `data` member that is an object of string values, read in text order within the limits (CL-29; `rivqen-proto-legacy`, WP-08 S1 T-01). A value that is not a string fails the update (the token `value-not-string` of CL-42).
4. `Rebuilt::new`, after step 2: rebuild the page from the stored template and the **full** server data with `render_v1`, within `TemplateLimits`. The lowercase hex SHA-1 of the page must equal the `html-sha1` member **and** the bare response etag (CL-29). Both values are required; an absent value never matches.
5. `diff_v1` of the stored data and the server data: each server entry whose value differs from the stored value, or whose key the stored data does not have, in **server order** (CL-42, D-25). A stored key that the server data does not have is not in the diff.
6. Deliver and commit (CL-27): an empty diff sends no message and the final code is `304` (CL-28). The new entry has the rebuilt page, the full server data (D-35), the unchanged template, and the etag and template tag of the response.
7. On any failure: discard the result, remove the stored entry, do not commit, and load the page normally; codes `-1 / -1` (CL-29, P-13).

<Badge type="tip" text="FACT" /> Corrections of the earlier procedure of this section (WP-08 S1): a `template-tag` mismatch is a failed update (CL-29), not a template change; the diff is in server order, not document order, and includes new keys; the new page uses the full server data, not merged data; the `html-sha1` check and the etag check are both required; a failure removes the entry and gives a normal load, not `Fallback(ProtocolError)`.

### 2.1 Functions and types

<Badge type="tip" text="FACT" /> The module `rivqen_diff::legacy` (feature `legacy`, default on; WP-23 removes it). The data list type is `rivqen_template::legacy::DataBlock` (key with braces, value).

| Item | What it does |
|---|---|
| `diff_v1(local, server)` | `local` is `Option<&[DataBlock]>`; `None` gives the whole server list. Gives references to the server entries of the diff, in server order. Keys compare byte for byte. A repeated key is `DiffError::DuplicateLocalKey` (code `CACHE_CORRUPTED`) or `DiffError::DuplicateServerKey` (`INTEGRITY_ERROR`), never a choice of one value |
| `RebuildInput` | The stored template and data, the server data (owned), `html_sha1`, the bare `etag`, `response_template_tag` and `stored_template_tag` |
| `Rebuilt::new(input, limits)` | Steps 2, 4 and 5 in this order: the template tag comparison, `render_v1`, the `html-sha1` comparison, the etag comparison, `diff_v1`. All three checks of CL-29. The only constructor of `Rebuilt` (R-CORE-05) |
| `Rebuilt` | `page`, `etag` (`Sha1Hex`), `data` (the full server data), `diff` (copies of the diff entries), `into_parts` (`RebuiltParts`) |
| `RebuildError` | `TemplateTagMismatch`, `Render(RenderError)`, `HtmlSha1Mismatch`, `EtagMismatch`, `Diff(DiffError)`. Codes: `DOCUMENT_TOO_LARGE` for a limit, `CACHE_CORRUPTED` for a repeated stored key, else `INTEGRITY_ERROR` |

<Badge type="tip" text="FACT" /> The tag and hash comparisons are plain text comparisons of public values: an uppercase, quoted, `W/` or padded value does not match. The page message of a data update takes its diff only from a `Rebuilt` (CL-27; the session rule of S2), so data that did not pass the three checks never reaches the page. `Debug` of `RebuildInput`, `Rebuilt` and `RebuiltParts` gives lengths and counts only; the error texts are static and have no key, value, page byte or hash (tests `legacy_rebuilt_debug`, `legacy_rebuilt_errors_have_no_content`).

<Badge type="tip" text="FACT" /> A `DataBlock` value is always a string, so the token `value-not-string` of CL-42 cannot reach `diff_v1`: the body reader rejects such a body. The golden test makes the token when it converts a JSON object of a fixture to a data list.

**Bounds.** <Badge type="tip" text="FACT" /> `diff_v1` takes `O(n log n)` key comparisons and allocates one map of references and the result list. `Rebuilt::new` calls `render_v1` first, which applies `TemplateLimits` (template and page at most `max_document_bytes`, at most `max_blocks` blocks plus `{title}`, each block value at most `max_block_bytes`); every value that it accepts is in the page, so the diff copy is at most the page size. Then one SHA-1 pass over the page.

### 2.2 Tests

<Badge type="tip" text="FACT" /> Tests in `crates/rivqen-diff/tests/`:

| Test | What it shows |
|---|---|
| `legacy_fn_diff` | The 24 canonical function fixtures `diff` pass, 2 of them with the token `value-not-string`; the order of each JSON object comes from the file text |
| `legacy_rebuilt_*` | A data update, an empty diff, a key that the stored data does not have, each mismatch, the template tag check (equal, different, empty, before the render), the order of the hash checks, exact comparison, data that does not fit the template, each limit, a repeated stored key, the error codes and texts, `Debug` |
| `legacy_rebuilt_fixture_pages` | The split of each fixture page that the server splits (50 page and 17 malformed fixtures) as the stored entry and the server data, with the page hash: 66 give the page byte for byte; `FX-LEG-MAL-PLACEHOLDER-TEXT` (key text before its block, DL-166) gives an error |
| `legacy_prop_diff_*` | `diff_v1(x, x)` is empty; the diff applied to the stored data gives every server value; the diff is exactly the changed entries in server order; repeated keys give the error |
| `legacy_prop_rebuilt_*` | P-DIFF-5 through `Rebuilt`; a data update gives the new page and the changed entries; never another page; any input gives no panic |

<Badge type="tip" text="FACT" /> P-DIFF-5 holds through `Rebuilt` on the domain of DL-166 and DL-167 (each placeholder is the first occurrence of its key, and the title is outside the blocks). Outside that domain `Rebuilt::new` gives an error, never another page. Fuzz target `diff_v1` (`fuzz/fuzz_targets/diff_v1.rs`): two data lists and a template from the input; its header lists the invariants.

## 3. RQP patch validation

<Badge type="info" text="DESIGN" /> Normative rules: [RQP manifest and patch](/engineering/protocol/manifest-patch) §2.4 (client verification), §3.2 (JSON rules) and §4 (content rules); [RQP negotiation](/engineering/protocol/negotiation) §6.3 (client response handling). The HTTP envelope has no `page_id` and no `sequence` (H-9), so there is no sequence check in this list; a push channel (WP-16) checks its own channel fields before step 1. The only operation replaces the content of a block (H-47). Since H-104 and H-107 the envelope is `binding` and the operations (`block_id`, `content`): there is no template revision, base revision, page revision or hash in it, so checks 7 and 8 are removed and check 11 is the only integrity check of the envelope. `binding` is a SHA-256 value over the canonical final URL U, the base page revision B and the next page revision R′ ([manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields)); the client computes all three inputs itself. The snapshot values (template revision, block hashes) are the ones that the client computed with its markup analysis and checked through the page revision of the manifest (H-103).

A patch passes only if **all** checks pass. The checks run in this order. The first failure stops the process. On any failure the engine changes neither the page nor the snapshot, and the session does one fallback load of the full document over HTTPS ([negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), item 5).

| # | Check | Code |
|---|---|---|
| 1 | The response belongs to the current navigation: same generation and request ([INV-12](/engineering/architecture/invariants)); the final response URL (after redirects) has the cache key of the snapshot: scheme, host, port, path, query ([cache identity](/engineering/cache/identity)); the partition is check 3 (H-58). A release build sends RQP fields only to `https`, so this check also rejects a delta from a final URL that is not `https` | Stale: no-op. Other URL: `PATCH_INVALID` |
| 2 | The request offered a delta: it had a snapshot, `block-patch`, the delta media type in `Accept` and a nonce in `Rq-Nonce` (H-108, H-109). Else the delta is unsolicited (also on the fallback load) | `PATCH_INVALID` |
| 3 | The partition did not change between the request and the response | `AUTH_PARTITION_CHANGED` |
| 4 | Envelope bytes ≤ `min(maxPatchBytes, limits.max_patch_bytes)`, counted while the body is read (after content decoding) | `PATCH_TOO_LARGE` |
| 5 | JSON rules: UTF-8, no duplicate member names, no lone surrogate escape, integer form, depth ≤ `maxJsonDepth` | `PATCH_INVALID` |
| 6 | Schema `urn:rivqen:rqp:1:patch`: exactly `binding` and 1 to 256 operations, each exactly `block_id` and `content`; an unknown member (also `type`, `protocol`, `base_revision`, `template_revision`, `op`, `format`, `sha256`, and `next_revision` of H-104) fails (H-104, H-107) | `PATCH_INVALID` |
| 7 | *Removed by H-104* (was: `template_revision` equals the snapshot template revision, `TEMPLATE_MISMATCH`). A delta for another template fails check 11 | — |
| 8 | *Removed by H-104* (was: `base_revision` equals the snapshot page revision, `PATCH_BASE_MISMATCH`). A delta for another base fails check 11: the snapshot page revision is B of the binding (H-107) | — |
| 9 | Each `block_id` is a block of the snapshot; at most one operation per block; operations in document order | `PATCH_INVALID` |
| 10 | Per operation (step 6 already limits `content` to 1 048 576 code points, so only multi-byte content reaches this byte check; with the 256 KiB envelope limit neither case is reachable): UTF-8 bytes of `content` ≤ `maxBlockBytes`; then compute `B64U(SHA-256(UTF-8(content)))`, the new block hash | `PATCH_TOO_LARGE` |
| 11 | Compute the next page revision R′ from the snapshot template revision, the policy line of the snapshot manifest (H-55) and the new block hashes (the hash of check 10 for each block with an operation, the full snapshot hash for each other block, the format of the snapshot for every block). Compute the binding of U = the canonical final URL, B = the snapshot page revision, N = the nonce that the request of this response sent and R′ (H-107, H-108); it equals `binding`. An input that the client cannot compute (no final URL that parses, no snapshot URL, no nonce) is a mismatch, never an exception | `INTEGRITY_ERROR` |
| 12 | Apply the operations to a **copy** of the snapshot document; the result ≤ `maxDocumentBytes`. Run the markup analysis on it (H-48): valid for RQP, same template revision, same blocks (`id`, `format`, order); then its page revision, computed with the policy line of the snapshot manifest, equals R′ of check 11 | `PATCH_TOO_LARGE`; `PATCH_CONTENT_REJECTED`; `INTEGRITY_ERROR`. Functions: `rqp::patch::render_rqp`, `check_patch` ([template engine §9](/engineering/core/template-engine#_9-patched-document)) |
| 13 | Patch content rule on each patched `html` block (manifest-patch §4 item 1, H-78): no element of `PATCH_FORBIDDEN`, no foreign element outside `FOREIGN_ALLOWED`; attribute rule on every start tag (`on*` names; `javascript:` and `vbscript:` after the URL normalization, H-59; `data:` except raster images in `img`/`source`/`poster`, H-83; a character reference at the scheme decision byte, H-77). Function: `rqp::content::check_patch_content`, called by `rqp::patch::check_patch` | `PATCH_CONTENT_REJECTED` |

Step 11 is what makes the 64-bit block digest prefixes of the request safe (H-44): a changed block that the server omits keeps its old full hash in the recomputation, so R′ and the binding differ and the client loads the full document. Since H-104, H-107 and H-108 it is also the only check of the template, the base, the URL, the request and the contents: equal binding values mean equal U, B, N and R′ unless SHA-256 has a collision, and equal R′ means equal page revision preimages. So a delta for another template, for another base (also an earlier base: a rollback; the base of another user; the page that the snapshot already is: a stale replay), for another URL, for another request with the same base and URL (another nonce: a replay after A → B → A, or the delta of another user with the same `public` base), or with content that the server did not hash, fails here with `INTEGRITY_ERROR` ([manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply), "Why step 7 is sufficient"). Under H-104 alone (before H-107) a delta whose operations covered every block that differed passed step 11 on any snapshot of the template: the S6 security review (F1) showed rollback, stale pinning, and cross-user and cross-URL content through a cache that ignores `no-store`; H-107 closes these cases. Under H-107 alone (before H-108) a delta still passed for every request with the same U and B (S6 security re-review B1 and B2); the nonce of H-108 closes them, under the assumptions of [manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields) ("Replay": an honest server, TLS, a CSPRNG nonce). Step 12 checks the page revision of the patched copy against R′, on its bytes, and gives the block offsets of the new snapshot. Only after step 13 does the engine build the `ValidatedPatch` (section 4). Codes: [error catalog](/engineering/core/errors) (H-50). Steps 12 and 13 give no script element, no inline event handler and no attribute value that starts with `javascript:`, `vbscript:` or `data:` after URL normalization in patched content (DL-036). They are not an HTML sanitizer: later URLs in `srcset` or `ping`, `url()` in `style`, app-specific schemes, DOM clobbering, CSS overlays and script gadgets of a framework are not covered ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)); CSP ([SEC-04](/engineering/security/controls)) and a bridge that URL-scheme navigation cannot reach ([SEC-02](/engineering/security/controls)) control them.

A `304` is not a patch, but it confirms a snapshot as the current page, so the session binds it the same way (H-58; [negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), item 1): the request sent `If-None-Match` with the revision of the snapshot, the final URL has the cache key of the snapshot, and the `ETag` of the `304`, when present, matches that revision by weak comparison. Else the snapshot stays unchanged and the WebView loads the URL normally. Check 3 applies to a `304` too (DL-036): when the partition changed between the request and the response (for example a logout during the revalidation), the session stops with `AUTH_PARTITION_CHANGED`, does not show the snapshot, and the WebView loads the URL normally in the new partition. A `304` on a snapshot in the shared `public` partition keeps that snapshot as it is, in `public` (H-65): a `304` has no content, so no content of this request enters the shared partition.

The session uses a delta, a `304` or a full document only when the final request carried RQP fields: the origin of the final URL is in the allowlist (exact match of the serialized origin, DL-038), the final URL has no user name and no password (H-68), and the build allows its scheme (H-62). The origin comes from the URL object that the network stack used for the final request (DL-040). A delta or a `304` from an origin outside the allowlist never has the cache key of the snapshot, so check 1 rejects it.

Bounds: steps 4 to 6 read at most `maxPatchBytes` bytes (`rqp::PatchLimits` caps it at 5 MiB); steps 9 to 11 do work linear in the envelope size and in the number of blocks (≤ 256): at most 256 SHA-256 computations over at most `maxPatchBytes` content bytes in total, one over a page revision preimage of at most about 30 KiB, and one over a binding preimage of the URL length plus 125 bytes; step 12 is one markup pass over at most `maxDocumentBytes` and one copy of at most that size; step 13 reads only the patched content, which step 12 has already tokenized.

<Badge type="tip" text="FACT" /> Checks 12 and 13 use functions of `rivqen-template` (WP-06, H-142): the build of the patched document (`render_rqp`), the markup analysis (`split_rqp`) and the patch content and attribute rule. `rivqen-diff` keeps the order of the checks (`rqp::validate`, below). <Badge type="info" text="DESIGN" /> The apply plan of section 4 (the partition and the cache transaction) is a session task (WP-08 S3).

**One call for checks 4 to 13 (WP-08).** <Badge type="tip" text="FACT" /> The module `rivqen_diff::rqp` runs checks 4 to 13 with one function. The module is not behind the feature `legacy`.

| Item | What it does |
|---|---|
| `validate(body, snapshot, request, limits)` | Calls `delta::verify_delta` (checks 4, 5, 6, 9, 10, 11), then `rqp::patch::check_patch` (checks 12 and 13), and stops at the first failure. The result is a `ValidatedPatch` (section 4) or a `ValidateError` |
| `StoredSnapshot` | The snapshot of the delta: the original document O, its markup analysis and the checked snapshot values (`SnapshotMeta`: T, B, the blocks, `policy`, `limits`). When the three do not belong together, check 12 fails closed (`INTEGRITY_ERROR` or `PATCH_CONTENT_REJECTED`), never with another page |
| `DeltaRequest` | U (the canonical final URL as text; `validate` does not parse it) and N (the nonce of the request). An absent U, an empty U or an absent N fails check 11 |
| `PatchLimits` | `maxPatchBytes` (capped at 5 MiB, the top of its safe range; a configured value can only lower it) and the `TemplateLimits` of check 12. `maxBlockBytes` of check 10 is the value of the `TemplateLimits`, so checks 10 and 12 use one value. `PatchLimits::try_from(&ResourceLimits)` builds it from the engine configuration |
| `ValidateError` | `Delta(DeltaError)` for checks 4 to 11, `Patch(PatchError)` for checks 12 and 13. `check()` gives the number of the table: 4, 5, 6, 9, 10, 11, 12 or 13 (`PatchError::Content` is 13; every other `PatchError` is 12, also `Snapshot`, the stored analysis that does not fit the snapshot values). The code is `PATCH_TOO_LARGE`, `PATCH_INVALID`, `INTEGRITY_ERROR` or `PATCH_CONTENT_REJECTED`. The texts have the check number, the code and a static reason with at most an operation index, a block index or a rule id |

<Badge type="tip" text="FACT" /> Tests in `crates/rivqen-diff/tests/`:

| Test | What it shows |
|---|---|
| `rqp_check_order_*` | One test per check; where the input also breaks a later check, the earlier check wins (4 before 5; 9 and 10 before 11; 11 before 13; in check 12 the size and the markup analysis before check 13). Each request input of check 11. The limit of check 4 is inclusive and capped. No error text, `Debug` text or source text has a block id, content, the URL or the nonce |
| `rqp_validated_*` | What a `ValidatedPatch` gives (section 4), the decoded JSON content, the form with an operation for every block (H-7), `policy` and `limits` of the base, a second patch on the result, `Debug` without content |
| `rqp_ledger_delta` | The 87 client fixtures with a `200` delta response: 26 patched; 56 rejected with the expected code (by check: 4: 1, 5: 1, 6: 4, 9: 3, 11: 13, 12: 6, 13: 28); 5 that checks 1 to 3 of the session reject (owner WP-08, S3) |
| `rqp_prop_*` | Properties P-DIFF-1 to P-DIFF-4 (section 5) |

**Checks in `rivqen-proto` (WP-05).** <Badge type="tip" text="FACT" /> Checks 4, 5, 6, 9, 10 and 11 of the table above are in the crate `rivqen-proto`. The session calls one function for all of them, `delta::verify_delta`, after checks 1 to 3 and before checks 12 and 13 (tests `delta_check_order_*`, `delta_prop_any_body`):

| # | Function | What it does |
|---|---|---|
| 4 to 11 | `delta::verify_delta` | Runs checks 4, 5, 6, 9, 10 and 11 in this order and stops at the first failure. Inputs: the decoded body, the client `maxPatchBytes`, the snapshot values (template revision, page revision B, blocks, `policy` and `limits`), the canonical final URL U and the nonce N of the request (both optional), the client `maxBlockBytes`. The result is a `VerifiedDelta` (the envelope, R′, the list L′ and the new hash of each operation, so checks 12 and 13 do not hash again), or a `DeltaError` with the check number and the code `PATCH_TOO_LARGE`, `PATCH_INVALID` or `INTEGRITY_ERROR`. The error text has no content, block id, URL or nonce |
| 4 | `envelope::parse_envelope` | Compares the body length with `envelope_limit(maxPatchBytes, limits)` before the JSON parse; a larger body is `PATCH_TOO_LARGE` |
| 5 | `json::Reader` (in `parse_envelope`) | Reads the body once, without recursion, with the rules of [manifest and patch §3.2](/engineering/protocol/manifest-patch#_3-2-json-text-of-the-manifest-and-the-envelope); a break is `PATCH_INVALID` with the rule, never with body bytes |
| 6 | `envelope::parse_envelope` | Checks the schema while it reads and stops at the first failure (also an unknown member or operation 257); it keeps only `binding` and at most 256 operations. Checks 5 and 6 share one pass, so the first failure in text order stops it; both are `PATCH_INVALID` |
| 9 | `compute::next_blocks` | The operations against the snapshot block list, for the whole list: a known block (the first block with the id), at most one operation per block, document order; else `PATCH_INVALID` |
| 10 | `compute::next_blocks` | For each operation: the UTF-8 bytes of `content` are at most `maxBlockBytes` (a parameter; else `PATCH_TOO_LARGE`), then `SHA-256(UTF-8(content))` replaces the hash of its block in L′. This is the only content size check |
| 11 | `compute::page_revision`, `compute::binding` (in `verify_delta`) | R′ from the template revision, L′ and the policy line of the snapshot; the binding of U, B, N and R′ equals `binding`. No U (absent or empty) or no N is a mismatch, never another code; else `INTEGRITY_ERROR` |

The manifest checks of [manifest and patch §1.8](/engineering/protocol/manifest-patch#_1-8-client-checks-of-the-manifest), items 1, 2 and 4, are `manifest::read_manifest` in the same crate.

## 4. Apply plan

A validated patch becomes a `ValidatedPatch`. It contains:

- The list of block replacements in document order.
- The new `page_revision`: R′ of check 11, which the client computed (H-107; the envelope has no page revision).
- A `CacheTransaction` for the new snapshot, with its cache key: the canonical URL of the snapshot and the partition below.

<Badge type="tip" text="FACT" /> `rivqen_diff::rqp::ValidatedPatch` (WP-08 S1) has no public constructor: only `validate` makes one, after checks 4 to 13 passed (R-CORE-05, INV-13). A failed check gives no value, and `validate` reads the snapshot only through shared references, so a failure changes nothing. It gives:

| Method | Value |
|---|---|
| `replacements()` | One `BlockReplacement` per operation, in document order (strictly increasing block index, check 9): the block index, the block id, the format of the block in the snapshot, and the decoded `content` string. Each content is the content of its block in `document()` |
| `document()` | The patched document: the original document O of the new snapshot |
| `analysis()` | The markup analysis of `document()`: the block offsets of the new snapshot |
| `page_revision()` | R′ (check 11), which the patched document has (check 12) |
| `base_revision()` | B, the page revision of the snapshot that the delta applies to |
| `snapshot_meta()` | The values of the new snapshot: the template revision of the base, R′, the blocks of `analysis()` (equal to L′), and the `policy` and `limits` of the base ([manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply), "Apply") |
| `into_parts()` | The replacements, the document, the analysis and the snapshot values, by value |

<Badge type="tip" text="FACT" /> `Debug` of `ValidatedPatch`, `BlockReplacement`, `StoredSnapshot` and `DeltaRequest` gives lengths, counts, indices and formats only: no content, no block id, no revision, no URL, no nonce (test `rqp_validated_debug_has_no_content`).

<Badge type="info" text="DESIGN" /> The `CacheTransaction`, its cache key and the partition below are not in `rivqen_diff::rqp::ValidatedPatch`: the session builds them from it (WP-08 S3). The payload of the action `ApplyDataPatch` in `rivqen-core` is still a placeholder.

<Badge type="info" text="DESIGN" /> **Partition of the new snapshot (H-65).** The engine selects the partition before it builds the `CacheTransaction`:

| Snapshot | Request and delta response | Partition of the new snapshot |
|---|---|---|
| In the partition of the request | Any | The partition of the request |
| In `public` | The client knows that the request had no credentials; the delta response did not come through a redirect (H-67); the native SDK saw all header fields of the delta response (H-64); no `Set-Cookie` or `Set-Cookie2`; no `Cache-Control: private`; the origin is declared public; the manifest policy of the snapshot has `"mode": "public"` | `public`: the transaction replaces the `public` snapshot |
| In `public` | Any other case (also unknown credentials, a redirect, and an explicit `Cache-Control: public` with credentials) | The partition of the request: a new entry; the `public` snapshot does not change |

Rules: [RQP manifest and patch §1.4](/engineering/protocol/manifest-patch#_1-4-policy), "Delta on a `public` snapshot", and RQP-25 of [negotiation](/engineering/protocol/negotiation#_8-rules). Reason: the server computes the changed blocks for this request, so with credentials the new content can be personal.

The adapter delivers the patch to the web SDK **and** commits the cache. The web SDK puts each patched `html` block into the page as [manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply) says ("DOM apply", H-102). When the adapter gives the patched document (or any snapshot) to the WebView as a document, it gives the bytes as `text/html` with the encoding `utf-8` and decodes them only as UTF-8; with an API that takes a string, it removes a leading UTF-8 byte order mark first (H-66, DL-040; [negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), "Delivery to the WebView"). If the web SDK reports a failure, the core invalidates the new snapshot and selects a full reload. The page and the cache are never left in different revisions after the session ends.

## 5. Properties (tested)

| ID | Property |
|---|---|
| P-DIFF-1 | `apply(rev_n, patch(n → n+1)) == rev_(n+1)` |
| P-DIFF-2 | A patch built for another base, another template, another URL or another request (its `binding` does not fit the snapshot, the final URL and the nonce of the request, H-104, H-107, H-108) changes no byte of the committed snapshot. Also a rollback (a patch n → n+1 on n+2, and on a snapshot whose page went A → B → A: the patch was built for an earlier request, so its nonce differs) and a patch whose operations cover every block that differs. The A → B → A case holds only with a nonce that the client never used before (H-108). |
| P-DIFF-3 | Applying the same patch twice gives the same result as applying it once: the second apply is on the snapshot n+1, its B is n, so it is rejected (`INTEGRITY_ERROR`, H-107) and changes nothing. |
| P-DIFF-4 | For any malformed input, the engine returns an error within the time and memory budget. It never panics. |
| P-DIFF-5 | legacy: `render(template, extract(html)) == html` for every document that the legacy grammar accepts. |

<Badge type="tip" text="FACT" /> P-DIFF-1 to P-DIFF-4 are property tests of `rqp::validate` in `crates/rivqen-diff/tests/rqp_props.rs` (fixed seeds; `PROPTEST_CASES` raises the case count). The generator makes documents of one template with 1 to 5 `html` and `json` blocks, with or without the manifest members `policy` and `limits`. A server delta is built as a server builds it: the operations carry the content of the next document, and R′ is the page revision of the analysis of the next document.

| Property | Test | What it checks |
|---|---|---|
| P-DIFF-1 | `rqp_prop_apply` | With the changed blocks or with every block (H-7): the result is the next document byte for byte, with its analysis and its snapshot values; the replacements are the operations in document order |
| P-DIFF-2 | `rqp_prop_other_base` | A delta whose operations cover every block, on a snapshot with another base, on its own target (a stale replay), on another template, with another URL or with another nonce (A → B → A, a `public` replay): check 11, `INTEGRITY_ERROR`, and the snapshot does not change |
| P-DIFF-3 | `rqp_prop_twice` | The same delta on the result of its first apply: check 11, `INTEGRITY_ERROR`; when n+1 equals n, the same document again |
| P-DIFF-4 | `rqp_prop_malformed_*` | Any body; a valid body with one byte changed, removed or added; a stored original with one byte changed; operations with any ids, order and content, with a binding that fits them or not. No panic; an error has one of the four codes and a check number of the table; a `ValidatedPatch` is always the base with the operation contents in their blocks (or the page of the server), never another page |

<Badge type="tip" text="FACT" /> The time and memory part of P-DIFF-4 follows from the bounds of section 3; no test measures it.

## Related

- [Template engine](/engineering/core/template-engine)
- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Security controls SEC-09, SEC-17, SEC-20](/engineering/security/controls)
