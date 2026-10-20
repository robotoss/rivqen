# RQP: negotiation and headers

The Rivqen protocol (RQP) has explicit versions, capabilities and revisions. This page defines how a client and a server agree on RQP, which headers they send, how the server selects the response (full document, delta or `304`), how caches handle the responses, and what the client checks.

**Status:** <Badge type="info" text="DESIGN" /> Normative for `rqp/1` from WP-17 S2. Decisions: `Rq-` header prefix and Structured Field syntax ([ADR-006](/engineering/architecture/adr/#adr-006)); the `dict-compression` token is reserved (H-4); the client sends its block digests, with an all-blocks fallback (H-7); content-derived revisions `t1.`/`r1.` (H-8); ASCII case-insensitive manifest type match (H-28); `Rq-Have` with 64-bit digest prefixes (H-44); caching of full document and delta (H-45); `If-None-Match` carries the client's page revision (H-46); client markup analysis (H-48); `http` only in debug builds (H-49); error codes (H-50); the same full document for every request (H-51); a weak ETag with a content coding (H-52); `Rq-Capabilities` is an Inner List of Tokens (H-53); withdrawn tokens and response headers (H-54); `policy` and `limits` of the manifest are part of the page revision (H-55); the server sends F when a changed block fails the client attribute rule (H-56); the client binds a `304`, a manifest and a delta to its request and to the cache key of the final URL (H-58); the attribute rule also denies `vbscript:` and `data:` (H-59); privacy of the digests (H-60); the `public` partition (H-61); the client uses a response only from an allowlisted final request, unknown credentials count as present, and UTF-8 only (H-62); a `Set-Cookie` field always gives the partition of the request (H-63); the `public` partition only for a response whose header fields the native SDK saw in full (H-64); the result of a delta on a `public` snapshot (H-65); F has exactly `Content-Type: text/html; charset=utf-8`, and the client gives F and every snapshot to the WebView as UTF-8 (H-66); a result that comes through a redirect never goes to the `public` partition (H-67); no RQP for a URL with a user name or a password (H-68); the SDK rejects an allowlist entry that is not its own serialized origin (H-69); the manifest goes immediately after the head start tag of the document, and the client finds it as the first token after that tag (H-72, [WP-17 S3](/engineering/plan/sprints/WP-17-S3) §1; replaces the insertion point of H-49 and DL-031); the manifest has no block list and no template revision, the client computes them and uses them after the page revision check, also for `Rq-Have` (H-103); the delta envelope is `next_revision` and the operations, and the client checks it only through `next_revision` (H-104); no further compaction and 64-bit prefixes in `Rq-Have` (H-105, H-106); the member `binding` over the URL, the base revision and the next revision replaces `next_revision` (H-107; [WP-17 S6](/engineering/plan/sprints/WP-17-S6) §1). Architect decisions: the server removes the `Rq-*` fields of the application response (DL-030); the head end tag of the document (DL-031, replaced by H-72); no `Last-Modified` on F (DL-032); the exact claim of the attribute rule, `Rq-Version` on a `304` and the partition check for a `304` (DL-036); the details of the charset rule and of H-62 (DL-037); the exact origin match of the allowlist, `Set-Cookie2`, and two client charset cases (DL-038); the details of H-64 to H-66, with the choice between two entries of one cache key (DL-039); the delivery of the bytes to the WebView, the origin from the URL object of the network stack, and the metadata of a `public` entry after a `304` (DL-040); the client nonce `n` in `Rq-Have` and in the delta binding (H-108, [WP-17 S6](/engineering/plan/sprints/WP-17-S6) §1). Records: [WP-17 S1](/engineering/plan/sprints/WP-17-S1) §1 and [WP-17 S2](/engineering/plan/sprints/WP-17-S2) §1. Block markup: [RQP markup](/engineering/protocol/markup). Manifest, patch envelope and revision algorithm: [RQP manifest and patch](/engineering/protocol/manifest-patch).

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
| Snapshot | A page that the client holds for one cache key ([cache identity](/engineering/cache/identity)): the original document, the page revision, `policy` and `limits` of its manifest, and the template revision and block hashes that the client computed with its markup analysis and checked through the page revision (H-103) |
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
- Response: the server sends `Rq-Version: 1` on every full document and every delta of a valid page, as exactly one field line. It MAY send it on a `304`, also as exactly one field line with the value `1` (DL-036). Only the RQP server step sets the field: it removes every `Rq-*` field of the application response first (section 4, DL-030).

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
| `t` | Token | The template revision of the client's snapshot (`t1.…`), as the client computed it (H-103) |
| `b` | Byte Sequence | For each block of the snapshot in document order (the order of the markup analysis, markup M-27): the first 8 bytes of the raw SHA-256 digest of the block content. Concatenated, no separator |
| `n` | Byte Sequence | The client nonce of this request (H-108): exactly 16 bytes from a cryptographically secure random number generator, new for every request and never used again. The server binds a delta to it ([manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields)) |

Example (3 blocks, 24 bytes in `b`; values of [§9](#_9-examples); the nonce of the known-answer set, the bytes 00 to 0F):

```http
Rq-Have: t=t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM, b=:0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:, n=:AAECAwQFBgcICQoLDA0ODw==:
```

The prefix is taken from the digest bytes, not from the base64url text. The manifest has no block hashes (H-103): the client takes the template revision and the digests from its markup analysis of the snapshot, which the page revision check of the manifest confirmed ([manifest and patch §1.8](/engineering/protocol/manifest-patch#_1-8-client-checks-of-the-manifest), item 5). After a patch, the digests of the changed blocks are the ones that the client computed from the new contents ([manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply), step 4).

**Client.** The client MUST NOT send an `Rq-Have` field value longer than 4096 bytes. With at most 256 blocks the value is at most 2816 bytes (`b` has 2048 bytes, 2732 base64 characters; `, n=:` and the 24 base64 characters of the nonce and `:` add 30 bytes), so a client with a valid snapshot never reaches the limit. A client sends `b` always when it sends `Rq-Have`.

**Client nonce (H-108).** <Badge type="info" text="DESIGN" /> A client sends `Rq-Have` only with the member `n`: 16 bytes that it takes from a cryptographically secure random number generator (CSPRNG) for this request, written as a Byte Sequence with padding (24 base64 characters). It MUST NOT use a nonce for a second request, also not for a retry of the same navigation, and MUST NOT derive it from a counter, a time or another value that a party can predict. It keeps the nonce with the request and uses it for the `binding` check of the delta that answers this request (section 6.3, item 2.3); after it has handled the response (or the navigation is cancelled) it discards it. A client that cannot get a nonce (its CSPRNG fails) sends no `Rq-Have` and no delta media type in `Accept`; it can still send `If-None-Match`, so a `304` stays possible (fail closed: no delta, never a fixed value). The nonce is not secret: TLS protects it as any other request field.

**Server.** The server reads the field in this order:

1. If the field value (all field lines combined with `, ` — comma and one space — between them, RFC 9651 §4.2; DL-029) is longer than 4096 bytes (`maxRqHaveBytes`), treat `Rq-Have` as absent. Do not parse it.
2. Parse the field as a Dictionary. If the parse fails, treat `Rq-Have` as absent.
3. If the member `t` is missing or is not a Token, treat `Rq-Have` as absent. Ignore parameters of `t`.
4. The client template matches when `t` is equal, byte for byte, to the current template revision. If it does not match, stop: `b` is not used.
5. If the member `b` is missing or is not a Byte Sequence, `b` is **absent**.
6. Let L be the length of the decoded `b` in bytes. If L is not a multiple of 8, or L is not 8 × N, `b` is **absent**. (The template matches, so the client page has the same N blocks.)
7. **Nonce (H-108).** If the key `n` appears more than one time in the field value (also in two field lines, and also when every copy is valid), `n` is **absent**. If the member `n` is missing, is not a Byte Sequence (for example a String, a Token, an Integer, a Boolean or an Inner List), or its decoded length is not exactly 16 bytes, `n` is **absent**. Parameters of `n` are ignored. Without `n` the request is not delta-eligible (section 4, step 5.7): the server sends F (or a `304` by step 4).
8. Ignore other members and all parameters. They are reserved for later versions.

<Badge type="tip" text="FACT" /> RFC 9651 §4.2.2: when a Dictionary has a key two times, the parser overwrites the value of the first member with the later one. So `t=X, b=:…:, t=Y` has `t` = `Y` (`FX-RQ-P34-HAVE-DUPLICATE-KEY`). The client sends each key once.

<Badge type="info" text="DESIGN" /> **A repeated `n` is no `n` (step 7; fail closed).** For `t` and `b` the last value wins, as RFC 9651 says; for `n` the server needs the count of the key, not only the parsed value: a field with two nonces does not name one request, so the server sends F (`FX-RQ-P42-NONCE-DUPLICATE`: two valid values). RFC 9651 §2.2 allows this "other handling" of a member. A server whose parser gives only the last value counts the key `n` in the field value (for example with a second pass over the member keys) before it uses the value.

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

<Badge type="info" text="DESIGN" /> Weak tags are accepted for the base (H-52): the server itself sends `W/"r1.…"` with a content coding (section 5.1), and an intermediary can weaken a strong tag (section 5.4). B is the `r1.…` value in both forms. B makes the request delta-eligible (section 4, step 5), and it is an input of the `binding` of D (H-107, with the nonce N of H-108; [manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields)); the change set comes from `Rq-Have`. A wrong base (for example an intermediary that replaces the tag, section 5.4) gives a binding that the client cannot match: `INTEGRITY_ERROR` and one fallback load (section 6.3), never a wrong page. D does not carry B as a value: the client puts the page revision of its own snapshot into the binding. An `rqp/1` client always sends the strong form, with the page revision of its manifest.

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
| Template revision | `t1.` + 43 base64url characters (46 in total) | `Rq-Have` member `t`. Not in the manifest and not in the envelope (H-103, H-104): both sides compute it |
| Page revision | `r1.` + 43 base64url characters (46 in total) | Manifest (`page_revision`), `ETag`, `If-None-Match`; in the patch envelope only inside the preimage of `binding` (B and R′, H-107) |

The algorithm that computes them is normative in [manifest and patch §1.3](/engineering/protocol/manifest-patch#_1-3-hashes-and-revisions) (H-8). The page revision covers the template, the block contents and the manifest members `policy` and `limits` (H-55). Servers compute revisions with that algorithm. Clients compute the template revision, the block hashes and the page revision with the same algorithm, and use their computed values only after a check against a value of the server: the markup analysis of the original document of F against `page_revision` of its manifest, the `binding` of a delta over the computed next page revision R′ (H-107), and the markup analysis of the patched document against R′ (H-48, H-103, H-104; section 6.3). Revisions are compared byte for byte.

### 3.8 Response headers

| Response | Headers that RQP sets |
|---|---|
| Full document F | `ETag: "r1.…"` (identity coding) or `ETag: W/"r1.…"` (content coding, H-52); `Rq-Version: 1`; exactly `Content-Type: text/html; charset=utf-8` (H-66). `Cache-Control` of the application stays. No `Last-Modified` (DL-032) |
| Delta D | `Content-Type: application/vnd.rivqen.delta+json`; `Rq-Version: 1`; `Cache-Control: no-store`. No `ETag` |
| `304` | The `ETag` that F would have; the fields of section 4, step 9 |
| Invalid page, out of scope | None. The server removes the `Rq-*` fields of the application response; the status, the other fields and the content stay (DL-030) |

<Badge type="info" text="DESIGN" /> In every outcome, the response has no `Rq-*` field of the application response (DL-030). An application, a framework or an upstream layer that sets `Rq-Version` would otherwise make a client trust a manifest that the server SDK rejected (section 10, item 5).

<Badge type="info" text="DESIGN" /> The draft response headers `Rq-Revision` and `Rq-Template-Revision` are removed (H-54). A server MUST NOT send them. F carries the page revision in the manifest and in `ETag`; the client computes the template revision (H-103). D carries `binding` in the envelope (H-107); the client computes the next page revision. A second copy in a header can disagree with the body, and the client would need a rule for that case.

## 4. Response selection (server)

<Badge type="info" text="DESIGN" /> The server runs these steps for each request. The first step that sends a response ends the algorithm.

Inputs: the request; the application response A (status, header fields, page bytes P); the server limit `maxPatchBytes` ([resource limits](/engineering/core/resource-limits), default 262144 bytes).

**`Rq-*` fields of A (DL-030).** Before step 1, the server removes from A every header field whose name starts with `Rq-` (ASCII case-insensitive), in every outcome: passthrough (steps 1 and 2), F, D and `304`. In the steps below, "A" means A without these fields. Only the server step sets `Rq-*` response fields.

1. **Scope.** If the request method is not `GET` or `HEAD`, or the status of A is not `200`, or the media type of A is not `text/html`, send A (status, other header fields and content unchanged). RQP does not apply.
2. **Validity.** If the `Content-Type` of A has a charset that is not UTF-8 (the charset rule below; H-62), P is invalid for RQP. Analyze P with the [markup rules](/engineering/protocol/markup) (section 2, "invalid for RQP"; H-12, H-15). If P is invalid for RQP, or has no insertion point for the manifest (no head start tag of the document and no implied head; [manifest and patch §1.7](/engineering/protocol/manifest-patch#_1-7-delivery-in-the-document), "Document head"; H-72, H-79a), send A (status, other header fields and content unchanged). The response has no manifest, no `Rq-Version` and no other `Rq-*` field, and no ETag that the server computed (an `ETag` of A stays as it is). The server SDK reports the broken rule to the template author, not to the client. An internal error of the server SDK has the same result and is logged as `SERVER_PASSTHROUGH` ([errors](/engineering/core/errors)).
3. **Full document.** Build F: insert the manifest into P at the insertion point: immediately after the head start tag of the document, or immediately before the start tag that implies the head ([manifest and patch §1.7](/engineering/protocol/manifest-patch#_1-7-delivery-in-the-document), server item 2; H-72, H-79a). F is the same for every request (H-51). Let E be the entity-tag of F: the strong `"` + page revision + `"` when the response has no content coding, and the weak `W/"` + page revision + `"` when a content coding is applied (H-52, section 5.1). The header fields of F are those of A, with these changes: `ETag: E` replaces an `ETag` of A; remove `Last-Modified` (DL-032); add exactly one field line `Rq-Version: 1`; set `Content-Length` for F; replace the `Content-Type` of A with exactly one field line `Content-Type: text/html; charset=utf-8`, whatever the text of the `Content-Type` of A was (H-66; charset rule below). Do not add an `Rq-*` field or `Accept` to `Vary`. Keep `Cache-Control` of A (H-45).
4. **Conditions.** Evaluate the preconditions of the request against F, in the order of RFC 9110 §13.2.2. If `If-None-Match` is present, ignore `If-Modified-Since`. If the `If-None-Match` condition is false (`*`, or one listed tag matches E by weak comparison; so `"r1.X"` and `W/"r1.X"` both match), go to step 9.
5. **Delta eligibility.** The request is delta-eligible only when all of these are true. Else go to step 8.
   1. The method is `GET`, and the request has no `Range` field.
   2. `Rq-Version` is `1` ([§3.2](#_3-2-rq-version)).
   3. The capability set has `block-patch` ([§3.3](#_3-3-rq-capabilities)).
   4. `Accept` accepts the delta media type ([§3.6](#_3-6-accept-and-the-delta-media-type)).
   5. `If-None-Match` gives a base B ([§3.5](#_3-5-if-none-match)). B is not the current page revision, because step 4 did not match.
   6. `Rq-Have` is present and its template matches ([§3.4](#_3-4-rq-have), steps 1 to 4).
   7. `Rq-Have` has a valid nonce `n`: exactly one member `n`, a Byte Sequence of 16 bytes ([§3.4](#_3-4-rq-have), step 7; H-108). Fixtures `FX-RQ-P42-NONCE-*`.
6. **Change set.** If `b` is present: the change set C has each block i (0-based, document order) whose current digest prefix (the first 8 bytes of SHA-256 of its content) is not equal to the bytes 8i to 8i + 7 of `b`. If `b` is absent: C has all N blocks (H-7). If C is empty, go to step 8.
7. **Delta.** If the current content of an `html` block in C fails the patch content rule of [manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules), item 1 (an element of `PATCH_FORBIDDEN`, a foreign element outside `FOREIGN_ALLOWED`, or the attribute rule of §4.2), the server SHOULD go to step 8 (H-56, H-78). Build D ([manifest and patch §2](/engineering/protocol/manifest-patch#_2-patch-envelope)) with `binding` = the binding of U, B, N and R′ ([manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields); H-107, H-108: U = the canonical URL of this request, B = the base of step 5.5, N = the nonce `n` of this request (step 5.7) as base64url without padding, R′ = the current page revision) and one operation (`block_id`, `content`) for each block in C, in document order, and no other member (H-104). If the size of D in bytes is larger than `maxPatchBytes`, or is not smaller than the size of F, go to step 8. Else send D: status `200`, the header fields of A without `ETag`, `Last-Modified` and `Expires`, with `Content-Type: application/vnd.rivqen.delta+json`, `Rq-Version: 1`, `Cache-Control: no-store`, and `Content-Length` for D.
8. **Full.** Send F with status `200`. For `HEAD`, send the header fields of F without content.
9. **Not modified.** Send `304` without content. Include the fields that F would have of this list: `ETag` (E), `Cache-Control`, `Expires`, `Vary`, `Date`, `Content-Location` (RFC 9110 §15.4.5). `Rq-Version: 1` MAY be included, as exactly one field line with the value `1` (DL-036). No other field of A, so no `Rq-*` field of A, no `Last-Modified` and no `Content-Type`.

**Charset rule (H-62, H-66).** <Badge type="info" text="DESIGN" /> RQP is for UTF-8 documents only. The server reads the `Content-Type` of A in step 2 as follows. The rule only decides if the page is passed through; it never decides the `Content-Type` of F, which is always exactly `text/html; charset=utf-8` (step 3, H-66):

1. The field has exactly one field line with exactly one media type (no comma outside a quoted string). Else the charset is not UTF-8. This fails closed: <Badge type="tip" text="FACT" /> the Fetch Standard ("extract a MIME type") uses the last media type of a list and can keep the charset of an earlier one; this rule does not model that.
2. The charset is the value of the first valid parameter whose name is `charset` (ASCII case-insensitive), read with the parameter steps of "parse a MIME type" (WHATWG MIME Sniffing §4.4): a quoted value loses its double quotes and its backslash escapes; an unquoted value loses its trailing white space, and an empty unquoted value does not count. Without such a parameter, A has **no charset**.
3. The charset is **UTF-8** when, without its leading and trailing ASCII white space, it is equal to one of these labels by ASCII case-insensitive comparison: `unicode-1-1-utf-8`, `unicode11utf8`, `unicode20utf8`, `utf-8`, `utf8`, `x-unicode20utf8`. Any other value (also an empty quoted value) is not UTF-8.
4. When the charset of A is not UTF-8 by items 1 to 3, P is invalid for RQP (step 2). When A has no charset or a UTF-8 charset, the server continues with the markup analysis; F then has the field of step 3. The text of the `Content-Type` of A (a quoted or upper-case label, other parameters, an unclosed quoted string, white space) never reaches F (H-66).

<Badge type="info" text="DESIGN" /> **Why one exact value (H-66).** Before H-66, the server kept the `Content-Type` of A, or appended `; charset=utf-8` to it, and parsed the result again. Security review round 3 (L-1): a WebView stack can parse an unusual `Content-Type` text in another way than the WHATWG algorithm of items 1 to 3. Unusual texts are, for example, a quoted label, a parameter name with white space, or a quoted string without its end quote (the cases `FX-RQ-P40-*`). One exact value removes this parser difference: every browser and every WebView reads `text/html; charset=utf-8` as UTF-8. A page whose application sends no charset, or a UTF-8 label in any form, gets F; only a charset that is not UTF-8 (or a field that items 1 to 3 cannot read as one value) passes the page through. A page with an unclosed quoted string and no charset parameter is therefore no longer passed through: the old reparse was only needed because of the appended text.

<Badge type="tip" text="FACT" /> WHATWG Encoding Standard §4.2 "Names and labels" (checked on 2026-10-17; the standard was last updated on 21 May 2026): these six labels are the labels of the encoding UTF-8, and "get an encoding" removes leading and trailing ASCII white space and compares labels ASCII case-insensitively. WHATWG HTML, "the encoding sniffing algorithm": BOM sniffing comes first (step 1, confidence certain), then a character encoding that the transport layer specifies (step 4, certain), and only after that the `<meta>` prescan and other detection. <Badge type="info" text="DESIGN" /> The server and the client read P as UTF-8 (markup M-02). A browser that decodes the document with another encoding (for example `ISO-2022-JP`, whose escape sequences change how the next bytes decode, or an auto-detected legacy encoding) can see other characters in place of bytes such as `">` or `<head>`. Then its block boundaries and its manifest position are not the ones that RQP computed (security review S2 round 2, L3). With the UTF-8 charset in F, step 4 of the browser algorithm gives UTF-8 before any `<meta>` prescan or detection. The replaced field does not change the ETag or the `304`: the page revision covers the bytes of F, and the field is the same for every request to the page (H-51). When the client, not the WebView, fetches F or shows a snapshot, it gives the bytes to the WebView as UTF-8 itself (section 6.3, "Delivery to the WebView"; H-66).

Sizes in step 7 are the bytes before any content coding. The comparison of digest prefixes in step 6 does not need a constant-time function: the values are not secret.

| Outcome | Step | Status | Body |
|---|---|---|---|
| Out of scope | 1 | As A | The content of A (fields without `Rq-*`) |
| Invalid for RQP (also a charset that is not UTF-8, H-62) | 2 | `200` | P (fields of A without `Rq-*`) |
| Not modified | 4, 9 | `304` | None |
| Delta | 7 | `200` | D |
| Full document | 8 | `200` | F, with exactly `Content-Type: text/html; charset=utf-8` (H-66) |

<Badge type="info" text="DESIGN" /> Decisions in these steps:

- **One full representation (H-51).** A request that is not an RQP request, or has a version that the server does not support, gets F, as an RQP request does. F does not depend on any RQP request field, so caches need no `Vary` for RQP (H-45). A client without RQP ignores the manifest script (its type is not a script type that a browser runs) and the `Rq-Version` field.
- **Empty change set → F.** With a base B that is not the current revision, an empty change set means that the client has a prefix collision or inconsistent headers. A `304` would be wrong (the client does not have E). An empty delta would fail the client check of `binding` and cost a second request. F is correct in one response.
- **No base → F.** B is a condition of step 5 (RQP-10) and an input of `binding` (H-107): a request without exactly one page revision in `If-None-Match` names no snapshot, so the server has no B to bind. The client checks with `binding` that D was built for its page (B), for its URL (U), as the answer to its request (N) and gives the page revision that it computes (R′).
- **No nonce → F (H-108).** A request without a valid `n` (section 3.4, step 7) gets F. A delta without a nonce of the request would apply to every request with the same U and B (S6 security re-review B1); F is correct for every request. A `304` does not need `n` (`FX-RQ-P42-NONCE-MISSING-NOT-MODIFIED`).
- **Size.** At equal size, F is better: it can be cached and needs no client work.
- **Patch content rule → F (H-56, H-78).** The client rejects a delta whose `html` content fails the patch content rule (elements or attributes) and then loads F: two requests. When the server sends F at once, the client needs one request. The client check stays (MUST); the server check is SHOULD because the result is the same page in both cases. The result depends only on the content of the block, so the server can compute it once for each block hash.
- **`Rq-*` fields of A are removed (DL-030).** The server SDK owns every `Rq-*` field. An `Rq-Version` that the application or an upstream layer sets on a page that the SDK passes through would make a client use a manifest that the SDK rejected, for example an author manifest (markup rule M-13). In a layered deployment (an SDK at the origin and one at the edge) the outer SDK removes the fields of the inner one and sets its own; the only loss is a duplicate.
- **No `Last-Modified` on F (DL-032).** The bytes of F also depend on the manifest (`policy`, `limits`, `origin`), which a `Last-Modified` of the application does not cover. A cache that revalidates only with `If-Modified-Since` could keep an old F. The ETag `r1.` is the only validator of F. D has no validator at all (step 7).
- **Change of `policy` or `limits` only → F.** The page revision changes (H-55), so step 4 does not match, but the change set is empty: the server sends F, and the client gets the new values. When blocks changed too, the client computes R′ with its old policy line, its binding differs, and it rejects D (`INTEGRITY_ERROR`), then loads F: one extra request after a change of the server configuration ([manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply)).

## 5. Caching

### 5.1 Full document

F keeps the cache policy (`Cache-Control`, `Expires`) of the application (H-45), but not its `Last-Modified`: the ETag is the only validator of F (DL-032). F is the same for all requests to the URI that have the same application output (H-51), so it has no `Vary` on `Rq-*` fields or on `Accept`. `Vary` fields of the application (for example `Accept-Encoding`, `Cookie`) stay.

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

<Badge type="info" text="DESIGN" /> **A nonce-bound delta is never reusable (H-108).** The `binding` of D has the nonce of the request that D answers, and every request has a new nonce. So no other request can use D, also not a later request of the same client from the same snapshot: a stored D has no value for any cache. `no-store` stays: it keeps D out of caches, and a client that accepted a stored D only by mistake would still reject it (step 7 of [manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply)). For the same reason `Vary: Rq-Have` would only add a cache entry that never matches.

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
| A cache replaces the client's tag with its own tag | The server builds D from the `Rq-Have` of the client and binds it to the tag that it got (B, H-107) | The client puts its own revision into the binding: when the tags differ, `INTEGRITY_ERROR` and one load of F (section 6.3); never a wrong page |
| A cache ignores `no-store` and returns a stored D to another request | The client gets a D that the server built for another request | D applies only for the request that sent the nonce N of its binding (H-108), so it applies to no other request: no rollback (also not after A → B → A), no stale replay, no D of another base, another user or another URL. The assumptions (an honest server, TLS, a CSPRNG nonce) are in manifest and patch §2.2, "Replay" |
| A cache or server weakens the ETag (`W/"r1.…"`), for example for a content coding | `304` still works (weak comparison); the base for a delta still works (section 3.5) | The client uses the manifest, not the ETag, as its revision |
| A cache stores F with `W/"r1.X"` and gets a request with `If-None-Match: "r1.X"` | The cache sends `304` (weak comparison) | The client has the page `r1.X` |

<Badge type="info" text="DESIGN" /> Decided by the human (H-52): the server sends one weak tag for all content codings of F, so the distinct-tag rule of RFC 9110 §8.8.3 (section 5.1) holds. RQP clients do not use range requests for documents. <Badge type="tip" text="FACT" /> RFC 9110 §13.1.5: a client MUST NOT send a weak entity-tag in `If-Range`, and the server compares it by the strong function, so a weak tag never joins parts of two codings.

A page that wants each navigation to reach the server, so that the server can send a delta, uses a cache policy that makes caches revalidate (for example `Cache-Control: no-cache`). This is the choice of the application.

## 6. Client rules

### 6.1 Where the client sends RQP fields

1. The client MUST NOT send `Rq-*` fields to origins outside its allowlist (RQP-05). **Origin match (DL-038).** The client serializes the origin of the URL as the WHATWG URL Standard does: the scheme, the host in lower case and in its IDNA (`xn--`) form, and the port only when it is not the default port of the scheme ([cache identity §2](/engineering/cache/identity#_2-url-canonicalization), steps 1 and 2). It compares this string with each allowlist entry by exact match. There are no wildcards, no suffix or subdomain match, and no other normalization at match time. So `https://SHOP.example.com` and `https://shop.example.com:443` are the origin `https://shop.example.com`, and `https://shop.example.com.attacker.net` and `https://www.shop.example.com` are other origins. Each allowlist entry is a serialized origin; an entry in another form (upper case, a path, a trailing `/`, a default port, `*`) never matches.
   - **Configuration (H-69).** The SDK MUST reject the configuration with a clear error (it names the entry by its index and a reason, never by its raw text, which can hold a password; DL-042) when an allowlist entry is not equal to its own serialized origin: it parses the entry with the WHATWG URL parser, serializes the origin of the result, and compares the two strings. A string that the parser rejects is also an error. So the SDK never starts with an entry that can never match. Entries that fail: `https://Shop.example.com` (upper case), `https://shop.example.com:443` (a default port), `https://shop.example.com/` (a trailing `/`), `https://shop.example.com/catalog` (a path), `https://user@shop.example.com` (user information), `https://faß.example` (a Unicode IDN host; the entry is `https://xn--fa-hia.example`), and `*` or `shop.example.com` (not a URL). The SDK MUST also reject an entry whose host contains `*` (H-70) and an entry whose scheme is not `https` or `http` (DL-041; only these schemes can get RQP, rule 5). <Badge type="tip" text="FACT" /> The WHATWG host parser accepts `*` in a domain, so without H-70 the comparison would accept `https://*.example.com`; it is an exact host, never a wildcard (Node.js 26.8.2, `new URL('https://*.example.com').origin`, checked on 2026-10-18). With H-70 such an entry is a configuration error.
   - **The URL that the client checks (DL-040, L-3).** The SDK takes the origin from the URL object that its network stack used for the request (for the final request, for example OkHttp `Response.request.url` or `URLResponse.url`). It never parses a URL string again with another parser: two parsers can give two hosts for one string. <Badge type="tip" text="FACT" /> The WHATWG URL parser reads `\` as `/` in an `https` URL, so `https://attacker.example\@shop.example.com/` has the host `attacker.example` and no user information, and `https://shop.example.com\@attacker.example/` has the host `shop.example.com`; it maps a Unicode host with UTS 46 nontransitional processing, so `faß.example` is `xn--fa-hia.example` (transitional processing gives `fass.example`). Fixtures `FX-RQ-C25-FINAL-BACKSLASH-*` and `FX-RQ-C25-IDN-HOST` pin these results.
2. The client sends RQP fields only on same-origin requests or on requests that the native SDK makes itself (for example an intercepted navigation). <Badge type="tip" text="FACT" /> `Rq-*` fields and `If-None-Match` are not CORS-safelisted request headers (Fetch Standard, "CORS-safelisted request-header": only `Accept`, `Accept-Language`, `Content-Language`, `Content-Type` and `Range`, each with value rules). A cross-origin `fetch()` with them needs a CORS preflight. RQP is for same-origin navigation; it defines no CORS use.
3. The client sends `If-None-Match` and `Rq-Have` only from a snapshot with the same cache key as the request (same partition, same canonical URL; [cache identity](/engineering/cache/identity)). The partition of that snapshot is the partition of the request, or the shared `public` partition for a public page ([manifest and patch §1.4](/engineering/protocol/manifest-patch#_1-4-policy); H-61, H-65).
4. The client MUST NOT copy the fields of one request to a redirected request. The client handles a redirect as a new request and builds its RQP fields for the new URI (rules 1 to 3).
5. A release build sends RQP fields only to `https` origins. A debug build also sends them to an `http` origin, but only when the app sets an explicit flag for it (H-49; [manifest and patch §1.6](/engineering/protocol/manifest-patch#_1-6-origin-and-path)). Without RQP fields the request is a normal load.
6. **No RQP for a URL with user information (H-68).** The client MUST NOT send RQP fields to a URL that has a user name or a password (the WHATWG URL "includes credentials"), also when its origin is in the allowlist. It uses no full document, `304` or delta of such a request and stores no snapshot for it; the page loads normally, with no error code. The rule applies to the request URL and to the final URL after a redirect. Reason: the origin has no user information, so the allowlist alone accepts such a URL, and the password would go into the cache key ([cache identity §2](/engineering/cache/identity#_2-url-canonicalization)). An empty user information (`https://@shop.example.com/`) is not kept by the parser and does not count. Fixtures `FX-RQ-C25-USERINFO-*`.

### 6.2 Request

With a snapshot S (the template revision T and the block hashes that the client computed, H-103; the page revision R of its manifest):

```http
GET /catalog HTTP/2
Host: shop.example.com
Accept: text/html, application/vnd.rivqen.delta+json
Rq-Version: 1
Rq-Capabilities: (block-patch)
If-None-Match: "R"
Rq-Have: t=T, b=:<prefixes>:, n=:<nonce>:
```

`<nonce>` is the base64 form of 16 new random bytes for this request (section 3.4, "Client nonce"; H-108).

Without a snapshot, the client sends `Rq-Version: 1` and `Rq-Capabilities`. It does not send `Rq-Have` or an RQP `If-None-Match`, and it SHOULD NOT list the delta media type in `Accept`.

The client sends `Rq-Capabilities` as one field line with one Inner List of Tokens (H-53), and each field (`Rq-Have`, `Rq-Capabilities`) only once. It sends only the tokens of section 3.3 that it supports, never `manifest` or `json-blocks` (H-54).

### 6.3 Response

The **final URL** is the URL of the response after all redirects, and the **final request** is the request to it. Its **cache key** is the key of [cache identity](/engineering/cache/identity): the partition and the canonical URL (scheme, host, port, path and query; no fragment). The client binds each response to the request that it sent and to this cache key (H-58).

**Final request (H-62).** The client uses a full document, a `304` or a delta only when the final request carried RQP fields: the origin of the final URL is in the allowlist (section 6.1, rule 1; RQP-05), the build allows its scheme (rule 5; H-49), and the final URL has no user name and no password (rule 6; H-68). After a redirect to an origin outside the allowlist, the final request has no RQP fields (rule 4), so its response is a normal response. A `304` and a delta need the cache key of S (items 1 and 2), and S has an allowlisted origin, so in practice this check matters for the full document (item 3). Without it, an open redirect on an allowlisted site would make the client store a snapshot for another origin.

**Redirects and the `public` partition (H-67).** <Badge type="info" text="DESIGN" /> A result that comes through a redirect never goes to the `public` partition: a full document, and the result of a delta on a `public` snapshot, when the response came after one or more redirects (any redirect that the network stack follows, for example a status `300`, `301`, `302`, `303`, `307` or `308`; an SDK that cannot see the chain counts the response as redirected, DL-042), also after a redirect to the same URL. The snapshot then goes to the partition of the request (item 2.6, item 3). Reason: a redirect response can have a `Set-Cookie` field, and the network stack adds that cookie to the next request; the client sees only the final response, so it cannot know that the final request had no credentials. The rule fails closed; the cost is that a page reached through a redirect gets no shared snapshot. Fixtures `FX-RQ-C24-PUBLIC-REDIRECT*` and `FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-REDIRECT*`.

The client handles the response in this order:

1. **`304`.** Keep S only when all of these are true (H-58):
   1. The request sent `If-None-Match: "R"`, the strong tag of the page revision R of S (section 6.2). A first-visit request and the fallback load (item 5) do not send it.
   2. The final URL has the cache key of S.
   3. The `304` has no `ETag` field, or its `ETag` matches `"R"` by weak comparison (`"R"` or `W/"R"`).
   4. The partition of the request is the partition of S (or S is in the `public` partition, rule 3 of section 6.1), and the partition of the request did not change between the request and the response (DL-036). For example, the user did not log out or change the account during the revalidation. This is check 3 of the [diff engine §3](/engineering/core/diff-engine#_3-rqp-patch-validation).

   Then S is the current page. Do not require `Rq-Version` on a `304`. When sub-item 4 fails, the client stops the session with `AUTH_PARTITION_CHANGED`, as the diff engine does for a delta: it does not show S, and the WebView loads the URL normally in the new partition ([cache identity §4](/engineering/cache/identity#_4-verification), "Logout during a revalidation"). When another sub-item fails, the `304` does not belong to S: show nothing of it, keep S unchanged, and let the WebView load the URL normally. This is not an error code: a `304` has no content that the client could reject.

   **A `304` on a `public` snapshot (H-65).** <Badge type="info" text="DESIGN" /> When the sub-items pass, S stays as it is, in the `public` partition, also when the `304` has a `Set-Cookie` field or `Cache-Control: private`, or the client does not know the credentials of the request. Reason: a `304` has no content. It only confirms that S, whose bytes met the conditions of the `public` partition when the client stored them (RQP-25), is the current page; so no content of this request can enter the `public` partition. The client does not move S and does not copy it to the partition of the request. A later `200` (F or D) for the same request follows the partition rules again. **Metadata (DL-040, I-3).** A `304` that does not meet the conditions of RQP-25 (for example one with a `Set-Cookie` field, one after a redirect, or one to a request with unknown credentials) does not update the stored metadata of S in `public` (its freshness: the time of the response, `Cache-Control`, `Expires`, `Age`): the next navigation revalidates S again. The reference client does not model freshness, so no fixture checks this sentence.
2. **Delta.** A `200` whose `Content-Type` media type is `application/vnd.rivqen.delta+json` (ASCII case-insensitive):
   1. If the request did not offer a delta (no S, no `block-patch`, no delta media type in `Accept`, or no nonce in `Rq-Have`), or the final URL does not have the cache key of S (H-58), stop with `PATCH_INVALID` and do the fallback load (item 5). A server that follows section 4 never sends an unsolicited delta (RQP-10). A release build sends RQP fields only to `https` (section 6.1), so a delta from a final URL that is not `https` never has the cache key of S.
   2. Validate D as [manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply) and the [diff engine §3](/engineering/core/diff-engine#_3-rqp-patch-validation) define. The checks include: size ≤ the client limit (`PATCH_TOO_LARGE`), the JSON rules and the schema, with no unknown member (`PATCH_INVALID`), and the operation rules: known block ids, one operation per block, document order (`PATCH_INVALID`).
   3. Compute the hash of each new content, then the page revision R′ with T, the policy line of the manifest of S and the new block hashes (algorithm in [manifest and patch §1.3](/engineering/protocol/manifest-patch#_1-3-hashes-and-revisions)), then the binding of the canonical final URL, the page revision R of S, the nonce N that this request sent and R′ ([manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields); H-107, H-108). It MUST be equal to `binding`. Else the result is discarded (`INTEGRITY_ERROR`). When the client cannot compute an input of the binding (a final URL that does not parse, no URL of S, no nonce), the result is also `INTEGRITY_ERROR`, never an exception (fail closed). This is the only integrity check of D (H-104, H-107, H-108): it also detects a delta for another template, another base (also an earlier base: a rollback, and the base of another user), for the page that S already is (a stale replay), for another request with the same base and URL (another nonce: a replay after A → B → A, or the delta of another user with the same `public` base), for another URL, content that the server did not hash, a prefix collision in `Rq-Have` and a change of `policy` or `limits` on the server (H-55).
   4. Build the patched document on a copy and run the markup analysis on it; check the patch content rule, with the attribute rule (H-48, H-78; [manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)). A failure is `PATCH_CONTENT_REJECTED` (or `INTEGRITY_ERROR` when only the page revision differs from R′). After all checks the page revision of the new snapshot is R′.
   5. On any failure, show nothing of D and do the fallback load (item 5).
   6. **Partition of the result (H-65).** When S is in the partition of the request, the patched snapshot is in the partition of the request. When S is in the `public` partition, the patched snapshot stays in `public` only when the request and the delta response meet the conditions of RQP-25 and the client knows that the request had no credentials: the delta response did not come through a redirect (H-67), the native SDK saw all header fields of the delta response (H-64), the response has no `Set-Cookie` or `Set-Cookie2` field and no `Cache-Control` directive `private`, the app declares the origin public, and the manifest policy of S has `"mode": "public"`. An explicit `Cache-Control: public` does not replace known absent credentials here: a delta has `Cache-Control: no-store` (section 4, step 7). Else the patched snapshot goes to the partition of the request, and S in `public` does not change. Reason: the server computes the changed blocks for this request; with credentials the new content can be personal, and the `public` partition is shared by all profiles ([manifest and patch §1.4](/engineering/protocol/manifest-patch#_1-4-policy)).
3. **Full document.** A `200` with an HTML media type is an RQP page only when it has the field `Rq-Version: 1` and the final request carried RQP fields: the origin of the final URL is in the allowlist (H-62), the final URL has no user information (H-68), and the final URL is an `https` URL (a debug build with the app flag also accepts `http`; H-49, H-58). Then the client checks that the browser decodes the document as UTF-8 (H-62; [manifest and patch §1.7](/engineering/protocol/manifest-patch#_1-7-delivery-in-the-document), client item 6): the body starts with a UTF-8 byte order mark, or the `Content-Type` has a UTF-8 charset by the charset rule of section 4. Then the client finds and checks the manifest as [manifest and patch §1.7 and §1.8](/engineering/protocol/manifest-patch#_1-8-client-checks-of-the-manifest) define: exactly one manifest element (H-51), at the insertion point that the client computes on the original document, with no gap (H-72, H-79a, client rule C2), schema, `origin` and `path` of the final URL, and the markup analysis of the original document: the page revision that the client computes from it with the policy line of the manifest is `page_revision` (H-48, H-103). The client compares the `type` value of the manifest script with `application/rivqen-manifest+json` by an ASCII case-insensitive byte match only, never by Unicode case folding (H-28). If a check fails (also the UTF-8 check, and when there is no manifest element), the client reports `MANIFEST_INVALID`, shows the document as a normal page and stores no snapshot. Without `Rq-Version: 1`, or from a final URL that the allowlist or the build does not allow, the client does not use a manifest, also when the body has one, and reports no error: only a server SDK sets the field (it removes an `Rq-Version` of the application, DL-030), and markup rule M-13 makes the server SDK reject a page that already has a manifest script. The revision of the new snapshot is the manifest page revision, not the `ETag`. The new snapshot has the cache key of the final URL; its partition follows [manifest and patch §1.4](/engineering/protocol/manifest-patch#_1-4-policy) (H-61 to H-64; after a redirect always the partition of the request, H-67).
4. **Other.** Any other response is a normal HTTP response. The session is not RQP for this navigation ([versioning](/engineering/protocol/versioning)).
5. **Fallback load.** One `GET` without `Rq-Have`, without an RQP `If-None-Match` and without the delta media type in `Accept`. By section 4, the response cannot be a delta. If it is a delta, stop with `PATCH_INVALID` and let the WebView load the URL normally. The client does at most one fallback load per navigation.

**Delivery to the WebView (H-66).** <Badge type="info" text="DESIGN" /> The client MUST give F and every snapshot (also a patched snapshot and a snapshot that it shows offline or from its cache) to the WebView with the media type `text/html` and the encoding `utf-8`, and MUST decode the bytes only as UTF-8. It never takes the encoding from the `Content-Type` of a response, from a `<meta>` element or from detection. A UTF-8 byte order mark at the start stays in the stored bytes. **Bytes, not a string (DL-040, L-2).** The platform adapter gives the bytes of the document to the WebView together with the encoding `utf-8` (Android `WebResourceResponse`, iOS `load(_:mimeType:characterEncodingName:baseURL:)`); then the HTML parser of the WebView removes the byte order mark (BOM sniffing). When the adapter must use an API that takes a string (Android `loadDataWithBaseURL`, iOS `loadHTMLString(_:baseURL:)`), it removes a leading `EF BB BF` from the bytes before it decodes them as UTF-8. Reason: a decoder of the platform can keep the byte order mark as the character U+FEFF; in the string, that character is text before the `<head>` start tag, so the HTML parser starts the body there, ignores the `<head>` start tag and puts the manifest and the head content into the body: the head of the WebView is not the head that RQP checked (markup §2.3, H-72). <Badge type="tip" text="FACT" /> The Java UTF-8 decoder keeps U+FEFF: `new String(bytes, StandardCharsets.UTF_8)` and an `InputStreamReader` give a string that starts with U+FEFF (checked with JDK 25.0.3 on 2026-10-18; [JDK-4508058](https://bugs.openjdk.org/browse/JDK-4508058), "UTF-8 encoding does not recognize initial BOM", closed as Won't Fix). In Swift, `String(decoding:as: UTF8.self)` keeps U+FEFF, and Foundation `String(data:encoding: .utf8)` and `String(bytes:encoding: .utf8)` remove it (checked with Swift 6.4 on macOS 26 on 2026-10-18). Whether a WebView removes a leading U+FEFF from a string that it gets: NOT FOUND. So the adapter removes the bytes itself and depends neither on the decoder nor on the WebView. Reason: the server and the client compute blocks, revisions and the manifest position on the UTF-8 bytes (markup M-02); the WebView must see the same characters, or its block boundaries and its manifest position are not the ones that RQP checked (security review round 3, L-2; code review round 3, finding 1). This rule is for the platform adapters: [Android](/engineering/platforms/android) and [iOS](/engineering/platforms/ios) give the encoding with the API that delivers the document. When the WebView loads F from the network itself, the `Content-Type` of F (section 4, step 3) gives UTF-8.

### 6.4 Versions, capabilities and downgrade

1. The server does not announce capabilities. The client learns what the server does from the response: a manifest with `Rq-Version: 1` means RQP; a delta means `block-patch`.
2. The client ignores unknown `Rq-*` response fields and `Rq-Version` values other than `1` (the response is then not RQP).
3. A server ignores unknown capability tokens and unknown `Rq-Have` members.
4. Moves to legacy mode or to normal HTTPS follow [versioning](/engineering/protocol/versioning) (DG-01 to DG-05).

## 7. Limits

| Item | Limit | On violation |
|---|---|---|
| `Rq-Have` field value | 4096 bytes (`maxRqHaveBytes`) | Server: `Rq-Have` is absent. Client: never sends more |
| `Rq-Have` member `n` | Exactly 16 bytes, one member (H-108) | Server: `n` is absent; the request gets F or a `304` |
| `Rq-Capabilities` field value | 1024 bytes (`maxRqCapabilitiesBytes`) | Server: field is absent |
| Blocks in `Rq-Have` member `b` | N (at most 256, `maxBlocks`) | Server: `b` is absent |
| Delta size | `maxPatchBytes` and smaller than F | Server: sends F. Client: `PATCH_TOO_LARGE`, fallback load |
| Fallback loads per navigation | 1 | Normal HTTPS load |

<Badge type="warning" text="RESEARCH" /> The 4096-byte budget comes from research R-6: about 8 KiB per field is the smallest common request field limit (nginx, Apache, Tomcat); [WP-17 S2](/engineering/plan/sprints/WP-17-S2) §1.

Both field limits are fixed by `rqp/1` (not configurable): a lower limit on one side would turn valid requests into full loads. Names: [resource limits](/engineering/core/resource-limits).

Server work for steps 4 to 7 is bounded: one parse of at most 4096 bytes and one of at most 1024 bytes, at most 256 comparisons of 8 bytes, and one envelope of at most `maxPatchBytes`. The block digests are the ones that the server computes for the manifest anyway. The patch content rule of step 7 (H-56, H-78) reads the start tags of the changed `html` blocks once; the markup pass of step 2 already tokenizes them, so the server can keep one result for each block and reuse it.

## 8. Rules

| ID | Rule |
|---|---|
| RQP-01 | A delta MUST use the media type `application/vnd.rivqen.delta+json`, MUST have `Cache-Control: no-store` and MUST NOT have an `ETag`. |
| RQP-02 | A server MUST NOT use status `226 IM Used` for RQP deltas. (RFC 3229 delta encoding is a different mechanism.) |
| RQP-03 | A `304` MUST NOT have content. It MUST have the `ETag` and the cache fields that F would have. |
| RQP-04 | A server that does not support RQP ignores the fields and sends a normal response. The client then treats the session as non-RQP. |
| RQP-05 | A client MUST NOT send `Rq-*` fields to origins outside its allowlist, or to a URL with a user name or a password (H-68). A client MUST compare the serialized origin of the URL (scheme, lower-case IDNA host, no default port) with each allowlist entry by exact match, with no wildcard and no suffix match (DL-038). An SDK MUST reject a configuration with an allowlist entry that is not equal to its own serialized origin (WHATWG URL parse, then serialization of the origin), whose host contains `*` (H-70), or whose scheme is not `https` or `http` (DL-041), with a clear error (H-69). A client MUST take the origin from the URL object that its network stack used for the request, never from a string that another parser reads again (DL-040). |
| RQP-06 | A server MUST ignore unknown capability tokens and unknown `Rq-Have` members. A server MUST NOT send a delta to a request without `block-patch`. |
| RQP-07 | A revision is `t1.` or `r1.` followed by 43 base64url characters. Revisions are compared byte for byte. Clients compute the template revision, the block hashes and the page revision with the markup analysis and use them only after the page revision check of a manifest or the `binding` check of a delta (H-48, H-103, H-104, H-107). |
| RQP-08 | F MUST have the `ETag` `"r1.…"` (strong) or `W/"r1.…"` (weak, RQP-18), MUST NOT depend on RQP request fields (`Rq-*`, the delta media type in `Accept`, `If-None-Match`), and MUST NOT list an `Rq-*` field in `Vary`. |
| RQP-09 | For a page that is invalid for RQP or has no insertion point for the manifest (no head start tag of the document and no implied head; H-72, H-79a), and for a response out of scope, the server MUST send the status, the content and the header fields of the application response unchanged, except that it MUST remove every `Rq-*` field (DL-030): no manifest, no `Rq-*` field, no ETag that RQP computed. |
| RQP-10 | A server MUST NOT send a delta without a single `r1.` entity-tag in `If-None-Match` and a matching `Rq-Have` template. |
| RQP-11 | A delta MUST have at least one operation, MUST NOT be larger than `maxPatchBytes`, and MUST be smaller than F. Else the server sends F. |
| RQP-12 | A client MUST NOT send an `Rq-Have` value longer than 4096 bytes. A server MUST treat a longer value as absent, without parsing it. |
| RQP-13 | A client MUST check `binding` of a delta against the binding that it computes from the canonical final URL, the page revision of its snapshot, the nonce that its request sent (H-108) and the page revision R′ that it computes from the template revision and the policy line of its snapshot, the block hashes of its snapshot and the hashes of the new contents (H-44, H-104, H-107). A server MUST compute `binding` from the canonical URL of the request, the base revision of `If-None-Match`, the nonce `n` of the request and the current page revision. On failure the client discards the delta and does one fallback load without delta fields. After acceptance the page revision of the new snapshot is R′. |
| RQP-14 | A client MUST compare the manifest script type by ASCII case-insensitive byte match only (H-28), MUST use a manifest only from a response with `Rq-Version: 1`, MUST accept it only at the insertion point that it computes on the original document (after the head start tag of the document, or before the start tag that implies the head), with no gap (H-72, H-79a, client rule C2), and MUST check it with the markup analysis of the original document: the page revision computed from the analysis and the policy line of the manifest equals `page_revision` (H-48, H-103). |
| RQP-15 | A client MUST send `If-None-Match` and `Rq-Have` only from a snapshot with the same cache key, and MUST NOT copy them to a redirected request. |
| RQP-16 | Preconditions are evaluated against F. A server SDK MUST NOT let the application answer `304` by a validator that the SDK replaces. F, D and a `304` MUST NOT have the `Last-Modified` of the application (DL-032). |
| RQP-17 | Logs MUST NOT contain the `Rq-Have` member `b`, block content, or the `binding` value of a delta or its preimage (it has the URL with the query, H-107). Revisions, sizes and outcomes are allowed ([errors and logging](/engineering/architecture/errors-logging)). |
| RQP-18 | A server MUST send the weak `ETag` `W/"r1.…"` whenever a content coding is applied to F, and the strong `"r1.…"` only for the identity coding (H-52). A server MUST accept a weak tag as the delta base. |
| RQP-19 | A server MUST read `Rq-Capabilities` only as one Inner List of Tokens; any other shape is an absent field (H-53). |
| RQP-20 | A server MUST send the same F for every request to a valid page, also without RQP fields, with exactly one manifest element (H-51) at the insertion point: immediately after the head start tag of the document, or immediately before the start tag that implies the head (H-72, H-79a). A server MUST remove every `Rq-*` field of the application response in every outcome; F and D MUST have exactly one field line `Rq-Version: 1` (DL-030). |
| RQP-21 | A server MUST NOT send the response fields `Rq-Revision` and `Rq-Template-Revision`; a client MUST NOT send the tokens `manifest` and `json-blocks` (H-54). |
| RQP-22 | A release build of a client MUST NOT send `Rq-*` fields to an `http` origin. A debug build MAY, only with an explicit app flag (H-49). |
| RQP-23 | A server SHOULD send F instead of D when the current content of a changed `html` block fails the patch content rule of [manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules), item 1: an element of `PATCH_FORBIDDEN`, a foreign element outside `FOREIGN_ALLOWED`, or the attribute rule of §4.2 (H-56, H-78). A client MUST apply the patch content rule to every delta, also when the server checks it. Both use the same rule, with `javascript:`, `vbscript:` and `data:` (H-59), the character references of H-77 and the `data:` raster images of H-83. |
| RQP-24 | A client MUST use a full document, a `304` or a delta only when the final request carried RQP fields: the origin of the final URL is in the allowlist (RQP-05, exact origin match, DL-038), the final URL has no user information (H-68), and the build allows its scheme (H-62). A client MUST keep its snapshot on a `304` only when the request sent `If-None-Match: "R"` with the page revision R of the snapshot, the final URL has the cache key of the snapshot (also the same partition, DL-036), and the `ETag` of the `304`, when present, matches `"R"` by weak comparison; else a normal load. A client MUST bind a manifest and a delta to the cache key of the final URL (scheme, host, port, path, query, partition). A release build MUST NOT accept a manifest or a delta from a final URL that is not `https` (H-58). |
| RQP-25 | A client MUST store a snapshot in the shared `public` partition only when the manifest has `"mode": "public"`, the app declares the origin public, the native SDK saw all header fields of the response (H-64; else the client acts as if the response had a `Set-Cookie` field), the response has no `Cache-Control` directive `private`, the response has no `Set-Cookie` or `Set-Cookie2` field (also not with the directive `public`, H-63, DL-038), and the response has the directive `public` or the request had no credentials (H-61). A client MUST treat a request as credentialed unless it knows that no cookie (also no cookie that the WebView cookie store adds), no `Authorization` field, no HTTP authentication and no client certificate was used; a `Set-Cookie` field in the response also counts as credentials (H-62). The result of a delta on a `public` snapshot MUST stay in `public` only when these conditions hold and the client knows that the request had no credentials; else it goes to the partition of the request, and the `public` snapshot does not change (H-65). A client MUST NOT store a full document or the result of a delta in `public` when the response came through a redirect, also a redirect to the same URL (H-67). A `304` does not move a snapshot (section 6.3, item 1), and a `304` that does not meet these conditions does not update the stored metadata of a `public` entry (DL-040). |
| RQP-26 | A server MUST treat a page as invalid for RQP when the `Content-Type` of the application response has a charset that is not a UTF-8 label, or more than one field line or media type. For every other valid page, F MUST have exactly one field line `Content-Type: text/html; charset=utf-8`, whatever the text of the `Content-Type` of the application response was (H-66). A client MUST use a full document only when its body starts with a UTF-8 byte order mark or its `Content-Type` has a UTF-8 charset; else `MANIFEST_INVALID` (H-62; section 4, charset rule). A client MUST give F and every snapshot to the WebView as `text/html` with the encoding `utf-8` and MUST decode the bytes only as UTF-8 (H-66; section 6.3); with an API that takes a string, it MUST remove a leading UTF-8 byte order mark before it decodes the bytes (DL-040). |
| RQP-27 | A manifest MUST have exactly the members `protocol`, `origin`, `path`, `page_revision` and, optionally, `policy` and `limits`; a client MUST reject another member (`MANIFEST_INVALID`) (H-103). A server MUST NOT write a member of `policy` with its default value or `policy` or `limits` without members, and SHOULD NOT write a `max_patch_bytes` hint that is not smaller than its `maxPatchBytes`. A client MUST treat an absent member as its default and accepts a manifest with a default member. |
| RQP-28 | A delta envelope MUST have exactly the members `binding` and `operations`, and each operation exactly `block_id` and `content` (H-104, H-107). A client MUST reject another member (`PATCH_INVALID`), also a member of the envelope before H-104 (`type`, `protocol`, `base_revision`, `template_revision`, `op`, `format`, `sha256`) and `next_revision` of the envelope of H-104. |
| RQP-29 | A client MUST send in each delta request a new nonce of 16 bytes from a CSPRNG in the `Rq-Have` member `n` and MUST NOT use a nonce again. A server MUST NOT send a delta for a request without exactly one member `n` that is a Byte Sequence of 16 bytes (H-108). |

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

Headers that do not matter for RQP are not shown. The bodies of F and D follow [manifest and patch](/engineering/protocol/manifest-patch): F of page A is `kat/full.html` (462 bytes since H-103, before 799; another server can write the manifest with other white space), D of section 9.3 is `valid/patch/shop-price.json`. The manifest of these examples has no `policy` and no `limits`. With them, the page revisions are different (H-55): page A `r1.pONNMoyS…` and page B `r1.kX-KQFiX…` for the values of the second known-answer set ([manifest and patch §1.3](/engineering/protocol/manifest-patch#_1-3-hashes-and-revisions)); the `Rq-Have` value stays the same.

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

Body: F of page A (page A with the manifest immediately after `<head>`). Section 4, step 8.

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
Rq-Have: t=t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM, b=:0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:, n=:AAECAwQFBgcICQoLDA0ODw==:
```

```http
HTTP/2 304
ETag: "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8"
Cache-Control: no-cache
```

No body. Section 4, steps 4 and 9. The nonce `n` (here the known-answer value; a real client sends new random bytes) does not change the `304`. If the client had received the weak tag of section 9.1, it still sends the strong form from its manifest; the weak comparison matches. With `Accept-Encoding: br` the `304` carries `ETag: W/"r1.RdZd…"`, the tag that F with that coding has.

### 9.3 Delta

The same request as in 9.2. The current page is B. Only the `price` prefix differs.

```http
HTTP/2 200
Content-Type: application/vnd.rivqen.delta+json
Rq-Version: 1
Cache-Control: no-store
```

Body: D with `binding` `d1.orrx-_-I…` and one operation, for block `price` (content `130.00`): the file `valid/patch/shop-price.json` (H-104, H-107, H-108). The binding has U = `https://shop.example.com/catalog`, B = R(A), N = `AAECAwQFBgcICQoLDA0ODw` (the nonce of the request of 9.2) and R′ = R(B) (`kat.json` member `bindings`). The client computes the hash of `130.00`, the page revision R′ of the result and the binding with its own R and the nonce that it sent. Section 4, step 7. Without the member `b` in `Rq-Have`, D has three operations (all blocks, H-7), if it is still smaller than F.

### 9.4 Invalid page

The application sends a page with the block id `price` two times (`RQP_MARKUP_DUPLICATE`, markup rule M-21). The request is as in 9.1.

```http
HTTP/2 200
Content-Type: text/html; charset=utf-8
Cache-Control: no-cache
```

Body: the page bytes of the application, unchanged. No manifest, no `Rq-Version`, no RQP `ETag`. When the application itself sets an `Rq-*` field (for example `Rq-Version: 1`), the server removes it (DL-030). Section 4, step 2. The client shows the page as normal HTML.

## 10. Security and privacy

1. **Client input to the server.** `Rq-Have` and `If-None-Match` only select what the server sends to the same client. A false value gives that client a wrong delta, which its checks reject, or a full document. Deltas are `no-store`, so a false value cannot reach another client through a cache. A cache that ignores `no-store` can send a delta to another request; the `binding` makes the client accept it only as the answer to the request that sent its nonce, with the same base revision and the same final URL (H-107, H-108; [manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields), "Replay", also for the assumptions: an honest server, TLS and a CSPRNG nonce).
2. **Digest prefixes.** 8 bytes of SHA-256 per block. An accidental collision is very improbable. A forced collision (a block content with the same 64-bit prefix) makes the server omit a changed block; the R′ and `binding` check uses the full digests (H-8, H-107), so the client loads F. The result is one extra request, never wrong content (H-44).
3. **Bounded work.** Section 7. The server never stores request state.
4. **Privacy.** `Rq-Have` holds hashes of page content. Clients send it only to the origin of the snapshot and only from the same partition (RQP-15); Rivqen logs never contain it (RQP-17). A content-derived ETag is the same for all users that see the same page. For private pages, a keyed revision is decided in WP-18 (H-8).

   <Badge type="info" text="DESIGN" /> **Digests in intermediary logs (H-60).** Until the keyed revisions of WP-18, the values of `Rq-Have` (template revision and 64-bit block digest prefixes) and of `If-None-Match` (the page revision) are unkeyed SHA-256 values of page content. A CDN, a reverse proxy or a load balancer that logs request header fields keeps them. A party that reads such logs and can guess the content of a block (for example a short value from a small set, such as an account balance or a yes/no state) can test its guess against the digest. Deployments SHOULD exclude `Rq-Have` and `If-None-Match` from the header logs of intermediaries until WP-18. The member `b` stays for private pages (H-60): without it, every delta has all blocks.
5. **Injected manifest.** A manifest is used only with `Rq-Version: 1` (RQP-14). The server removes every `Rq-*` field of the application response (DL-030), so an `Rq-Version` that the application or an upstream layer sets never reaches the client on a page that the server passes through. A page with a manifest-like script is invalid for RQP (markup rule M-13), so it is sent without `Rq-Version`. The client accepts exactly one manifest, only at the insertion point of the server that it computes on the original document (H-72, H-79a), and only when the page revision that it computes from the markup analysis of the original document is the page revision of the manifest (H-48, H-49, H-51, H-103).
6. **Patch content.** The client runs the markup analysis on the patched document and the patch content rule on patched `html` content ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules), H-48, H-78). So a delta brings no `script`, `style`, `link`, `meta`, `iframe`, `object`, `embed`, `base`, `basefont` or `bgsound` element, no SVG or MathML element outside `FOREIGN_ALLOWED`, no inline event handler and no attribute value that the URL parser reads as a `javascript:`, `vbscript:` or `data:` URL into the page, except `data:` raster images in `img`, `source` and `video` (H-59, DL-036, H-77, H-83). Since H-78 the markup analysis accepts these elements in a block, so the patch content rule is the only gate for them. A rejection is `PATCH_CONTENT_REJECTED`, a `SecurityViolation` with a metric (H-50). The rule is not an HTML sanitizer. It does not cover later URLs in `ping`, `url()` in a `style` value, app-specific schemes such as `intent:` or `myapp://`, DOM clobbering through `id` or `name`, overlays through `style`, full documents, and script gadgets of a framework on the page (the list: [manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)). The Content Security Policy of the page ([SEC-04](/engineering/security/controls)) and a native bridge that URL-scheme navigation cannot reach ([SEC-02](/engineering/security/controls)) control them.
7. **Cleartext.** A release build uses RQP only over `https`, also for the final URL after redirects (H-49, H-58, RQP-22, RQP-24).
8. **Response binding (H-58, H-62, DL-038, H-68, DL-040).** A `304`, a manifest and a delta are used only for the snapshot of the cache key of the final URL, only when the final request carried RQP fields (an allowlisted origin by exact match of the serialized origin, so a look-alike host such as `shop.example.com.attacker.net` is not allowed; no user information in the URL; the origin of the URL object that the network stack used, so a second parser cannot see another host), and a `304` only for a request that sent the revision of that snapshot, in the same partition (section 6.3). A response for another URL, another query, another partition or another request never changes the snapshot. A delta also carries this binding in its body (H-107, H-108): its `binding` has the canonical URL of the request that the server answered, the base revision that the request named and the nonce of the request, so a delta that a cache gives to another request (another URL, another base, or the same URL and base) fails the client check even when the transport binding passes.
9. **Encoding (H-62, H-66).** RQP is for UTF-8 documents only (section 4, charset rule; RQP-26). Server and client read the bytes as UTF-8; a browser that decodes them with another encoding would parse other blocks and another manifest position. So F has one exact `Content-Type` that every WebView reads as UTF-8, and the client gives F and every snapshot to the WebView as UTF-8 (section 6.3, "Delivery to the WebView"). The manifest position does not depend on the scripting mode: the manifest is the first child of the head in the scripting-enabled and the scripting-disabled view (H-72; [manifest and patch §1.7](/engineering/protocol/manifest-patch#_1-7-delivery-in-the-document), "Document head"). The blocks in the scripting-disabled view follow the markup rules ([markup](/engineering/protocol/markup) M-11).
10. **Credentials (H-62, H-64, H-65, H-67).** The client treats a request as credentialed unless it knows that none were used: a WebView adds cookies from its cookie store, and HTTP authentication and client certificates are not visible to page code. Unknown credentials and a `Set-Cookie` or `Set-Cookie2` in the response keep the snapshot in the partition of the request (RQP-25). The client can see `Set-Cookie` only in a response that the native SDK fetched itself: the Fetch Standard hides `Set-Cookie` from page script, and a response that the WebView loads is not visible to the SDK in full. So such a response counts as a response with `Set-Cookie` (H-64). A delta on a `public` snapshot keeps the result in `public` only when the client knows that the request had no credentials (H-65); else personal content from a request with cookies could reach the shared partition. A redirect can set a cookie that the next request carries, and the client sees only the final response, so a result that comes through a redirect never goes to `public` (H-67).

## 11. Intermediaries

<Badge type="warning" text="RESEARCH" /> The intermediary test runs in WP-13 (with WP-17). It checks for each CDN and proxy in the support matrix: `Rq-*` request fields arrive unchanged (also the member `n` of `Rq-Have`: an intermediary that changes or removes it costs a delta, never a wrong page, H-108); `Rq-Version` in responses arrives unchanged; a delta with `no-store` is not stored; `ETag` and `If-None-Match` pass (strong or weakened); a component that compresses F weakens a strong `ETag` and does not remove it (H-52, section 5.1); a cached F is served to a delta-eligible request without harm. Research R-6 found that Cloudflare ignores `Vary` by default; RQP does not depend on `Vary` (section 5). An intermediary that removes `Rq-Version` turns RQP off for its clients (section 6.3, item 3): the pages still work as normal HTML. The same is true for an intermediary that changes the charset of F to a label that is not UTF-8 (`MANIFEST_INVALID`, H-62).

## Related

- [RQP markup](/engineering/protocol/markup)
- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Versioning and downgrade](/engineering/protocol/versioning)
- [Diff engine](/engineering/core/diff-engine)
- [Resource limits](/engineering/core/resource-limits)
- [Golden fixtures](/engineering/protocol/fixtures)
