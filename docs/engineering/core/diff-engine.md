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

A patch passes only if **all** checks pass. The checks run in this order. The first failure stops the process.

| # | Check | On failure |
|---|---|---|
| 1 | Envelope size ≤ `limits.maxPatchBytes`; JSON depth ≤ 16; operation count ≤ limit | Reject; `PATCH_TOO_LARGE` |
| 2 | Schema valid (`type`, `protocol`, required fields) | Reject; `PROTOCOL_ERROR` |
| 3 | `origin`, `page_id`, partition match the current navigation | Reject; `SECURITY_VIOLATION` |
| 4 | `template_revision` equals the current template revision | Full reload; `TEMPLATE_MISMATCH` |
| 5 | `base_revision` equals the current page revision | Request snapshot; `PATCH_BASE_MISMATCH` |
| 6 | `sequence` = last + 1 on this channel (duplicates are idempotent no-ops) | Gap → resync over HTTPS |
| 7 | Every `block_id` exists in the manifest (or is a declared insert) | Reject; `PROTOCOL_ERROR` |
| 8 | Each block's SHA-256 over canonical bytes equals the declared hash | Reject; `INTEGRITY_ERROR` |
| 9 | Content policy: HTML blocks pass the trusted-insertion policy; no `<script>`, no inline event handlers unless the host policy allows them | Reject; `SECURITY_VIOLATION` |

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
