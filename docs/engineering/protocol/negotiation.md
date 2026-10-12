# RQP: negotiation and headers

The Rivqen protocol (RQP) has explicit versions, capabilities and revisions. This page defines how a client and a server agree on RQP, which headers they send, how the server selects the response (full document, delta or `304`), how caches handle the responses, and what the client checks.

**Status:** <Badge type="info" text="DESIGN" /> Normative for `rqp/1` from WP-17 S2. Decisions: `Rq-` header prefix and Structured Field syntax ([ADR-006](/engineering/architecture/adr/#adr-006)); the `dict-compression` token is reserved (H-4); the client sends its block digests, with an all-blocks fallback (H-7); content-derived revisions `t1.`/`r1.` (H-8); ASCII case-insensitive manifest type match (H-28); `Rq-Have` with 64-bit digest prefixes (H-44); caching of full document and delta (H-45); `If-None-Match` carries the client's page revision (H-46). Records: [WP-17 S1](/engineering/plan/sprints/WP-17-S1) §1 and [WP-17 S2](/engineering/plan/sprints/WP-17-S2) §1. Block markup: [RQP markup](/engineering/protocol/markup). Manifest, patch envelope and revision algorithm: [RQP manifest and patch](/engineering/protocol/manifest-patch).

[[toc]]

## 1. Goals

1. Explicit negotiation of version and capabilities.
2. Same result over HTTP/1.1, HTTP/2 and HTTP/3. Extra channels never affect correctness.
3. A stateless server: the request holds all the state that the server needs to select the response.
4. Atomic delivery of patches; rejection of stale patches by the client.
5. Correct behavior through HTTP caches without RQP knowledge.
6. Bounded work for every input: header sizes, block counts and response sizes have limits.

<Badge type="info" text="DESIGN" /> The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are used as in RFC 2119 and RFC 8174.

## 2. Terms

| Term | Meaning |
|---|---|
| Page bytes P | The HTML body that the application produced for the request, before RQP processing |
| Full document F | P with the page manifest inserted ([manifest and patch](/engineering/protocol/manifest-patch)). The one representation of a valid page |
| Delta D | A patch envelope that changes a client's page from its revision to the current revision |
| Template revision | `t1.…` of the page ([§3.7](#_3-7-revisions)) |
| Page revision | `r1.…` of the page. The strong ETag of F |
| Snapshot | A page that the client holds, with its manifest, for one cache key ([cache identity](/engineering/cache/identity)) |
| N | The number of blocks of the current page (0 to 256) |

## 3. Headers

### 3.1 Overview

RQP headers use **Structured Field Values** ([RFC 9651](https://www.rfc-editor.org/rfc/rfc9651)). RQP reuses the standard fields `If-None-Match`, `ETag`, `Accept`, `Content-Type` and `Cache-Control`.

| Header | Direction | Type |
|---|---|---|
| `Rq-Version` | both | Integer |
| `Rq-Capabilities` | request | List with one Inner List of Tokens |
| `Rq-Have` | request | Dictionary |
| `If-None-Match` | request | RFC 9110 |
| `Accept` | request | RFC 9110 |
| `ETag` | response | RFC 9110, strong |

<Badge type="tip" text="FACT" /> RFC 9651 §2.2: when parsing fails, the entire field is ignored; when a field-specific constraint is violated, the entire field is also ignored, unless the field definition defines other handling. <Badge type="info" text="DESIGN" /> In RQP, "ignored" means "absent" in the algorithm of section 4. Only `Rq-Have` member `b` has other handling ([§3.4](#_3-4-rq-have)).

### 3.2 `Rq-Version`

An Item, type Integer. Parameters are ignored.

- Request: the RQP major version that the client uses. `rqp/1` clients send `Rq-Version: 1`.
- Response: the server sends `Rq-Version: 1` on every full document and every delta of a valid page. It MAY send it on a `304`.

A server supports the request version only when the value is the Integer `1`. Any other value, a parse failure or no field means: the request is not an RQP request. The server still sends F (section 4); it never sends a delta for such a request.

### 3.3 `Rq-Capabilities`

The field value is a List with exactly one member: an Inner List of Tokens. Example: `Rq-Capabilities: (block-patch)`.

The server reads the field as follows:

1. If the field value is longer than 1024 bytes, treat the field as absent.
2. Parse the field as a List (RFC 9651 §4.2). If the parse fails, or the List does not have exactly one member, or the member is not an Inner List, treat the field as absent.
3. The capability set is the set of the Token items in the Inner List. Ignore items of other types and all parameters.
4. Ignore tokens that the server does not know.

Absent field = empty capability set.

| Token | Meaning in `rqp/1` |
|---|---|
| `block-patch` | The client can apply a delta. Required for a delta (section 4, step 5) |
| `dict-compression` | Reserved for Compression Dictionary Transport ([ADR-013](/engineering/architecture/adr/#adr-013); H-4, WP-16). No effect in `rqp/1` |
| `realtime-ws` | Reserved for a WebSocket channel (P8, WP-16). No effect in `rqp/1` HTTP |
| `realtime-wt` | Reserved for a WebTransport channel (P8, WP-16, experimental). No effect in `rqp/1` HTTP |

<Badge type="info" text="DESIGN" /> The draft tokens `manifest` and `json-blocks` are withdrawn. The manifest is always inline (H-1) and is part of F for every request (section 5). Both block formats (`html`, `json`) are part of `rqp/1` markup (M-16), so every `rqp/1` client supports both. A client that still sends these tokens is not an error: the server ignores unknown tokens.

### 3.4 `Rq-Have`

The client's template revision and the digest prefixes of its blocks (H-44). A Dictionary:

| Key | Type | Value |
|---|---|---|
| `t` | Token | The template revision of the client's snapshot (`t1.…`) |
| `b` | Byte Sequence | For each block of the snapshot in document order (the order of the manifest `blocks`): the first 8 bytes of the raw SHA-256 digest of the block content. Concatenated, no separator |

Example (3 blocks, 24 bytes in `b`; values of [§9](#_9-examples)):

```http
Rq-Have: t=t1.AKmWJPPGuP_pqb-eZhXtMCvGiXIFKGkWrwe8Plibllk, b=:0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:
```

The prefix is taken from the digest bytes, not from the base64url text. The client gets the digest by decoding the manifest `sha256` value (base64url, 43 characters, 32 bytes).

**Client.** The client MUST NOT send an `Rq-Have` field value longer than 4096 bytes. With at most 256 blocks the value is at most 2786 bytes (`b` has 2048 bytes, 2732 base64 characters), so a client with a valid snapshot never reaches the limit. A client sends `b` always when it sends `Rq-Have`.

**Server.** The server reads the field in this order:

1. If the field value (all field lines combined, RFC 9651 §4.2) is longer than 4096 bytes, treat `Rq-Have` as absent. Do not parse it.
2. Parse the field as a Dictionary. If the parse fails, treat `Rq-Have` as absent.
3. If the member `t` is missing or is not a Token, treat `Rq-Have` as absent. Ignore parameters of `t`.
4. The client template matches when `t` is equal, byte for byte, to the current template revision. If it does not match, stop: `b` is not used.
5. If the member `b` is missing or is not a Byte Sequence, `b` is **absent**.
6. Let L be the length of the decoded `b` in bytes. If L is not a multiple of 8, or L is not 8 × N, `b` is **absent**. (The template matches, so the client page has the same N blocks.)
7. Ignore other members and all parameters. They are reserved for later versions.

When `b` is absent and the template matches, the server uses the all-blocks fallback (H-7; section 4, step 6). Steps 5 and 6 are the "other handling" that RFC 9651 §2.2 allows: a bad `b` does not remove `t`.

<Badge type="info" text="DESIGN" /> A Byte Sequence uses standard base64 with padding (RFC 9651 §3.3.5). RFC 9651 §4.2.7 tells parsers to accept missing padding and non-zero pad bits. The server uses the decoded bytes only, so such variants give the same result.

### 3.5 `If-None-Match`

The client sends the page revision of its snapshot as a strong entity-tag (H-46):

```http
If-None-Match: "r1.cosEo1VifLe4w8LUfueSxKvirLAUuX6eZ7Nex-F6Q3M"
```

The server uses the field for two things:

1. **`304`.** The HTTP condition (RFC 9110 §13.1.2) against the ETag of F. <Badge type="tip" text="FACT" /> RFC 9110 §13.1.2: a recipient MUST use the weak comparison function for `If-None-Match`; the condition is false when the value is `*` and a current representation exists, or when one of the listed tags matches; then the server responds `304` to `GET` and `HEAD`.
2. **Delta base.** The field gives a base revision B only when the combined field value is exactly one entity-tag, strong or weak (`W/`), and its opaque-tag is `"r1.` + 43 base64url characters + `"`. Then B is the value between the quotes. In every other case (no field, `*`, a list of two or more tags, another syntax) there is no base, and the server does not send a delta.

<Badge type="info" text="DESIGN" /> Weak tags are accepted for the base because an intermediary can weaken an ETag (section 5.4). A wrong base does no harm: the client checks `base_revision` (section 6.3).

The draft header `Rq-Base-Revision` is withdrawn (H-46).

### 3.6 `Accept` and the delta media type

The delta media type is `application/vnd.rivqen.delta+json`. A client that can take a delta lists it in `Accept`:

```http
Accept: text/html, application/vnd.rivqen.delta+json
```

The server accepts that the client takes a delta when `Accept` has a media range whose type and subtype are exactly `application` and `vnd.rivqen.delta+json` (ASCII case-insensitive), with no `q` parameter or `q` > 0. Wildcards (`*/*`, `application/*`) do not count.

### 3.7 Revisions

| Revision | Syntax | Where |
|---|---|---|
| Template revision | `t1.` + 43 base64url characters (46 in total) | Manifest, `Rq-Have` member `t`, patch envelope |
| Page revision | `r1.` + 43 base64url characters (46 in total) | Manifest, `ETag`, `If-None-Match`, patch envelope |

The algorithm that computes them is normative in [manifest and patch](/engineering/protocol/manifest-patch) (H-8). Servers compute revisions with that algorithm. Clients compute the page revision only to check a delta (section 6.3); otherwise they treat revisions as opaque. Revisions are compared byte for byte.

### 3.8 Response headers

| Response | Headers that RQP sets |
|---|---|
| Full document F | `ETag: "r1.…"` (strong); `Rq-Version: 1`. `Content-Type` and `Cache-Control` of the application stay |
| Delta D | `Content-Type: application/vnd.rivqen.delta+json`; `Rq-Version: 1`; `Cache-Control: no-store`. No `ETag` |
| `304` | `ETag: "r1.…"`; the fields of section 4, step 9 |
| Invalid page | None. The application response is unchanged |

<Badge type="info" text="DESIGN" /> The draft response headers `Rq-Revision` and `Rq-Template-Revision` are withdrawn. F carries both revisions in the manifest and the page revision in `ETag`. D carries `base_revision`, `next_revision` and `template_revision` in the envelope. A second copy in a header can disagree with the body, and the client would need a rule for that case.

## 4. Response selection (server)

<Badge type="info" text="DESIGN" /> The server runs these steps for each request. The first step that sends a response ends the algorithm.

Inputs: the request; the application response A (status, header fields, page bytes P); the server limit `maxPatchBytes` ([resource limits](/engineering/core/resource-limits), default 262144 bytes).

1. **Scope.** If the request method is not `GET` or `HEAD`, or the status of A is not `200`, or the media type of A is not `text/html`, send A unchanged. RQP does not apply.
2. **Validity.** Analyze P with the [markup rules](/engineering/protocol/markup) (section 2, "invalid for RQP"; H-12, H-15). If P is invalid for RQP, send A unchanged. The response has no manifest, no `Rq-Version` and no other `Rq-*` field, and no ETag that the server computed (an `ETag` of A stays as it is). The server SDK reports the broken rule to the template author, not to the client. An internal error of the server SDK has the same result and is logged as `SERVER_PASSTHROUGH` ([errors](/engineering/core/errors)).
3. **Full document.** Build F: insert the manifest into P ([manifest and patch](/engineering/protocol/manifest-patch)). Let E be the strong entity-tag `"` + page revision + `"`. The header fields of F are those of A, with these changes: `ETag: E` replaces an `ETag` of A; add `Rq-Version: 1`; set `Content-Length` for F. Do not add an `Rq-*` field or `Accept` to `Vary`. Keep `Cache-Control` of A (H-45).
4. **Conditions.** Evaluate the preconditions of the request against F, in the order of RFC 9110 §13.2.2. If `If-None-Match` is present, ignore `If-Modified-Since`. If the `If-None-Match` condition is false (`*`, or one listed tag matches E by weak comparison), go to step 9.
5. **Delta eligibility.** The request is delta-eligible only when all of these are true. Else go to step 8.
   1. The method is `GET`, and the request has no `Range` field.
   2. `Rq-Version` is `1` ([§3.2](#_3-2-rq-version)).
   3. The capability set has `block-patch` ([§3.3](#_3-3-rq-capabilities)).
   4. `Accept` accepts the delta media type ([§3.6](#_3-6-accept-and-the-delta-media-type)).
   5. `If-None-Match` gives a base B ([§3.5](#_3-5-if-none-match)). B is not the current page revision, because step 4 did not match.
   6. `Rq-Have` is present and its template matches ([§3.4](#_3-4-rq-have), steps 1 to 4).
6. **Change set.** If `b` is present: the change set C has each block i (0-based, document order) whose current digest prefix (the first 8 bytes of SHA-256 of its content) is not equal to the bytes 8i to 8i + 7 of `b`. If `b` is absent: C has all N blocks (H-7). If C is empty, go to step 8.
7. **Delta.** Build D ([manifest and patch](/engineering/protocol/manifest-patch)) with `base_revision` = B, `next_revision` = the current page revision, `template_revision` = the current template revision, and one operation for each block in C, in document order. If the size of D in bytes is larger than `maxPatchBytes`, or is not smaller than the size of F, go to step 8. Else send D: status `200`, the header fields of A without `ETag`, `Last-Modified` and `Expires`, with `Content-Type: application/vnd.rivqen.delta+json`, `Rq-Version: 1`, `Cache-Control: no-store`, and `Content-Length` for D.
8. **Full.** Send F with status `200`. For `HEAD`, send the header fields of F without content.
9. **Not modified.** Send `304` without content. Include the fields that F would have of this list: `ETag` (E), `Cache-Control`, `Expires`, `Vary`, `Date`, `Content-Location` (RFC 9110 §15.4.5). `Rq-Version: 1` MAY be included.

Sizes in step 7 are the bytes before any content coding. The comparison of digest prefixes in step 6 does not need a constant-time function: the values are not secret.

| Outcome | Step | Status | Body |
|---|---|---|---|
| Out of scope | 1 | As A | A |
| Invalid for RQP | 2 | `200` | P (A unchanged) |
| Not modified | 4, 9 | `304` | None |
| Delta | 7 | `200` | D |
| Full document | 8 | `200` | F |

<Badge type="info" text="DESIGN" /> Decisions in these steps:

- **One full representation.** A request that is not an RQP request, or has a version that the server does not support, gets F, as an RQP request does. F does not depend on any RQP request field, so caches need no `Vary` for RQP (H-45). A client without RQP ignores the manifest script (its type is not a script type that a browser runs) and the `Rq-Version` field.
- **Empty change set → F.** With a base B that is not the current revision, an empty change set means that the client has a prefix collision or inconsistent headers. A `304` would be wrong (the client does not have E). An empty delta would fail the client check of `next_revision` and cost a second request. F is correct in one response.
- **No base → F.** A delta needs `base_revision`. Without B, the client cannot check that the delta fits its page.
- **Size.** At equal size, F is better: it can be cached and needs no client work.

## 5. Caching

### 5.1 Full document

F has a strong `ETag` equal to `"r1.…"` and keeps the cache policy (`Cache-Control`, `Expires`) of the application (H-45). F is the same for all requests to the URI that have the same application output, so it has no `Vary` on `Rq-*` fields or on `Accept`. `Vary` fields of the application (for example `Accept-Encoding`, `Cookie`) stay.

The bytes of F MUST be a function of the page bytes P and the target URI, so that one page revision always gives the same F (a strong validator). Manifest fields that are not in the page revision preimage MUST NOT change while P stays the same ([manifest and patch](/engineering/protocol/manifest-patch)).

### 5.2 Delta

D has `Cache-Control: no-store` and no `ETag` (H-45). A cache that follows RFC 9111 never stores D, so it never sends D to another request. D needs no `Vary`. A server MAY add `Vary: Rq-Have` as defense in depth for a cache that does not follow `no-store`; clients and fixtures do not depend on it.

Status `226 IM Used` is never used (RQP-02). <Badge type="tip" text="FACT" /> `226` belongs to RFC 3229 delta encoding, a different mechanism with its own `A-IM` and `IM` fields.

### 5.3 `304`

`304` follows RFC 9110 §15.4.5 and RFC 9111 §4.3.4. It carries the `ETag` E and the cache fields of F (section 4, step 9). An HTTP cache that receives the `304` updates its stored F; the RQP client keeps its snapshot.

### 5.4 Shared caches

<Badge type="info" text="DESIGN" /> These cases are safe without RQP knowledge in the cache:

| Case | What happens | Why it is safe |
|---|---|---|
| A cache has a fresh F and gets a delta-eligible request | The cache sends its F (`200`) | A full `200` is always valid for the client. The client gets no delta |
| A cache has a fresh F with the client's revision | The cache sends `304` | `If-None-Match` matches. The client keeps its snapshot |
| A cache revalidates and sends the union of its tags and the client's tag (RFC 9111 §4.3.2) | `If-None-Match` has two or more tags, so there is no base | The server sends F or `304` |
| A cache replaces the client's tag with its own tag | The server can send D with the wrong `base_revision` | The client rejects D (`PATCH_BASE_MISMATCH`) and loads F once (section 6.3) |
| A cache or server weakens the ETag (`W/"r1.…"`), for example for a content coding | `304` still works (weak comparison) | The client uses the manifest, not the ETag, as its revision |

<Badge type="tip" text="FACT" /> RFC 9110 §8.8.3 notes that a strong entity-tag of a content-encoded representation "has to be distinct" from the tag of the unencoded representation, because of cache updates and range requests.

<Badge type="info" text="DESIGN" /> RQP uses the same tag E for all content codings of F, and allows a server or an intermediary to send it weak (`W/`) when it applies a content coding. RQP clients do not use range requests for documents, and a `304` updates only header fields that are the same for all codings. This choice is an open question for the human in sprint [WP-17 S2](/engineering/plan/sprints/WP-17-S2).

A page that wants each navigation to reach the server, so that the server can send a delta, uses a cache policy that makes caches revalidate (for example `Cache-Control: no-cache`). This is the choice of the application.

## 6. Client rules

### 6.1 Where the client sends RQP fields

1. The client MUST NOT send `Rq-*` fields to origins outside its allowlist (RQP-05).
2. The client sends RQP fields only on same-origin requests or on requests that the native SDK makes itself (for example an intercepted navigation). <Badge type="tip" text="FACT" /> `Rq-*` fields and `If-None-Match` are not CORS-safelisted request headers (Fetch Standard, "CORS-safelisted request-header": only `Accept`, `Accept-Language`, `Content-Language`, `Content-Type` and `Range`, each with value rules). A cross-origin `fetch()` with them needs a CORS preflight. RQP is for same-origin navigation; it defines no CORS use.
3. The client sends `If-None-Match` and `Rq-Have` only from a snapshot with the same cache key as the request (same partition, same canonical URL; [cache identity](/engineering/cache/identity)).
4. The client MUST NOT copy the fields of one request to a redirected request. The client handles a redirect as a new request and builds its RQP fields for the new URI (rules 1 to 3).

### 6.2 Request

With a snapshot S (template revision T, page revision R, blocks):

```http
GET /catalog HTTP/2
Host: shop.example.com
Accept: text/html, application/vnd.rivqen.delta+json
Rq-Version: 1
Rq-Capabilities: (block-patch)
If-None-Match: "R"
Rq-Have: t=T, b=:<prefixes>:
```

Without a snapshot, the client sends `Rq-Version: 1` and `Rq-Capabilities`. It does not send `Rq-Have` or an RQP `If-None-Match`, and it SHOULD NOT list the delta media type in `Accept`.

### 6.3 Response

The client handles the response in this order:

1. **`304`.** Keep S. S is the current page. Do not require `Rq-Version` on a `304`.
2. **Delta.** A `200` whose `Content-Type` media type is `application/vnd.rivqen.delta+json` (ASCII case-insensitive):
   1. If the request did not offer a delta (no S, no `block-patch` or no delta media type in `Accept`), stop with `PROTOCOL_ERROR` and do the fallback load (item 5).
   2. Validate and apply D as [manifest and patch](/engineering/protocol/manifest-patch) and the [diff engine](/engineering/core/diff-engine) define. The checks include: size ≤ the client `maxPatchBytes` (`PATCH_TOO_LARGE`), `template_revision` = T (`TEMPLATE_MISMATCH`), `base_revision` = R (`PATCH_BASE_MISMATCH`), the hash of each operation (`INTEGRITY_ERROR`).
   3. Compute the page revision of the result with T and the new block hashes (algorithm in [manifest and patch](/engineering/protocol/manifest-patch)). It MUST be equal to `next_revision`. Else the result is discarded (`INTEGRITY_ERROR`). This check also detects a prefix collision in `Rq-Have`.
   4. On any failure, show nothing of D and do the fallback load (item 5).
3. **Full document.** A `200` with an HTML media type is an RQP page only when it has the field `Rq-Version: 1` and the manifest that [manifest and patch](/engineering/protocol/manifest-patch) defines. The client compares the `type` value of the manifest script with `application/rivqen-manifest+json` by an ASCII case-insensitive byte match only, never by Unicode case folding (H-28). Without `Rq-Version: 1` the client does not use a manifest, also when the body has one: only a server SDK sets the field, and markup rule M-13 makes the server SDK reject a page that already has a manifest script. The revision of the new snapshot is the manifest page revision, not the `ETag`.
4. **Other.** Any other response is a normal HTTP response. The session is not RQP for this navigation ([versioning](/engineering/protocol/versioning)).
5. **Fallback load.** One `GET` without `Rq-Have`, without an RQP `If-None-Match` and without the delta media type in `Accept`. By section 4, the response cannot be a delta. If it is a delta, stop with `PROTOCOL_ERROR` and let the WebView load the URL normally. The client does at most one fallback load per navigation.

### 6.4 Versions, capabilities and downgrade

1. The server does not announce capabilities. The client learns what the server does from the response: a manifest with `Rq-Version: 1` means RQP; a delta means `block-patch`.
2. The client ignores unknown `Rq-*` response fields and `Rq-Version` values other than `1` (the response is then not RQP).
3. A server ignores unknown capability tokens and unknown `Rq-Have` members.
4. Moves to legacy mode or to normal HTTPS follow [versioning](/engineering/protocol/versioning) (DG-01 to DG-05).

## 7. Limits

| Item | Limit | On violation |
|---|---|---|
| `Rq-Have` field value | 4096 bytes | Server: `Rq-Have` is absent. Client: never sends more |
| `Rq-Capabilities` field value | 1024 bytes | Server: field is absent |
| Blocks in `Rq-Have` member `b` | N (at most 256, `maxBlocks`) | Server: `b` is absent |
| Delta size | `maxPatchBytes` and smaller than F | Server: sends F. Client: `PATCH_TOO_LARGE`, fallback load |
| Fallback loads per navigation | 1 | Normal HTTPS load |

<Badge type="warning" text="RESEARCH" /> The 4096-byte budget comes from research R-6: about 8 KiB per field is the smallest common request field limit (nginx, Apache, Tomcat); [WP-17 S2](/engineering/plan/sprints/WP-17-S2) §1.

Server work for steps 4 to 7 is bounded: one parse of at most 4096 bytes, at most 256 comparisons of 8 bytes, and one envelope of at most `maxPatchBytes`. The block digests are the ones that the server computes for the manifest anyway.

## 8. Rules

| ID | Rule |
|---|---|
| RQP-01 | A delta MUST use the media type `application/vnd.rivqen.delta+json`, MUST have `Cache-Control: no-store` and MUST NOT have an `ETag`. |
| RQP-02 | A server MUST NOT use status `226 IM Used` for RQP deltas. (RFC 3229 delta encoding is a different mechanism.) |
| RQP-03 | A `304` MUST NOT have content. It MUST have the `ETag` and the cache fields that F would have. |
| RQP-04 | A server that does not support RQP ignores the fields and sends a normal response. The client then treats the session as non-RQP. |
| RQP-05 | A client MUST NOT send `Rq-*` fields to origins outside its allowlist. |
| RQP-06 | A server MUST ignore unknown capability tokens and unknown `Rq-Have` members. A server MUST NOT send a delta to a request without `block-patch`. |
| RQP-07 | A revision is `t1.` or `r1.` followed by 43 base64url characters. Revisions are compared byte for byte. Clients compute a page revision only to check a delta. |
| RQP-08 | F MUST have the strong `ETag` `"r1.…"`, MUST NOT depend on RQP request fields (`Rq-*`, the delta media type in `Accept`, `If-None-Match`), and MUST NOT list an `Rq-*` field in `Vary`. |
| RQP-09 | For a page that is invalid for RQP, the server MUST send the application response unchanged: no manifest, no `Rq-*` field, no ETag that RQP computed. |
| RQP-10 | A server MUST NOT send a delta without a single `r1.` entity-tag in `If-None-Match` and a matching `Rq-Have` template. |
| RQP-11 | A delta MUST have at least one operation, MUST NOT be larger than `maxPatchBytes`, and MUST be smaller than F. Else the server sends F. |
| RQP-12 | A client MUST NOT send an `Rq-Have` value longer than 4096 bytes. A server MUST treat a longer value as absent, without parsing it. |
| RQP-13 | A client MUST check `base_revision` and `next_revision` of a delta. On failure it discards the delta and does one fallback load without delta fields. |
| RQP-14 | A client MUST compare the manifest script type by ASCII case-insensitive byte match only (H-28), and MUST use a manifest only from a response with `Rq-Version: 1`. |
| RQP-15 | A client MUST send `If-None-Match` and `Rq-Have` only from a snapshot with the same cache key, and MUST NOT copy them to a redirected request. |
| RQP-16 | Preconditions are evaluated against F. A server SDK MUST NOT let the application answer `304` by a validator that the SDK replaces. |
| RQP-17 | Logs MUST NOT contain the `Rq-Have` member `b` or block content. Revisions, sizes and outcomes are allowed ([errors and logging](/engineering/architecture/errors-logging)). |

## 9. Examples

<Badge type="tip" text="FACT" /> The revisions and digests below are the output of the Node.js parser candidate (`tools/rqp-markup/node`) for these two pages (2026-10-12). Page A:

```html
<!DOCTYPE html>
<html>
<head>
  <title data-rq-block="title">Shop</title>
</head>
<body>
  <p class="price" data-rq-block="price">120.00</p>
  <script type="application/json" data-rq-block="cart">{"items":3}</script>
</body>
</html>
```

Page B is page A with `130.00` in place of `120.00` (each file ends with a line feed).

```text
Both pages
  template revision  t1.AKmWJPPGuP_pqb-eZhXtMCvGiXIFKGkWrwe8Plibllk
  title prefix (hex) d00aae6b7fbf77a4
  cart prefix (hex)  cf210bdca91994b9
Page A
  page revision      r1.cosEo1VifLe4w8LUfueSxKvirLAUuX6eZ7Nex-F6Q3M
  price prefix (hex) 5e0de86a2a25df2c
Page B
  page revision      r1.lROI6T5f4aHLk0U4ioQZd7GHs3jcbWbsoCXoc-fgYP8
  price prefix (hex) 6c1f527187104332
```

Headers that do not matter for RQP are not shown. The bodies of F and D follow [manifest and patch](/engineering/protocol/manifest-patch).

### 9.1 First visit: full document

The client has no snapshot. The current page is A.

```http
GET /catalog HTTP/2
Host: shop.example.com
Accept: text/html
Rq-Version: 1
Rq-Capabilities: (block-patch)
```

```http
HTTP/2 200
Content-Type: text/html; charset=utf-8
ETag: "r1.cosEo1VifLe4w8LUfueSxKvirLAUuX6eZ7Nex-F6Q3M"
Rq-Version: 1
Cache-Control: no-cache
```

Body: F of page A (page A with the manifest in `head`). Section 4, step 8.

### 9.2 Not modified

The client has a snapshot of A. The current page is still A.

```http
GET /catalog HTTP/2
Host: shop.example.com
Accept: text/html, application/vnd.rivqen.delta+json
Rq-Version: 1
Rq-Capabilities: (block-patch)
If-None-Match: "r1.cosEo1VifLe4w8LUfueSxKvirLAUuX6eZ7Nex-F6Q3M"
Rq-Have: t=t1.AKmWJPPGuP_pqb-eZhXtMCvGiXIFKGkWrwe8Plibllk, b=:0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:
```

```http
HTTP/2 304
ETag: "r1.cosEo1VifLe4w8LUfueSxKvirLAUuX6eZ7Nex-F6Q3M"
Cache-Control: no-cache
```

No body. Section 4, steps 4 and 9.

### 9.3 Delta

The same request as in 9.2. The current page is B. Only the `price` prefix differs.

```http
HTTP/2 200
Content-Type: application/vnd.rivqen.delta+json
Rq-Version: 1
Cache-Control: no-store
```

Body: D with `base_revision` `r1.cosEo1…`, `next_revision` `r1.lROI6T5f…`, `template_revision` `t1.AKmWJPPG…` and one operation, for block `price` (content `130.00`). Section 4, step 7. Without the member `b` in `Rq-Have`, D has three operations (all blocks, H-7), if it is still smaller than F.

### 9.4 Invalid page

The application sends a page with the block id `price` two times (`RQP_MARKUP_DUPLICATE`, markup rule M-21). The request is as in 9.1.

```http
HTTP/2 200
Content-Type: text/html; charset=utf-8
Cache-Control: no-cache
```

Body: the page bytes of the application, unchanged. No manifest, no `Rq-Version`, no RQP `ETag`. Section 4, step 2. The client shows the page as normal HTML.

## 10. Security and privacy

1. **Client input to the server.** `Rq-Have` and `If-None-Match` only select what the server sends to the same client. A false value gives that client a wrong delta, which its checks reject, or a full document. Deltas are `no-store`, so a false value cannot reach another client through a cache.
2. **Digest prefixes.** 8 bytes of SHA-256 per block. An accidental collision is very improbable. A forced collision (a block content with the same 64-bit prefix) makes the server omit a changed block; the `next_revision` check uses the full digests (H-8), so the client loads F. The result is one extra request, never wrong content (H-44).
3. **Bounded work.** Section 7. The server never stores request state.
4. **Privacy.** `Rq-Have` holds hashes of page content. Clients send it only to the origin of the snapshot and only from the same partition (RQP-15); logs never contain it (RQP-17). A content-derived ETag is the same for all users that see the same page. For private pages, a keyed revision is decided in WP-18 (H-8).
5. **Injected manifest.** A manifest is used only with `Rq-Version: 1` (RQP-14). A page with a manifest-like script is invalid for RQP (markup rule M-13), so it is sent without `Rq-Version`.

## 11. Intermediaries

<Badge type="warning" text="RESEARCH" /> The intermediary test runs in WP-13 (with WP-17). It checks for each CDN and proxy in the support matrix: `Rq-*` request fields arrive unchanged; `Rq-Version` in responses arrives unchanged; a delta with `no-store` is not stored; `ETag` and `If-None-Match` pass (strong or weakened); a cached F is served to a delta-eligible request without harm. Research R-6 found that Cloudflare ignores `Vary` by default; RQP does not depend on `Vary` (section 5). An intermediary that removes `Rq-Version` turns RQP off for its clients (section 6.3, item 3): the pages still work as normal HTML.

## Related

- [RQP markup](/engineering/protocol/markup)
- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Versioning and downgrade](/engineering/protocol/versioning)
- [Diff engine](/engineering/core/diff-engine)
- [Resource limits](/engineering/core/resource-limits)
- [Golden fixtures](/engineering/protocol/fixtures)
