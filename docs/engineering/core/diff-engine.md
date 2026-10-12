# Diff engine

The diff engine (`rivqen-diff`) computes which data blocks changed, validates incoming patches, and prepares an atomic apply plan. It never applies a patch that it did not validate.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 1. Two diff paths

| Path | Protocol | Who computes the diff | What the client receives |
|---|---|---|---|
| **legacy data diff** | Legacy protocol | Server sends **all** data blocks; client compares them with cached blocks | JSON map of block key → HTML (see [legacy wire contract](/engineering/protocol/legacy-wire)) |
| **RQP patch** | Rivqen protocol | Server sends only changed blocks in a `PatchEnvelope` | Typed operations with base and next revision |

## 2. legacy data diff

<Badge type="tip" text="FACT" /> In upstream legacy, a data-only response carries the data blocks, and the client computes which blocks differ from its cache before it sends the changed blocks to the page. The exact behavior and edge cases are in [legacy client behavior](/engineering/protocol/legacy-client).

Rivqen procedure:

1. Parse the response body as JSON within the size limit.
2. Check that the `template-tag` of the response equals the cached `template-tag`. If not, select `TemplateChanged`.
3. For each block key in the response, compare the value with the cached value byte for byte.
4. Collect the changed keys in document order.
5. Build the new snapshot: cached template + merged data.
6. Verify the integrity value of the new HTML if the server sent one (legacy `html-sha1`).
7. If any step fails, discard the result and select `Fallback(ProtocolError)`.

## 3. RQP patch validation

<Badge type="info" text="DESIGN" /> Normative rules: [RQP manifest and patch](/engineering/protocol/manifest-patch) §2.4 (client verification), §3.2 (JSON rules) and §4 (content rules). The HTTP envelope has no `page_id` and no `sequence` (H-9); a push channel (WP-16) checks its own channel fields before step 1.

A patch passes only if **all** checks pass. The checks run in this order. The first failure stops the process. On any failure the engine changes neither the page nor the snapshot, and the session loads the full document over HTTPS.

| # | Check | Code |
|---|---|---|
| 1 | The response belongs to the current navigation: same generation and request ([INV-12](/engineering/architecture/invariants)); the final response URL (after redirects) has the origin and path of the snapshot | Stale: no-op. Other URL: `PATCH_INVALID` |
| 2 | The partition did not change between the request and the response | `AUTH_PARTITION_CHANGED` |
| 3 | Envelope bytes ≤ `min(maxPatchBytes, limits.max_patch_bytes)`, counted while the body is read (after content decoding) | `PATCH_TOO_LARGE` |
| 4 | JSON rules: UTF-8, no duplicate member names, no lone surrogate escape, integer form, depth ≤ `maxJsonDepth` | `PATCH_INVALID` |
| 5 | Schema `urn:rivqen:rqp:1:patch`: `type`, `protocol` `"rqp/1"`, revision patterns, 1 to 256 operations, operation members | `PATCH_INVALID` |
| 6 | `template_revision` equals the snapshot template revision | `TEMPLATE_MISMATCH` |
| 7 | `base_revision` equals the snapshot page revision | `PATCH_BASE_MISMATCH` |
| 8 | Each `block_id` is a block of the snapshot; at most one operation per block; operations in document order; `format` equals the block format; `remove_block` only for `html` blocks | `PATCH_INVALID` |
| 9 | Per operation: UTF-8 bytes of `content` ≤ `maxBlockBytes`, then `B64U(SHA-256(UTF-8(content)))` equals `sha256` | `PATCH_TOO_LARGE`, `INTEGRITY_ERROR` |
| 10 | Recompute the page revision from the snapshot template revision and the new block hashes (the full snapshot hash for each block without an operation); it equals `next_revision` | `INTEGRITY_ERROR` |
| 11 | Content rules: the §4 rules of the manifest and patch page; the new document ≤ `maxDocumentBytes` | `PATCH_CONTENT_REJECTED`, `PATCH_TOO_LARGE` |

Step 10 is what makes the 64-bit block digest prefixes of the request safe (H-44): a changed block that the server omits keeps its old full hash in the recomputation, so the revision differs and the client loads the full document. `PATCH_INVALID` and `PATCH_CONTENT_REJECTED` are proposed codes for the [error catalog](/engineering/core/errors) (WP-17 S2, T-02).

Bounds: steps 3 to 5 read at most `maxPatchBytes` bytes; steps 8 to 10 do work linear in the envelope size and in the number of blocks (≤ 256); step 11 is one markup pass over at most `maxDocumentBytes` when the re-check of the manifest and patch page §4 is adopted (open question Q-T02-4).

## 4. Apply plan

A validated patch becomes a `ValidatedPatch`. It contains:

- The list of block replacements in document order.
- The new `page_revision`.
- A `CacheTransaction` for the new snapshot.

The adapter delivers the patch to the web SDK **and** commits the cache. If the web SDK reports a failure, the core invalidates the new snapshot and selects a full reload. The page and the cache are never left in different revisions after the session ends.

## 5. Properties (tested)

| ID | Property |
|---|---|
| P-DIFF-1 | `apply(rev_n, patch(n → n+1)) == rev_(n+1)` |
| P-DIFF-2 | A patch with `base ≠ current` changes no byte of the committed snapshot. |
| P-DIFF-3 | Applying the same patch twice gives the same result as applying it once. |
| P-DIFF-4 | For any malformed input, the engine returns an error within the time and memory budget. It never panics. |
| P-DIFF-5 | legacy: `render(template, extract(html)) == html` for every document that the legacy grammar accepts. |

## Related

- [Template engine](/engineering/core/template-engine)
- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Security controls SEC-09, SEC-17, SEC-20](/engineering/security/controls)
