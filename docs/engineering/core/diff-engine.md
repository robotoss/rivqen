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

Rivqen procedure:

1. Parse the response body as JSON within the size limit.
2. Check that the `template-tag` of the response equals the cached `template-tag`. If not, select `TemplateChanged`.
3. For each block key in the response, compare the value with the cached value byte for byte.
4. Collect the changed keys in document order.
5. Build the new snapshot: cached template + merged data.
6. Verify the integrity value of the new HTML if the server sent one (legacy `html-sha1`).
7. If any step fails, discard the result and select `Fallback(ProtocolError)`.

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
| 12 | Apply the operations to a **copy** of the snapshot document; the result ≤ `maxDocumentBytes`. Run the markup analysis on it (H-48): valid for RQP, same template revision, same blocks (`id`, `format`, order); then its page revision, computed with the policy line of the snapshot manifest, equals R′ of check 11 | `PATCH_TOO_LARGE`; `PATCH_CONTENT_REJECTED`; `INTEGRITY_ERROR` |
| 13 | Patch content rule on each patched `html` block (manifest-patch §4 item 1, H-78): no element of `PATCH_FORBIDDEN`, no foreign element outside `FOREIGN_ALLOWED`; attribute rule on every start tag (`on*` names; `javascript:` and `vbscript:` after the URL normalization, H-59; `data:` except raster images in `img`/`source`/`poster`, H-83; a character reference at the scheme decision byte, H-77) | `PATCH_CONTENT_REJECTED` |

Step 11 is what makes the 64-bit block digest prefixes of the request safe (H-44): a changed block that the server omits keeps its old full hash in the recomputation, so R′ and the binding differ and the client loads the full document. Since H-104, H-107 and H-108 it is also the only check of the template, the base, the URL, the request and the contents: equal binding values mean equal U, B, N and R′ unless SHA-256 has a collision, and equal R′ means equal page revision preimages. So a delta for another template, for another base (also an earlier base: a rollback; the base of another user; the page that the snapshot already is: a stale replay), for another URL, for another request with the same base and URL (another nonce: a replay after A → B → A, or the delta of another user with the same `public` base), or with content that the server did not hash, fails here with `INTEGRITY_ERROR` ([manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply), "Why step 7 is sufficient"). Under H-104 alone (before H-107) a delta whose operations covered every block that differed passed step 11 on any snapshot of the template: the S6 security review (F1) showed rollback, stale pinning, and cross-user and cross-URL content through a cache that ignores `no-store`; H-107 closes these cases. Under H-107 alone (before H-108) a delta still passed for every request with the same U and B (S6 security re-review B1 and B2); the nonce of H-108 closes them, under the assumptions of [manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields) ("Replay": an honest server, TLS, a CSPRNG nonce). Step 12 checks the page revision of the patched copy against R′, on its bytes, and gives the block offsets of the new snapshot. Only after step 13 does the engine build the `ValidatedPatch` (section 4). Codes: [error catalog](/engineering/core/errors) (H-50). Steps 12 and 13 give no script element, no inline event handler and no attribute value that starts with `javascript:`, `vbscript:` or `data:` after URL normalization in patched content (DL-036). They are not an HTML sanitizer: later URLs in `srcset` or `ping`, `url()` in `style`, app-specific schemes, DOM clobbering, CSS overlays and script gadgets of a framework are not covered ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)); CSP ([SEC-04](/engineering/security/controls)) and a bridge that URL-scheme navigation cannot reach ([SEC-02](/engineering/security/controls)) control them.

A `304` is not a patch, but it confirms a snapshot as the current page, so the session binds it the same way (H-58; [negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), item 1): the request sent `If-None-Match` with the revision of the snapshot, the final URL has the cache key of the snapshot, and the `ETag` of the `304`, when present, matches that revision by weak comparison. Else the snapshot stays unchanged and the WebView loads the URL normally. Check 3 applies to a `304` too (DL-036): when the partition changed between the request and the response (for example a logout during the revalidation), the session stops with `AUTH_PARTITION_CHANGED`, does not show the snapshot, and the WebView loads the URL normally in the new partition. A `304` on a snapshot in the shared `public` partition keeps that snapshot as it is, in `public` (H-65): a `304` has no content, so no content of this request enters the shared partition.

The session uses a delta, a `304` or a full document only when the final request carried RQP fields: the origin of the final URL is in the allowlist (exact match of the serialized origin, DL-038), the final URL has no user name and no password (H-68), and the build allows its scheme (H-62). The origin comes from the URL object that the network stack used for the final request (DL-040). A delta or a `304` from an origin outside the allowlist never has the cache key of the snapshot, so check 1 rejects it.

Bounds: steps 4 to 6 read at most `maxPatchBytes` bytes; steps 9 to 11 do work linear in the envelope size and in the number of blocks (≤ 256): at most 256 SHA-256 computations over at most `maxPatchBytes` content bytes in total, one over a page revision preimage of at most about 30 KiB, and one over a binding preimage of the URL length plus 125 bytes; step 12 is one markup pass over at most `maxDocumentBytes` and one copy of at most that size; step 13 reads only the patched content, which step 12 has already tokenized.

## 4. Apply plan

A validated patch becomes a `ValidatedPatch`. It contains:

- The list of block replacements in document order.
- The new `page_revision`: R′ of check 11, which the client computed (H-107; the envelope has no page revision).
- A `CacheTransaction` for the new snapshot, with its cache key: the canonical URL of the snapshot and the partition below.

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

## Related

- [Template engine](/engineering/core/template-engine)
- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Security controls SEC-09, SEC-17, SEC-20](/engineering/security/controls)
