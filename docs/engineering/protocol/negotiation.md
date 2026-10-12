# RQP: negotiation and headers

The Rivqen protocol (RQP) has explicit versions, capabilities and revisions. This page defines how a client and a server agree on RQP, which headers they send, how the server selects the response (full document, delta or `304`), how caches handle the responses, and what the client checks.

**Status:** <Badge type="info" text="DESIGN" /> Normative for `rqp/1` from WP-17 S2. Decisions: `Rq-` header prefix and Structured Field syntax ([ADR-006](/engineering/architecture/adr/#adr-006)); the `dict-compression` token is reserved (H-4); the client sends its block digests, with an all-blocks fallback (H-7); content-derived revisions `t1.`/`r1.` (H-8); ASCII case-insensitive manifest type match (H-28); `Rq-Have` with 64-bit digest prefixes (H-44); caching of full document and delta (H-45); `If-None-Match` carries the client's page revision (H-46); client markup analysis (H-48); `http` only in debug builds (H-49); error codes (H-50); the same full document for every request (H-51); a weak ETag with a content coding (H-52); `Rq-Capabilities` is an Inner List of Tokens (H-53); withdrawn tokens and response headers (H-54); `policy` and `limits` of the manifest are part of the page revision (H-55); the server sends F when a changed block fails the client attribute rule (H-56). Records: [WP-17 S1](/engineering/plan/sprints/WP-17-S1) §1 and [WP-17 S2](/engineering/plan/sprints/WP-17-S2) §1. Block markup: [RQP markup](/engineering/protocol/markup). Manifest, patch envelope and revision algorithm: [RQP manifest and patch](/engineering/protocol/manifest-patch).

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
| Full document F | P with the page manifest inserted ([manifest and patch](/engineering/protocol/manifest-patch)). The one representation of a valid page, the same for every request (H-51) |
| Delta D | A patch envelope that changes a client's page from its revision to the current revision |
| Template revision | `t1.…` of the page ([§3.7](#_3-7-revisions)) |
| Page revision | `r1.…` of the page. The opaque-tag of the ETag of F (strong or weak, [§5.1](#_5-1-full-document)) |
| Snapshot | A page that the client holds, with its manifest, for one cache key ([cache identity](/engineering/cache/identity)) |
| N | The number of blocks of the current page (0 to 256) |

## 3. Headers

### 3.1 Overview

RQP headers use **Structured Field Values** ([RFC 9651](https://www.rfc-editor.org/rfc/rfc9651)). RQP reuses the standard fields `If-None-Match`, `ETag`, `Accept`, `Content-Type` and `Cache-Control`.

| Header | Direction | Type |
|---|---|---|
| `Rq-Version` | both | Integer |
| `Rq-Capabilities` | request | Inner List of Tokens |
| `Rq-Have` | request | Dictionary |
| `If-None-Match` | request | RFC 9110 |
| `Accept` | request | RFC 9110 |
| `ETag` | response | RFC 9110; strong or weak ([§5.1](#_5-1-full-document)) |

<Badge type="tip" text="FACT" /> RFC 9651 §2.2: when parsing fails, the entire field is ignored; when a field-specific constraint is violated, the entire field is also ignored, unless the field definition defines other handling. <Badge type="info" text="DESIGN" /> In RQP, "ignored" means "absent" in the algorithm of section 4. Only `Rq-Have` member `b` has other handling ([§3.4](#_3-4-rq-have)).

### 3.2 `Rq-Version`

An Item, type Integer. Parameters are ignored.

- Request: the RQP major version that the client uses. `rqp/1` clients send `Rq-Version: 1`.
- Response: the server sends `Rq-Version: 1` on every full document and every delta of a valid page. It MAY send it on a `304`.

A server supports the request version only when the value is the Integer `1`. Any other value, a parse failure or no field means: the request is not an RQP request. The server still sends F, the same F as to an RQP request (section 4, H-51); it never sends a delta for such a request.

### 3.3 `Rq-Capabilities`

The field value is an Inner List of Tokens (H-53, [ADR-006](/engineering/architecture/adr/#adr-006)):

```http
Rq-Capabilities: (block-patch realtime-ws)
```

<Badge type="tip" text="FACT" /> RFC 9651 has no top-level Inner List type: an Inner List is a member of a List or of a Dictionary (§3.1.1). So the field is parsed as a List, and the only valid shape is a List with exactly one member, which is an Inner List.

The server reads the field as follows:

1. If the field value (all field lines combined with `, ` — comma and one space — between them, RFC 9651 §4.2; DL-029) is longer than 1024 bytes (`maxRqCapabilitiesBytes`), treat the field as absent. Do not parse it.
2. Parse the field as a List (RFC 9651 §4.2). If the parse fails, treat the field as absent.
3. If the List does not have exactly one member, or the member is not an Inner List, or an item of the Inner List is not a Token, treat the field as absent.
4. The capability set is the set of the Tokens in the Inner List. Ignore all parameters (of the Inner List and of its items). They are reserved for later versions.
5. Ignore tokens that the server does not know.

Absent field = empty capability set. Any other shape is not an error; it only means that the request gets no delta (H-53). Examples:

| Field lines | Capability set |
|---|---|
| `(block-patch)` | `block-patch` |
| `(block-patch realtime-ws);v=2` | `block-patch`, `realtime-ws` (parameter ignored) |
| `()` | Empty |
| `block-patch` | Absent: a Token, not an Inner List |
| `(block-patch "x")` | Absent: a String in the Inner List |
| `(block-patch), (realtime-ws)` | Absent: two members |
| Two lines, `(block-patch)` and `(realtime-ws)` | Absent: combined, they are a List with two members |

| Token | Meaning in `rqp/1` |
|---|---|
| `block-patch` | The client can apply a delta. Required for a delta (section 4, step 5) |
| `dict-compression` | Reserved for Compression Dictionary Transport ([ADR-013](/engineering/architecture/adr/#adr-013); H-4, WP-16). No effect in `rqp/1` |
| `realtime-ws` | Reserved for a WebSocket channel (P8, WP-16). No effect in `rqp/1` HTTP |
| `realtime-wt` | Reserved for a WebTransport channel (P8, WP-16, experimental). No effect in `rqp/1` HTTP |

<Badge type="info" text="DESIGN" /> The draft tokens `manifest` and `json-blocks` are removed (H-54). The manifest is always inline (H-1) and is part of F for every request (H-51, section 5). Both block formats (`html`, `json`) are part of `rqp/1` markup (M-16), so every `rqp/1` client supports both. A client MUST NOT send these tokens. A server treats them as unknown tokens and ignores them (step 5).

### 3.4 `Rq-Have`

The client's template revision and the digest prefixes of its blocks (H-44). A Dictionary:

| Key | Type | Value |
|---|---|---|
| `t` | Token | The template revision of the client's snapshot (`t1.…`) |
| `b` | Byte Sequence | For each block of the snapshot in document order (the order of the manifest `blocks`): the first 8 bytes of the raw SHA-256 digest of the block content. Concatenated, no separator |

Example (3 blocks, 24 bytes in `b`; values of [§9](#_9-examples)):

```http
Rq-Have: t=t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM, b=:0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:
```

The prefix is taken from the digest bytes, not from the base64url text. The client gets the digest by decoding the manifest `sha256` value (base64url, 43 characters, 32 bytes).

**Client.** The client MUST NOT send an `Rq-Have` field value longer than 4096 bytes. With at most 256 blocks the value is at most 2786 bytes (`b` has 2048 bytes, 2732 base64 characters), so a client with a valid snapshot never reaches the limit. A client sends `b` always when it sends `Rq-Have`.

**Server.** The server reads the field in this order:

1. If the field value (all field lines combined with `, ` — comma and one space — between them, RFC 9651 §4.2; DL-029) is longer than 4096 bytes (`maxRqHaveBytes`), treat `Rq-Have` as absent. Do not parse it.
2. Parse the field as a Dictionary. If the parse fails, treat `Rq-Have` as absent.
3. If the member `t` is missing or is not a Token, treat `Rq-Have` as absent. Ignore parameters of `t`.
4. The client template matches when `t` is equal, byte for byte, to the current template revision. If it does not match, stop: `b` is not used.
5. If the member `b` is missing or is not a Byte Sequence, `b` is **absent**.
6. Let L be the length of the decoded `b` in bytes. If L is not a multiple of 8, or L is not 8 × N, `b` is **absent**. (The template matches, so the client page has the same N blocks.)
7. Ignore other members and all parameters. They are reserved for later versions.

<Badge type="tip" text="FACT" /> RFC 9651 §4.2.2: when a Dictionary has a key two times, the parser overwrites the value of the first member with the later one. So `t=X, b=:…:, t=Y` has `t` = `Y`. The client sends each key once.

When `b` is absent and the template matches, the server uses the all-blocks fallback (H-7; section 4, step 6). Steps 5 and 6 are the "other handling" that RFC 9651 §2.2 allows: a bad `b` does not remove `t`.

<Badge type="info" text="DESIGN" /> A Byte Sequence uses standard base64 with padding (RFC 9651 §3.3.5). RFC 9651 §4.2.7 tells parsers to accept missing padding and non-zero pad bits. The server uses the decoded bytes only, so such variants give the same result.

### 3.5 `If-None-Match`

The client sends the page revision of its snapshot as a strong entity-tag (H-46):

```http
If-None-Match: "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8"
```

The server uses the field for two things:

1. **`304`.** The HTTP condition (RFC 9110 §13.1.2) against the ETag of F. <Badge type="tip" text="FACT" /> RFC 9110 §13.1.2: a recipient MUST use the weak comparison function for `If-None-Match`; the condition is false when the value is `*` and a current representation exists, or when one of the listed tags matches; then the server responds `304` to `GET` and `HEAD`.
2. **Delta base.** The field gives a base revision B only when the combined field value is exactly one entity-tag, strong or weak (`W/`), and its opaque-tag is `"r1.` + 43 base64url characters + `"`. Then B is the value between the quotes. In every other case (no field, `*`, a list of two or more tags, another syntax) there is no base, and the server does not send a delta.

<Badge type="info" text="DESIGN" /> Weak tags are accepted for the base (H-52): the server itself sends `W/"r1.…"` with a content coding (section 5.1), and an intermediary can weaken a strong tag (section 5.4). A wrong base does no harm: the client checks `base_revision` and `next_revision` (section 6.3). An `rqp/1` client always sends the strong form, with the page revision of its manifest.

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

The algorithm that computes them is normative in [manifest and patch §1.3](/engineering/protocol/manifest-patch#_1-3-hashes-and-revisions) (H-8). The page revision covers the template, the block contents and the manifest members `policy` and `limits` (H-55). Servers compute revisions with that algorithm. Clients compute revisions only to check them: the markup analysis of the original document of F against its manifest, the `next_revision` of a delta, and the markup analysis of the patched document (H-48; section 6.3). Revisions are compared byte for byte.

### 3.8 Response headers

| Response | Headers that RQP sets |
|---|---|
| Full document F | `ETag: "r1.…"` (identity coding) or `ETag: W/"r1.…"` (content coding, H-52); `Rq-Version: 1`. `Content-Type` and `Cache-Control` of the application stay |
| Delta D | `Content-Type: application/vnd.rivqen.delta+json`; `Rq-Version: 1`; `Cache-Control: no-store`. No `ETag` |
| `304` | The `ETag` that F would have; the fields of section 4, step 9 |
| Invalid page | None. The application response is unchanged |

<Badge type="info" text="DESIGN" /> The draft response headers `Rq-Revision` and `Rq-Template-Revision` are removed (H-54). A server MUST NOT send them. F carries both revisions in the manifest and the page revision in `ETag`. D carries `base_revision`, `next_revision` and `template_revision` in the envelope. A second copy in a header can disagree with the body, and the client would need a rule for that case.

## 4. Response selection (server)

<Badge type="info" text="DESIGN" /> The server runs these steps for each request. The first step that sends a response ends the algorithm.

Inputs: the request; the application response A (status, header fields, page bytes P); the server limit `maxPatchBytes` ([resource limits](/engineering/core/resource-limits), default 262144 bytes).

1. **Scope.** If the request method is not `GET` or `HEAD`, or the status of A is not `200`, or the media type of A is not `text/html`, send A unchanged. RQP does not apply.
2. **Validity.** Analyze P with the [markup rules](/engineering/protocol/markup) (section 2, "invalid for RQP"; H-12, H-15). If P is invalid for RQP, or has no `</head>` end tag in HTML context for the manifest ([manifest and patch §1.7](/engineering/protocol/manifest-patch#_1-7-delivery-in-the-document), H-49), send A unchanged. The response has no manifest, no `Rq-Version` and no other `Rq-*` field, and no ETag that the server computed (an `ETag` of A stays as it is). The server SDK reports the broken rule to the template author, not to the client. An internal error of the server SDK has the same result and is logged as `SERVER_PASSTHROUGH` ([errors](/engineering/core/errors)).
3. **Full document.** Build F: insert the manifest into P ([manifest and patch §1.7](/engineering/protocol/manifest-patch#_1-7-delivery-in-the-document)). F is the same for every request (H-51). Let E be the entity-tag of F: the strong `"` + page revision + `"` when the response has no content coding, and the weak `W/"` + page revision + `"` when a content coding is applied (H-52, section 5.1). The header fields of F are those of A, with these changes: `ETag: E` replaces an `ETag` of A; add `Rq-Version: 1`; set `Content-Length` for F. Do not add an `Rq-*` field or `Accept` to `Vary`. Keep `Cache-Control` of A (H-45).
4. **Conditions.** Evaluate the preconditions of the request against F, in the order of RFC 9110 §13.2.2. If `If-None-Match` is present, ignore `If-Modified-Since`. If the `If-None-Match` condition is false (`*`, or one listed tag matches E by weak comparison; so `"r1.X"` and `W/"r1.X"` both match), go to step 9.
5. **Delta eligibility.** The request is delta-eligible only when all of these are true. Else go to step 8.
   1. The method is `GET`, and the request has no `Range` field.
   2. `Rq-Version` is `1` ([§3.2](#_3-2-rq-version)).
   3. The capability set has `block-patch` ([§3.3](#_3-3-rq-capabilities)).
   4. `Accept` accepts the delta media type ([§3.6](#_3-6-accept-and-the-delta-media-type)).
   5. `If-None-Match` gives a base B ([§3.5](#_3-5-if-none-match)). B is not the current page revision, because step 4 did not match.
   6. `Rq-Have` is present and its template matches ([§3.4](#_3-4-rq-have), steps 1 to 4).
6. **Change set.** If `b` is present: the change set C has each block i (0-based, document order) whose current digest prefix (the first 8 bytes of SHA-256 of its content) is not equal to the bytes 8i to 8i + 7 of `b`. If `b` is absent: C has all N blocks (H-7). If C is empty, go to step 8.
7. **Delta.** If the current content of an `html` block in C fails the attribute rule of [manifest and patch §4.2](/engineering/protocol/manifest-patch#_4-2-attribute-rule), the server SHOULD go to step 8 (H-56). Build D ([manifest and patch](/engineering/protocol/manifest-patch)) with `base_revision` = B, `next_revision` = the current page revision, `template_revision` = the current template revision, and one operation for each block in C, in document order. If the size of D in bytes is larger than `maxPatchBytes`, or is not smaller than the size of F, go to step 8. Else send D: status `200`, the header fields of A without `ETag`, `Last-Modified` and `Expires`, with `Content-Type: application/vnd.rivqen.delta+json`, `Rq-Version: 1`, `Cache-Control: no-store`, and `Content-Length` for D.
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

- **One full representation (H-51).** A request that is not an RQP request, or has a version that the server does not support, gets F, as an RQP request does. F does not depend on any RQP request field, so caches need no `Vary` for RQP (H-45). A client without RQP ignores the manifest script (its type is not a script type that a browser runs) and the `Rq-Version` field.
- **Empty change set → F.** With a base B that is not the current revision, an empty change set means that the client has a prefix collision or inconsistent headers. A `304` would be wrong (the client does not have E). An empty delta would fail the client check of `next_revision` and cost a second request. F is correct in one response.
- **No base → F.** A delta needs `base_revision`. Without B, the client cannot check that the delta fits its page.
- **Size.** At equal size, F is better: it can be cached and needs no client work.
- **Attribute rule → F (H-56).** The client rejects a delta whose `html` content fails the attribute rule and then loads F: two requests. When the server sends F at once, the client needs one request. The client check stays (MUST); the server check is SHOULD because the result is the same page in both cases. The result depends only on the content of the block, so the server can compute it once for each block hash.
- **Change of `policy` or `limits` only → F.** The page revision changes (H-55), so step 4 does not match, but the change set is empty: the server sends F, and the client gets the new values. When blocks changed too, the client computes `next_revision` with its old policy line and rejects D (`INTEGRITY_ERROR`), then loads F: one extra request after a change of the server configuration ([manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply)).

## 5. Caching

### 5.1 Full document

F keeps the cache policy (`Cache-Control`, `Expires`) of the application (H-45). F is the same for all requests to the URI that have the same application output (H-51), so it has no `Vary` on `Rq-*` fields or on `Accept`. `Vary` fields of the application (for example `Accept-Encoding`, `Cookie`) stay.

The opaque-tag of the `ETag` of F is always `"r1.…"`, the page revision. Its form depends on the content coding of the response (H-52):

| Response | `ETag` |
|---|---|
| No content coding (identity) | Strong: `"r1.…"` |
| A content coding (`gzip`, `br`, `zstd`, …) | Weak: `W/"r1.…"`. MUST |

<Badge type="tip" text="FACT" /> RFC 9110 §8.8.3: a strong entity-tag of a content-encoded representation "has to be distinct" from the tag of the unencoded representation, because of cache updates and range requests. A weak tag does not claim byte equality, so one weak tag for all codings is correct.

The rule applies to the component that applies the content coding. When a server SDK does not apply the coding itself (for example a reverse proxy compresses the response), the deployment MUST make sure that this component weakens the tag or removes it. <Badge type="warning" text="RESEARCH" /> The nginx 1.7.3 change log (2014) says: "weak entity tags are now preserved on response modifications, and strong ones are changed to weak". A report on the nginx mailing list (2020) says that nginx removed a strong tag on gzip instead. The intermediary test of WP-13 checks this (section 11). Without an ETag the client still sends `If-None-Match` from its manifest; the server then answers by the algorithm of section 4, only a cache cannot answer `304` for it.

The bytes of F before the content coding are a function of the page revision and the target URI **by construction** (H-55): the page revision covers the template, the block contents, `policy` and `limits`; `protocol` comes from the version, and `origin` and `path` come from the target URI. A change of `policy` or `limits` in the server configuration therefore changes the ETag, as a strong validator requires. The only server rule that stays: the serializer of the manifest MUST be deterministic, so the same manifest value gives the same bytes ([manifest and patch §1.7](/engineering/protocol/manifest-patch#_1-7-delivery-in-the-document), server item 6).

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
| A cache or server weakens the ETag (`W/"r1.…"`), for example for a content coding | `304` still works (weak comparison); the base for a delta still works (section 3.5) | The client uses the manifest, not the ETag, as its revision |
| A cache stores F with `W/"r1.X"` and gets a request with `If-None-Match: "r1.X"` | The cache sends `304` (weak comparison) | The client has the page `r1.X` |

<Badge type="info" text="DESIGN" /> Decided by the human (H-52): the server sends one weak tag for all content codings of F, so the distinct-tag rule of RFC 9110 §8.8.3 (section 5.1) holds. RQP clients do not use range requests for documents. <Badge type="tip" text="FACT" /> RFC 9110 §13.1.5: a client MUST NOT send a weak entity-tag in `If-Range`, and the server compares it by the strong function, so a weak tag never joins parts of two codings.

A page that wants each navigation to reach the server, so that the server can send a delta, uses a cache policy that makes caches revalidate (for example `Cache-Control: no-cache`). This is the choice of the application.

## 6. Client rules

### 6.1 Where the client sends RQP fields

1. The client MUST NOT send `Rq-*` fields to origins outside its allowlist (RQP-05).
2. The client sends RQP fields only on same-origin requests or on requests that the native SDK makes itself (for example an intercepted navigation). <Badge type="tip" text="FACT" /> `Rq-*` fields and `If-None-Match` are not CORS-safelisted request headers (Fetch Standard, "CORS-safelisted request-header": only `Accept`, `Accept-Language`, `Content-Language`, `Content-Type` and `Range`, each with value rules). A cross-origin `fetch()` with them needs a CORS preflight. RQP is for same-origin navigation; it defines no CORS use.
3. The client sends `If-None-Match` and `Rq-Have` only from a snapshot with the same cache key as the request (same partition, same canonical URL; [cache identity](/engineering/cache/identity)).
4. The client MUST NOT copy the fields of one request to a redirected request. The client handles a redirect as a new request and builds its RQP fields for the new URI (rules 1 to 3).
5. A release build sends RQP fields only to `https` origins. A debug build also sends them to an `http` origin, but only when the app sets an explicit flag for it (H-49; [manifest and patch §1.6](/engineering/protocol/manifest-patch#_1-6-origin-and-path)). Without RQP fields the request is a normal load.

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

The client sends `Rq-Capabilities` as one field line with one Inner List of Tokens (H-53), and each field (`Rq-Have`, `Rq-Capabilities`) only once. It sends only the tokens of section 3.3 that it supports, never `manifest` or `json-blocks` (H-54).

### 6.3 Response

The client handles the response in this order:

1. **`304`.** Keep S. S is the current page. Do not require `Rq-Version` on a `304`.
2. **Delta.** A `200` whose `Content-Type` media type is `application/vnd.rivqen.delta+json` (ASCII case-insensitive):
   1. If the request did not offer a delta (no S, no `block-patch` or no delta media type in `Accept`), the delta is unsolicited: stop with `PATCH_INVALID` and do the fallback load (item 5). A server that follows section 4 never sends it (RQP-10).
   2. Validate D as [manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply) and the [diff engine §3](/engineering/core/diff-engine#_3-rqp-patch-validation) define. The checks include: size ≤ the client limit (`PATCH_TOO_LARGE`), `template_revision` = T (`TEMPLATE_MISMATCH`), `base_revision` = R (`PATCH_BASE_MISMATCH`), the hash of each operation (`INTEGRITY_ERROR`).
   3. Compute the page revision with T, the policy line of the manifest of S and the new block hashes (algorithm in [manifest and patch §1.3](/engineering/protocol/manifest-patch#_1-3-hashes-and-revisions)). It MUST be equal to `next_revision`. Else the result is discarded (`INTEGRITY_ERROR`). This check also detects a prefix collision in `Rq-Have` and a change of `policy` or `limits` on the server (H-55).
   4. Build the patched document on a copy and run the markup analysis on it; check the attribute rule (H-48; [manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)). A failure is `PATCH_CONTENT_REJECTED` (or `INTEGRITY_ERROR` when only the page revision differs).
   5. On any failure, show nothing of D and do the fallback load (item 5).
3. **Full document.** A `200` with an HTML media type is an RQP page only when it has the field `Rq-Version: 1`. Then the client finds and checks the manifest as [manifest and patch §1.7 and §1.8](/engineering/protocol/manifest-patch#_1-8-client-checks-of-the-manifest) define: exactly one manifest element (H-51), immediately before the first `</head>` (H-49), schema, origin and path, and the markup analysis of the original document against the manifest (H-48). The client compares the `type` value of the manifest script with `application/rivqen-manifest+json` by an ASCII case-insensitive byte match only, never by Unicode case folding (H-28). If a check fails (also when there is no manifest element), the client reports `MANIFEST_INVALID`, shows the document as a normal page and stores no snapshot. Without `Rq-Version: 1` the client does not use a manifest, also when the body has one, and reports no error: only a server SDK sets the field, and markup rule M-13 makes the server SDK reject a page that already has a manifest script. The revision of the new snapshot is the manifest page revision, not the `ETag`.
4. **Other.** Any other response is a normal HTTP response. The session is not RQP for this navigation ([versioning](/engineering/protocol/versioning)).
5. **Fallback load.** One `GET` without `Rq-Have`, without an RQP `If-None-Match` and without the delta media type in `Accept`. By section 4, the response cannot be a delta. If it is a delta, stop with `PATCH_INVALID` and let the WebView load the URL normally. The client does at most one fallback load per navigation.

### 6.4 Versions, capabilities and downgrade

1. The server does not announce capabilities. The client learns what the server does from the response: a manifest with `Rq-Version: 1` means RQP; a delta means `block-patch`.
2. The client ignores unknown `Rq-*` response fields and `Rq-Version` values other than `1` (the response is then not RQP).
3. A server ignores unknown capability tokens and unknown `Rq-Have` members.
4. Moves to legacy mode or to normal HTTPS follow [versioning](/engineering/protocol/versioning) (DG-01 to DG-05).

## 7. Limits

| Item | Limit | On violation |
|---|---|---|
| `Rq-Have` field value | 4096 bytes (`maxRqHaveBytes`) | Server: `Rq-Have` is absent. Client: never sends more |
| `Rq-Capabilities` field value | 1024 bytes (`maxRqCapabilitiesBytes`) | Server: field is absent |
| Blocks in `Rq-Have` member `b` | N (at most 256, `maxBlocks`) | Server: `b` is absent |
| Delta size | `maxPatchBytes` and smaller than F | Server: sends F. Client: `PATCH_TOO_LARGE`, fallback load |
| Fallback loads per navigation | 1 | Normal HTTPS load |

<Badge type="warning" text="RESEARCH" /> The 4096-byte budget comes from research R-6: about 8 KiB per field is the smallest common request field limit (nginx, Apache, Tomcat); [WP-17 S2](/engineering/plan/sprints/WP-17-S2) §1.

Both field limits are fixed by `rqp/1` (not configurable): a lower limit on one side would turn valid requests into full loads. Names: [resource limits](/engineering/core/resource-limits).

Server work for steps 4 to 7 is bounded: one parse of at most 4096 bytes and one of at most 1024 bytes, at most 256 comparisons of 8 bytes, and one envelope of at most `maxPatchBytes`. The block digests are the ones that the server computes for the manifest anyway. The attribute rule of step 7 (H-56) reads the start tags of the changed `html` blocks once; the markup pass of step 2 already tokenizes them, so the server can keep one result for each block and reuse it.

## 8. Rules

| ID | Rule |
|---|---|
| RQP-01 | A delta MUST use the media type `application/vnd.rivqen.delta+json`, MUST have `Cache-Control: no-store` and MUST NOT have an `ETag`. |
| RQP-02 | A server MUST NOT use status `226 IM Used` for RQP deltas. (RFC 3229 delta encoding is a different mechanism.) |
| RQP-03 | A `304` MUST NOT have content. It MUST have the `ETag` and the cache fields that F would have. |
| RQP-04 | A server that does not support RQP ignores the fields and sends a normal response. The client then treats the session as non-RQP. |
| RQP-05 | A client MUST NOT send `Rq-*` fields to origins outside its allowlist. |
| RQP-06 | A server MUST ignore unknown capability tokens and unknown `Rq-Have` members. A server MUST NOT send a delta to a request without `block-patch`. |
| RQP-07 | A revision is `t1.` or `r1.` followed by 43 base64url characters. Revisions are compared byte for byte. Clients compute revisions only to check a manifest or a delta (H-48). |
| RQP-08 | F MUST have the `ETag` `"r1.…"` (strong) or `W/"r1.…"` (weak, RQP-18), MUST NOT depend on RQP request fields (`Rq-*`, the delta media type in `Accept`, `If-None-Match`), and MUST NOT list an `Rq-*` field in `Vary`. |
| RQP-09 | For a page that is invalid for RQP, the server MUST send the application response unchanged: no manifest, no `Rq-*` field, no ETag that RQP computed. |
| RQP-10 | A server MUST NOT send a delta without a single `r1.` entity-tag in `If-None-Match` and a matching `Rq-Have` template. |
| RQP-11 | A delta MUST have at least one operation, MUST NOT be larger than `maxPatchBytes`, and MUST be smaller than F. Else the server sends F. |
| RQP-12 | A client MUST NOT send an `Rq-Have` value longer than 4096 bytes. A server MUST treat a longer value as absent, without parsing it. |
| RQP-13 | A client MUST check `base_revision` and `next_revision` of a delta. On failure it discards the delta and does one fallback load without delta fields. |
| RQP-14 | A client MUST compare the manifest script type by ASCII case-insensitive byte match only (H-28), MUST use a manifest only from a response with `Rq-Version: 1`, and MUST check it with the markup analysis of the original document (H-48). |
| RQP-15 | A client MUST send `If-None-Match` and `Rq-Have` only from a snapshot with the same cache key, and MUST NOT copy them to a redirected request. |
| RQP-16 | Preconditions are evaluated against F. A server SDK MUST NOT let the application answer `304` by a validator that the SDK replaces. |
| RQP-17 | Logs MUST NOT contain the `Rq-Have` member `b` or block content. Revisions, sizes and outcomes are allowed ([errors and logging](/engineering/architecture/errors-logging)). |
| RQP-18 | A server MUST send the weak `ETag` `W/"r1.…"` whenever a content coding is applied to F, and the strong `"r1.…"` only for the identity coding (H-52). A server MUST accept a weak tag as the delta base. |
| RQP-19 | A server MUST read `Rq-Capabilities` only as one Inner List of Tokens; any other shape is an absent field (H-53). |
| RQP-20 | A server MUST send the same F for every request to a valid page, also without RQP fields, with exactly one manifest element (H-51). |
| RQP-21 | A server MUST NOT send the response fields `Rq-Revision` and `Rq-Template-Revision`; a client MUST NOT send the tokens `manifest` and `json-blocks` (H-54). |
| RQP-22 | A release build of a client MUST NOT send `Rq-*` fields to an `http` origin. A debug build MAY, only with an explicit app flag (H-49). |
| RQP-23 | A server SHOULD send F instead of D when the current content of a changed `html` block fails the attribute rule of [manifest and patch §4.2](/engineering/protocol/manifest-patch#_4-2-attribute-rule) (H-56). A client MUST apply the attribute rule to every delta, also when the server checks it. |

## 9. Examples

<Badge type="tip" text="FACT" /> The examples use the known-answer set of [manifest and patch](/engineering/protocol/manifest-patch) (`spec/rqp/examples/kat/`). The revisions and digests below are the output of the Node.js parser candidate (`tools/rqp-markup/node`) for these two pages (2026-10-12), and the test `tools/rqp-schema/test/kat.test.mjs` checks them. Page A is `kat/page.html`:

```html
<!DOCTYPE html>
<html>
<head>
  <title data-rq-block="title">Shop</title>
</head>
<body>
  <header>Static header</header>
  <p class="price" data-rq-block="price">120.00</p>
  <script type="application/json" data-rq-block="cart">{"items":3}</script>
</body>
</html>
```

Page B is page A with `130.00` in place of `120.00` (each file ends with a line feed).

```text
Both pages
  template revision  t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM
  title prefix (hex) d00aae6b7fbf77a4
  cart prefix (hex)  cf210bdca91994b9
Page A
  page revision      r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8
  price prefix (hex) 5e0de86a2a25df2c
Page B
  page revision      r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI
  price prefix (hex) 6c1f527187104332
```

Headers that do not matter for RQP are not shown. The bodies of F and D follow [manifest and patch](/engineering/protocol/manifest-patch): F of page A is `kat/full.html` (799 bytes; another server can write the manifest with other white space), D of section 9.3 is `valid/patch/shop-price.json`. The manifest of these examples has no `policy` and no `limits`. With them, the page revisions are different (H-55): page A `r1.ke0dIjtk…` and page B `r1.0MisoW_J…` for the values of the second known-answer set ([manifest and patch §1.3](/engineering/protocol/manifest-patch#_1-3-hashes-and-revisions)); the `Rq-Have` value stays the same.

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
ETag: "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8"
Rq-Version: 1
Cache-Control: no-cache
```

Body: F of page A (page A with the manifest immediately before `</head>`). Section 4, step 8.

A browser without RQP sends no `Rq-*` field and gets the same response with the same body (H-51). When the request has `Accept-Encoding: br` and the server compresses F, only the coding fields and the ETag form change (H-52):

```http
HTTP/2 200
Content-Type: text/html; charset=utf-8
Content-Encoding: br
Vary: Accept-Encoding
ETag: W/"r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8"
Rq-Version: 1
Cache-Control: no-cache
```

### 9.2 Not modified

The client has a snapshot of A. The current page is still A.

```http
GET /catalog HTTP/2
Host: shop.example.com
Accept: text/html, application/vnd.rivqen.delta+json
Rq-Version: 1
Rq-Capabilities: (block-patch)
If-None-Match: "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8"
Rq-Have: t=t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM, b=:0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:
```

```http
HTTP/2 304
ETag: "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8"
Cache-Control: no-cache
```

No body. Section 4, steps 4 and 9. If the client had received the weak tag of section 9.1, it still sends the strong form from its manifest; the weak comparison matches. With `Accept-Encoding: br` the `304` carries `ETag: W/"r1.RdZd…"`, the tag that F with that coding has.

### 9.3 Delta

The same request as in 9.2. The current page is B. Only the `price` prefix differs.

```http
HTTP/2 200
Content-Type: application/vnd.rivqen.delta+json
Rq-Version: 1
Cache-Control: no-store
```

Body: D with `base_revision` `r1.RdZd06nC…`, `next_revision` `r1.psbsc6wh…`, `template_revision` `t1.86FtOeBe…` and one `replace_block` operation, for block `price` (content `130.00`): the file `valid/patch/shop-price.json`. Section 4, step 7. Without the member `b` in `Rq-Have`, D has three operations (all blocks, H-7), if it is still smaller than F.

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
5. **Injected manifest.** A manifest is used only with `Rq-Version: 1` (RQP-14). A page with a manifest-like script is invalid for RQP (markup rule M-13), so it is sent without `Rq-Version`. The client accepts exactly one manifest, only immediately before the first `</head>`, and only when the markup analysis of the original document gives the same blocks and revisions (H-48, H-49, H-51).
6. **Patch content.** A delta cannot bring script into the page: the client runs the markup analysis on the patched document and the attribute rule on patched `html` content ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules), H-48). A rejection is `PATCH_CONTENT_REJECTED`, a `SecurityViolation` with a metric (H-50).
7. **Cleartext.** A release build uses RQP only over `https` (H-49, RQP-22).

## 11. Intermediaries

<Badge type="warning" text="RESEARCH" /> The intermediary test runs in WP-13 (with WP-17). It checks for each CDN and proxy in the support matrix: `Rq-*` request fields arrive unchanged; `Rq-Version` in responses arrives unchanged; a delta with `no-store` is not stored; `ETag` and `If-None-Match` pass (strong or weakened); a component that compresses F weakens a strong `ETag` and does not remove it (H-52, section 5.1); a cached F is served to a delta-eligible request without harm. Research R-6 found that Cloudflare ignores `Vary` by default; RQP does not depend on `Vary` (section 5). An intermediary that removes `Rq-Version` turns RQP off for its clients (section 6.3, item 3): the pages still work as normal HTML.

## Related

- [RQP markup](/engineering/protocol/markup)
- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Versioning and downgrade](/engineering/protocol/versioning)
- [Diff engine](/engineering/core/diff-engine)
- [Resource limits](/engineering/core/resource-limits)
- [Golden fixtures](/engineering/protocol/fixtures)
