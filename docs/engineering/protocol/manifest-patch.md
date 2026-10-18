# Rivqen protocol: manifest and patch

This page defines the two RQP documents of `rqp/1`: the **page manifest** (what a page version contains) and the **patch envelope** (how a client goes from one page version to the next). It also defines the block hashes and the revisions.

**Status:** <Badge type="info" text="DESIGN" /> Normative for `rqp/1` from WP-17 S2. Human decisions ([WP-17 S1](/engineering/plan/sprints/WP-17-S1) §1, [WP-17 S2](/engineering/plan/sprints/WP-17-S2) §1): H-1 (inline manifest only), H-2 (full SHA-256, base64url), H-8 (revisions `t1.` and `r1.`), H-9 (no `page_id` and no `sequence` over HTTP), H-10 (hash in each operation), H-11 (JSON envelope, content as a string, no JCS), H-28 (ASCII case-insensitive match of the manifest type), H-44 (client block digests; the client verifies `next_revision`), H-45 (caching of full and delta responses), H-46 (`If-None-Match` carries the client revision), H-47 (only `replace_block`), H-48 (client markup analysis of the original and the patched document; attribute rule), H-49 (`http` only in debug builds; manifest position), H-50 (error codes, `maxManifestBytes`), H-51 (the same full document for every request), H-55 (`policy` and `limits` in the page revision), H-56 (the server sends the full document when a changed block fails the attribute rule), H-58 (binding to the final URL), H-59 (`vbscript:` and `data:` in the attribute rule), H-60 (privacy of the digests), H-61 (the `public` partition), H-62 (allowlisted final request, unknown credentials, UTF-8 only), H-63 (`Set-Cookie` gives the request partition). Architect decisions: DL-030 (`Rq-*` fields of the application response), DL-031 (the head end tag of the document), DL-034 (the precise head rule), DL-036 (the exact claim of the attribute rule). JSON Schemas: `spec/rqp/schemas/`. Headers and the choice between a full document, a delta and `304`: [negotiation](/engineering/protocol/negotiation). Block markup: [markup](/engineering/protocol/markup).

[[toc]]

## 0. Conventions

<Badge type="info" text="DESIGN" /> The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119.

| Notation | Meaning |
|---|---|
| `B64U(x)` | base64url without padding (RFC 4648 §5) of the bytes `x` |
| `SHA-256(x)` | SHA-256 (FIPS 180-4) of the bytes `x`; 32 bytes |
| `a ‖ b` | Concatenation of bytes |
| `"…"` | The UTF-8 bytes of the text; `\n` is the byte `0A`, `\t` is the byte `09` |
| Snapshot | The page version that a client holds: the template, the block contents, the manifest |

## 1. Page manifest

### 1.1 Example

The manifest of the example page of [markup §1](/engineering/protocol/markup#_1-example). All values are real; they are the first known-answer set in `spec/rqp/examples/kat/` (section 7). This manifest has no `policy` and no `limits`. Section 1.3 has the same page with both members (second known-answer set): they change the page revision.

```json
{
  "protocol": "rqp/1",
  "origin": "https://shop.example.com",
  "path": "/catalog",
  "template_revision": "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM",
  "page_revision": "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8",
  "blocks": [
    { "id": "title", "format": "html",
      "sha256": "0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces" },
    { "id": "price", "format": "html",
      "sha256": "Xg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs" },
    { "id": "cart", "format": "json",
      "sha256": "zyEL3KkZlLmAgTTFnonr3pdX-DnDM81i9xFoREUEaX4" }
  ]
}
```

### 1.2 Fields

<Badge type="info" text="DESIGN" /> The manifest is a JSON object with exactly these members. A member that is not in this table makes the manifest invalid; a new member needs a new protocol version.

| Member | Required | Rule |
|---|---|---|
| `protocol` | yes | The string `"rqp/1"` |
| `origin` | yes | Section 1.6 |
| `path` | yes | Section 1.6 |
| `template_revision` | yes | Section 1.3 |
| `page_revision` | yes | Section 1.3 |
| `blocks` | yes | Array of 0 to 256 block objects in document order (markup M-27). Also present for a page without blocks (`[]`). |
| `policy` | no | Section 1.4 |
| `limits` | no | Section 1.5 |

Block object, exactly these members:

| Member | Rule |
|---|---|
| `id` | The block id: `^[a-z0-9][a-z0-9_-]{0,63}$` ([markup](/engineering/protocol/markup) M-15). The ids in one manifest are unique (M-21). |
| `format` | `"html"` or `"json"` (M-16) |
| `sha256` | `B64U(SHA-256(content))` over the exact content bytes of the block (section 1.3, H-2). 43 characters. |

The manifest MUST NOT contain cookies, tokens, personal data, block content or executable code. It contains no query string (section 1.6).

### 1.3 Hashes and revisions

<Badge type="info" text="DESIGN" /> This section is the normative text of the hash and revision algorithm (H-2, H-8, H-11, H-55). The parser contract `tools/rqp-markup/CONTRACT.md` §4 implements it. A server computes the values on the **original document**: the bytes of the page before the server inserts the manifest (section 1.7). Markup rule M-13 makes sure that the original document has no manifest.

Let the original document have the blocks `b1 … bn` in document order (markup M-27). Each block has `id`, `format` and the byte range `[start, end)` of its content.

1. **Block hash.** `content(b)` = the bytes `[start, end)` of the original document. `sha256(b) = B64U(SHA-256(content(b)))`. This is the same for `html` and `json` blocks. The hash is over the raw bytes; there is no normalization and no JSON canonicalization (H-11; section 3).
2. **Template.** `template` = the original document with the bytes `[start, end)` of every block removed. The start tag and the end tag of each block stay. A UTF-8 byte order mark at offset 0 stays.
3. **Template revision.** `template_revision = "t1." ‖ B64U(SHA-256("rqp-t1\n" ‖ template))`.
4. **Page revision preimage.** `P = "rqp-r1\n" ‖ template_revision ‖ "\n" ‖ PL`, then for each block in document order: `id ‖ "\t" ‖ format ‖ "\t" ‖ sha256 ‖ "\n"`. PL is the **policy line**: `"PL\t" ‖ S ‖ "\n"` when the manifest that carries this page revision has the member `policy`, the member `limits` or both, with the text S of the table below; no bytes when the manifest has neither member (H-55). For a page without blocks, P ends after PL.
5. **Page revision.** `page_revision = "r1." ‖ B64U(SHA-256(P))`.

Each revision is 46 ASCII characters: the prefix `t1.` or `r1.` and 43 base64url characters.

**Text S of the policy line.** S is the items of this table that apply, in this order, separated by `;` (`3B`). There is no `;` before the first item or after the last item.

| Order | Item | Written when |
|---|---|---|
| 1 | `policy` | The manifest has the member `policy` (also `{}`) |
| 2 | `mode=` and the value (`private` or `public`) | `policy` has `mode` |
| 3 | `ttl_seconds=` and D(value) | `policy` has `ttl_seconds` |
| 4 | `offline=` and `true` or `false` | `policy` has `offline` |
| 5 | `limits` | The manifest has the member `limits` (also `{}`) |
| 6 | `max_patch_bytes=` and D(value) | `limits` has `max_patch_bytes` |

D(n) is the decimal digits of the integer n: no sign, no leading zero, `0` for zero. These are the digits of the JSON number in M (section 3.2, rule 4). An implementation computes S from the parsed member values after the schema check, not from the JSON text. So member order and white space in M do not change S.

<Badge type="info" text="DESIGN" /> Rules of the policy line:

1. **Members, not effective values.** S has the members that the manifest has. A missing member writes no item; its default (section 1.4) is not written. So no `policy` member, `"policy": {}` and `"policy": {"mode": "private"}` give three different page revisions, although the client applies the same policy for all three. Reason: the page revision then fixes every member of the manifest except `protocol`, `origin` and `path`, and these come from the protocol version and the target URI (section 1.7, server item 6). A strong ETag needs this.
2. **Absent line.** A manifest without `policy` and without `limits` has no policy line. P is then the same as before H-55, so no earlier value changes: the markup fixtures, the parser candidates and the first known-answer set keep their page revisions.
3. **Position.** PL is the third line, before the block lines, at the same place for every number of blocks. A verifier (section 2.4, step 6) writes the two header lines, the PL of its snapshot and then the block lines in one pass.
4. **No ambiguity.** A block id starts with `a`–`z` or `0`–`9` (M-15), so a line that starts with `PL` is never a block line. The item names are different, and the values come from closed sets (two `mode` values, digits, `true`, `false`), so they contain no `;`, `=`, tab or line feed. One preimage therefore has exactly one parse into template revision, policy line and block lines.
5. **Markup analysis.** The markup analysis ([markup §2](/engineering/protocol/markup); parser contract `tools/rqp-markup/CONTRACT.md` §4) has no manifest input. The `page_revision` that it gives is the value for a manifest without `policy` and `limits`. An implementation that has a manifest with one of these members computes steps 4 and 5 again with the PL of that manifest.

The preimage P of the example page (section 1.1, no policy line):

```text
rqp-r1
t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM
title	html	0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces
price	html	Xg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs
cart	json	zyEL3KkZlLmAgTTFnonr3pdX-DnDM81i9xFoREUEaX4
```

Each line ends with `0A`, also the last one. The separators in the block lines are `09` (tab). `SHA-256(P)` gives `r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8`. <Badge type="tip" text="FACT" /> The test `tools/rqp-schema/test/kat.test.mjs` computes these values from `spec/rqp/examples/kat/page.html` and compares them with the Node.js markup candidate.

The same page with `"policy": {"mode": "private", "ttl_seconds": 60, "offline": false}` and `"limits": {"max_patch_bytes": 262144}` (second known-answer set, `spec/rqp/examples/valid/manifest/shop-policy.json`) has this preimage:

```text
rqp-r1
t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM
PL	policy;mode=private;ttl_seconds=60;offline=false;limits;max_patch_bytes=262144
title	html	0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces
price	html	Xg3oaiol3ywUdxTQZNp7JJJu7_R8kUL-HK4accfrDKs
cart	json	zyEL3KkZlLmAgTTFnonr3pdX-DnDM81i9xFoREUEaX4
```

The separator after `PL` is `09` (tab). `SHA-256(P)` gives `r1.ke0dIjtkqBtefkyyLQU5ICLWPia5PfammTko2xmMLOQ`. With the price `130.00` (block hash `bB9ScYcQQzKWqPCBW62MTsc9Ff_djve7Denf3YMoCUg`) the page revision is `r1.0MisoW_Jyga-Mg3DpBueKx95g3OvhROWUgWwXI6t5uc` (patch `spec/rqp/examples/valid/patch/shop-policy-price.json`). The template revision and the block hashes are the same as in the first set. <Badge type="tip" text="FACT" /> The test `tools/rqp-schema/test/kat.test.mjs` computes both page revisions from a preimage that the test writes out itself and with the helper of the test package; `openssl dgst -sha256` gives the same values (checked 2026-10-12). The test `tools/rqp-schema/test/revision.test.mjs` checks the policy line rules on 112 combinations of present and missing members (with boundary values): each combination is schema-valid and has its own preimage, and only the combination without both members has no policy line.

**Rules for all implementations:**

1. Hashes and revisions are compared as ASCII strings, byte for byte. An implementation never decodes them for a comparison.
2. The encoding is canonical: 43 characters from the base64url alphabet, no padding, and the last character is one of `A E I M Q U Y c g k o s w 0 4 8` (its two low bits are zero). The schemas reject every other form. (A lenient decoder accepts other last characters for the same 32 bytes; the string comparison of item 1 would then fail.)
3. **Clients compute revisions only to check them.** A client stores the revisions of the manifest, sends them back ([negotiation](/engineering/protocol/negotiation)) and compares them. It never uses a revision that it computed in place of a revision that the server sent. A client computes revisions in three checks, all with this algorithm:
   - the markup analysis of the original document of a full response gives the block hashes and both revisions, and the client compares them with the manifest (section 1.8, H-48);
   - the verification of `next_revision` of a patch (section 2.4, steps 5 to 7; H-44) computes steps 4 and 5 from the stored `template_revision` string, the policy line of the snapshot manifest and the full block hashes of the snapshot and of the operations. It needs no template bytes;
   - the markup analysis of the patched document gives the template revision and the block hashes again, and the client computes the page revision from them with the policy line of the snapshot manifest (section 2.4, step 8; H-48, H-55).
4. A keyed variant of the revisions for private pages (HMAC) is decided in WP-18. It will have its own prefix and its own client rule. In `rqp/1` the only prefixes are `t1.` and `r1.`.
5. **Privacy (H-60).** The revisions and the block hashes are unkeyed. The client sends them in `If-None-Match` and in `Rq-Have` (64-bit prefixes), so the header logs of a CDN or a proxy can hold them. A party that reads such logs and can guess a block content can test the guess. Deployments SHOULD exclude `Rq-Have` and `If-None-Match` from intermediary header logs until the keyed revisions of WP-18 ([negotiation §10](/engineering/protocol/negotiation#_10-security-and-privacy), item 4).

### 1.4 Policy

<Badge type="info" text="DESIGN" /> `policy` is an object with these optional members. A missing member has its default. `"policy": {}` and no `policy` member give the same client behavior. They give different page revisions (section 1.3, policy line rule 1).

| Member | Type and range | Default |
|---|---|---|
| `mode` | `"private"` or `"public"` | `"private"` |
| `ttl_seconds` | Integer, 0 to 31 536 000 (one year) | `0` |
| `offline` | Boolean | `false` |

`policy` and `limits` are part of the page revision (section 1.3, H-55). A change of either member changes the page revision and the ETag of the full document, also when the original document stays the same. The client gets the new values with the next full document (section 2.4, "Policy and limits").

The manifest policy never allows what the HTTP headers ([freshness](/engineering/cache/policy)) or the app policy deny. `Cache-Control: no-store` always wins. Member by member (H-61):

- `ttl_seconds` and `offline: false` can only make the client stricter: a shorter freshness lifetime, no stale snapshot on a network error.
- `offline: true` and `"mode": "public"` allow more than the default, but only together with the HTTP headers and the app: `offline: true` has an effect only when the HTTP headers allow a stale response on an error (`stale-if-error`); `"mode": "public"` has an effect only under the conditions of the table below. Without them the client applies the default.

| Member | Effect |
|---|---|
| `mode` | `"private"`: the snapshot belongs to the partition of the request ([cache identity](/engineering/cache/identity)). `"public"`: the client MAY store the snapshot in the shared `public` partition, but only when all of these are true (H-61, RQP-25): the app declares the origin public; the response has no `Cache-Control` directive `private` (also not with field names); the response has no `Set-Cookie` field (H-63); the response has the `Cache-Control` directive `public`, **or** the request had no credentials (see "Credentials" below; H-62). Else the snapshot belongs to the partition of the request. |
| `ttl_seconds` | Upper bound of the freshness lifetime of the snapshot: the client uses the smaller of the RFC 9111 freshness lifetime and `ttl_seconds`. With `0`, the client revalidates before it shows the snapshot. |
| `offline` | `true`: the client MAY show the stale snapshot on a network error when the HTTP headers allow it (`stale-if-error`, [freshness](/engineering/cache/policy) rule 6). `false`: the client never shows the snapshot on a network error. |

<Badge type="tip" text="FACT" /> RFC 9111 §3.5: "A shared cache MUST NOT use a cached response to a request with an Authorization header field to satisfy any subsequent request unless the response contains a Cache-Control field with a response directive that allows it to be stored by a shared cache". §5.2.2.9: `public` "permits a shared cache to reuse a response to a request containing an Authorization header field". <Badge type="info" text="DESIGN" /> The `public` partition is shared by all profiles of the app, so the client applies this rule to it, and treats `Cookie` like `Authorization` (H-61). The manifest alone never moves a snapshot out of the partition of the request: when a server sets `"mode": "public"` on a page with personal content, but the request had credentials and the response has no `Cache-Control: public`, the content stays in the partition of the user.

**Credentials (H-62, RQP-25).** <Badge type="info" text="DESIGN" /> The client MUST treat a request as credentialed unless it knows that no cookie, no `Authorization` field, no HTTP authentication and no client certificate was used for it. "No cookie" includes the cookies that the WebView cookie store adds to the request itself. A `Set-Cookie` field in the response also counts as credentials. When the client cannot tell (for example, the WebView sends the request and the client cannot see its fields), the request had credentials. So "a request without credentials" in the table above is a positive fact that the client knows, never a default. The rule only changes the second condition of the `public` partition; an explicit `Cache-Control: public` does not depend on it (H-61), but it does not override the next rule. **A client MUST NOT store a snapshot in the `public` partition when the response has a `Set-Cookie` field, also when the response has an explicit `Cache-Control: public` (H-63).** Reason: a page that sets a session cookie can carry personal content, so the rule fails closed. It is stricter than RFC 9111 on purpose. The cost is a lost shared snapshot only.

### 1.5 Limits

<Badge type="info" text="DESIGN" /> `limits` is an object with one optional member:

| Member | Type and range | Rule |
|---|---|---|
| `max_patch_bytes` | Integer, 1 024 to 5 242 880 | Server hint: the largest patch envelope that the server sends for this page |

A missing member means "no hint". `"limits": {}` and no `limits` member give the same client behavior and different page revisions (section 1.3). The client limit for one envelope is `E = min(maxPatchBytes, max_patch_bytes)`, where `maxPatchBytes` is the client limit of [resource limits](/engineering/core/resource-limits) (256 KiB initial value). The size is the number of bytes of the envelope JSON text after content decoding (gzip, br, zstd). The client takes `max_patch_bytes` from the manifest of its current snapshot. The upper bound of the range is `maxDocumentBytes`: a patch larger than a full document has no use. A server SHOULD send the full document when the patch would be larger than its own hint. A client rejects a larger envelope (`PATCH_TOO_LARGE`) and loads the full document; this costs time, not correctness.

### 1.6 Origin and path

<Badge type="info" text="DESIGN" /> The manifest is bound to the URL of the response.

| Member | Rule |
|---|---|
| `origin` | The ASCII serialization of the origin of the response URL (WHATWG URL Standard): scheme `https` or `http`, `://`, the host in lower case (an internationalized domain in its `xn--` form, an IPv6 address in `[ ]`), and `:port` only when the port is not the default port. No path, no trailing `/`, no user information. At most 512 characters. |
| `path` | The path of the response URL as the WHATWG URL parser serializes it: it starts with `/`; non-ASCII bytes and the characters space, `"`, `#`, `<`, `>`, `?` are percent-encoded. No query, no fragment. At most 4 096 characters. |

1. The client compares `origin` and `path` byte for byte with its own serialization of the **response URL** (the final URL after redirects), never with values from the manifest alone. On a difference the manifest is invalid (section 1.8).
2. The query string is not part of the manifest, because a query can hold personal data. The client cache key keeps the query ([cache identity](/engineering/cache/identity)), so two URLs that differ only in the query have different snapshots with the same `path`.
3. The schemas accept `http` and `https` origins (H-49), so that a local development server can be tested. A client uses RQP for an `http` origin only in a **debug build** and only when the app sets an explicit flag for it. A **release build** uses RQP only for `https` origins: for an `http` URL it sends no `Rq-*` field and does a normal load ([negotiation §6.1](/engineering/protocol/negotiation#_6-1-where-the-client-sends-rqp-fields)). The rule applies to the final URL too: after a redirect to an `http` URL, a release build does not use the manifest (a normal page, no error) and does not accept a delta (H-58).
4. **Binding (H-58).** The client stores the new snapshot under the cache key of the final URL: the partition and the canonical URL with scheme, host, port, path and query ([cache identity](/engineering/cache/identity)). It applies a delta only to the snapshot with the cache key of the final URL of the delta ([negotiation §6.3](/engineering/protocol/negotiation#_6-3-response)). Rule 1 checks only origin and path, because the manifest has no query; the cache key adds the query and the partition.
5. The server takes `origin` from its configuration of the public origin of the site, not from the connection (a server behind a TLS proxy sees `http` and an internal host). `path` is the path of the target URI.

### 1.7 Delivery in the document

<Badge type="info" text="DESIGN" /> The server sends the manifest only inline, in the full document (H-1, [ADR-006](/engineering/architecture/adr/#adr-006)). There is no other delivery form.

**Server.**

1. The server inserts exactly these bytes, **exactly once** per document (H-51): `<script type="application/rivqen-manifest+json">`, then the manifest JSON text M, then `</script>`. The start tag has no other attribute and no white space other than the one space.
2. The insertion point is immediately before the `<` of the **head end tag** of the original document (H-49, DL-031; defined below). A document without a head end tag is sent without RQP: the original document, no manifest, no `Rq-*` field, no ETag that RQP computed (as an invalid page, [negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server), step 2). A `</head>` end tag is never in block content (markup M-23), so the insertion never changes block content.
3. M follows section 3.2 and MUST NOT contain the byte `<` (`3C`). The schema patterns allow `<` in no string, so a serializer that writes no `<` in its syntax meets this rule. Then the content of the element cannot end early or change the script data state.
4. All other bytes of the original document stay unchanged. The hashes and revisions are those of the original document (section 1.3).
5. **One full document for every request** (H-51). The server inserts the manifest into the response for every request to a valid page, also for a request without RQP fields or with an unsupported `Rq-Version`. The full document does not depend on any request field ([negotiation §5.1](/engineering/protocol/negotiation#_5-1-full-document)).
6. **Stable bytes** (H-51, H-55). The ETag of the full document is the page revision ([negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server)). The page revision covers the template, the block contents and the members `policy` and `limits` (section 1.3). The other members come from the protocol version (`protocol`) and the target URI (`origin`, `path`; section 1.6). So the manifest value is a function of the page revision and the target URI by construction, and a change of `policy` or `limits` in the server configuration changes the page revision. One rule stays for the server: the serializer of M MUST be deterministic (the same manifest value gives the same bytes of M). Then one page revision always identifies one full document for one URI, as a strong validator requires.

<Badge type="info" text="DESIGN" /> Example: `spec/rqp/examples/kat/full.html` is one valid full document of the example page. Another server can write M with other white space or member order (section 3.2); its full document has other bytes and the same page revision.

<Badge type="tip" text="FACT" /> A `script` element whose `type` is not a JavaScript MIME type, `module`, `importmap` or `speculationrules` is "a data block, which is not processed by the user agent" (WHATWG HTML §4.12.1; §4.12.1.1 step 13: "No script is executed"). The manifest element is such a data block, so it needs no CSP nonce.

**Client.**

1. The manifest element is the `script` start tag whose **first** `type` attribute has a raw value (markup §2.1, no character reference decoding) that is equal to `application/rivqen-manifest+json` by **ASCII case-insensitive byte comparison**: the value has exactly 32 bytes, and each byte is equal to the byte of the type after the bytes `41`–`5A` (A–Z) are mapped to `61`–`7A` (a–z) on both sides. No other mapping, no trimming of white space, no MIME parameters, no Unicode case folding (H-28). A value with a byte `80` or larger never matches.
2. A response with `Rq-Version: 1` has **exactly one** manifest element (H-51). No matching start tag, or more than one, means the document has no valid manifest (section 1.8).
3. The end tag of the manifest element is immediately followed by the head end tag of the document (server item 2, DL-031). Else the document has no valid manifest. This is also the case when the document has no head end tag.
4. M is the bytes between the `>` of the start tag and the `<` of the next `</script` end tag.
5. The client gets the original document when it removes the bytes of the element (from the `<` of the start tag to the `>` of the end tag). The client stores the snapshot without the manifest element.
6. **UTF-8 only (H-62, RQP-26).** The client checks this item first. It uses the document only when the browser decodes it as UTF-8: the body starts with the UTF-8 byte order mark `EF BB BF`, or the `Content-Type` of the response has a UTF-8 charset by the charset rule of [negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server) (one field line, one media type, a charset parameter that is a UTF-8 label). Else the document has no valid manifest (section 1.8): the client shows it as a normal page and reports `MANIFEST_INVALID`. A server that follows negotiation §4 sends a UTF-8 charset in every F (it appends `; charset=utf-8` when the application sends no charset, and passes a page with another charset through). <Badge type="tip" text="FACT" /> In the HTML encoding sniffing algorithm, BOM sniffing comes first and a transport-layer encoding comes before the `<meta>` prescan (WHATWG HTML, "the encoding sniffing algorithm", steps 1 and 4), so either signal makes the browser decode UTF-8.

The client finds the element with the tokenizer of [markup §2.3](/engineering/protocol/markup#_2-3-tokenization-and-token-stack), so a manifest-like text in a comment or in raw text is not an element.

**Head end tag (DL-031, DL-034).** <Badge type="info" text="DESIGN" /> The server (server item 2) and the client (client item 3) find the head end tag with the same walk: the tokenizer and the token stack of [markup §2.3](/engineering/protocol/markup#_2-3-tokenization-and-token-stack) (M-04, M-05) from the start of the document. A **gap** is the bytes between the end of one token and the start of the next token: character data, and also the bytes of `</>`, which gives no token. **White space** is the bytes `09`, `0A`, `0C`, `0D` and `20`; a UTF-8 byte order mark at offset 0 also counts as white space here.

An **ignored end tag** is an end tag whose name is not `head`, `body`, `html` or `br` (DL-034). In items 1 and 2 it changes nothing: it is not a failure, and the walk stays in the same item.

1. **Before the head.** Until the first `head` start tag, only these can come: gaps of white space, comments, DOCTYPE tokens, `html` start tags and ignored end tags. The first `head` start tag pushes the **document head**. Any other token (another start tag, or the end tag `</head>`, `</body>`, `</html>` or `</br>`) or a gap with another byte before it means: the document has no head end tag.
2. **In the head.** While the document head is the current node (the top entry of the token stack), only these can come: gaps of white space, comments, DOCTYPE tokens, the start tags `base`, `basefont`, `bgsound`, `link`, `meta`, `noframes`, `noscript`, `script`, `style`, `template` and `title`, ignored end tags (for example a stray `</meta>`, `</link>` or `</script>`), and the `</head>` end tag. Any other token (another start tag, or the end tag `</body>`, `</html>` or `</br>`) or gap means: the document has no head end tag. While another element is the current node (the content of a `title`, `script`, `style`, `noscript`, `noframes` or `template`), the walk checks nothing.
3. **The head end tag** is the `</head>` end tag of item 2. A document has at most one. When the document head leaves the token stack in another way (an end tag that pops it together with another element), the document has no head end tag.

| Document | Head end tag |
|---|---|
| A normal page: `<head>`, head content, `</head>` | That `</head>` |
| `<head>` with a template that holds `<head></head>`, then `</head>` | The last `</head>` (the inner pair is template content) |
| No `</head>` in the head; `<body>`; a stray `</head>` in the body | None: `<body>` came while the head was the current node |
| No `</head>` in the head; a template in the body holds `<head></head>` | None: as above, and that `head` start tag is not the first one |
| No `<head>` start tag; `<title>…</title></head>` | None: `<title>` came before a `head` start tag |
| `<head><svg>…</svg></head>` | None: `svg` is not head content |
| `<head><meta charset="utf-8"></meta><title>…</title></head>` (a stray `</meta>`) | The `</head>`: `</meta>` is an ignored end tag (DL-034) |
| `<head><title>…</title></body></head>` | None: `</body>` is not ignored |
| `<head><title>…</title></br></head>` | None: `</br>` is not ignored |

<Badge type="tip" text="FACT" /> For a UTF-8 document with scripting enabled, items 1 and 2 follow the "before head" and "in head" insertion modes of the HTML parser (WHATWG HTML §13.2.6.4.3, §13.2.6.4.4): there, a `head` start tag inserts the head element; in the head, the listed start tags are processed as head content, the `</head>` end tag pops the head element, and for "anything else" the parser pops the head element and reprocesses the token, so the head ends without a `</head>` end tag. In both modes the end tags `</body>`, `</html>` and `</br>` act as "anything else", and "any other end tag" is a parse error and is ignored (both modes checked on 2026-10-12); the same holds in a template (§13.2.6.4.16). <Badge type="info" text="DESIGN" /> The parity has two conditions. The document is decoded as UTF-8 (client item 6, H-62): with another encoding the browser can see other tokens. Scripting is enabled: with scripting disabled, a `noscript` start tag in the head switches the parser to the "in head noscript" mode, which this walk does not model; this is open item H-42 (WP-17 S3). Items 1 and 2 ignore the end tags that the parser ignores (DL-034; corpus of 2026-10-12: 10 of 791 valid pages had such stray end tags in the head). For `</body>`, `</html>` and `</br>` the parser closes or implies the head, so Rivqen finds no head end tag. Rivqen still fails closed for these tokens that the parser handles in another way: head content before the `head` start tag (the parser implies a head; DL-034 does not model it), a second `head` start tag, an `html` start tag in the head, and `</>`. The document then has no head end tag and is sent without RQP. A page whose minifier removed the `<head>` start tag also loses RQP (DL-031). A later version can relax the rule; clients of this version then show the newly allowed pages as normal pages (`MANIFEST_INVALID`), which is safe. Fixtures: `FX-RQ-P39-*`, `FX-RQ-C21-*` ([fixtures §4.1](/engineering/protocol/fixtures#_4-1-protocol-fixtures-fx-rq-p-fx-rq-c)).

### 1.8 Client checks of the manifest

<Badge type="info" text="DESIGN" /> The client accepts a manifest only when all checks pass, in this order:

1. M is at most 64 KiB (`maxManifestBytes`, [resource limits](/engineering/core/resource-limits); H-50). A manifest with 256 blocks and the longest ids has about 37 KiB.
2. M is a JSON text by section 3.2 and is valid against `manifest.schema.json`.
3. The block ids are unique.
4. `origin` and `path` match the response URL (section 1.6).
5. **Markup analysis of the original document (H-48, MUST).** The client runs the markup analysis of [markup §2](/engineering/protocol/markup) on the original document (section 1.7, client item 5). The document is valid for RQP, and the result has the same blocks as `blocks` (the same `id`, `format` and `sha256`, in the same order) and the same `template_revision`. The page revision that section 1.3 gives for this template revision, these blocks and the policy line of the manifest is equal to `page_revision` (H-55). For a manifest without `policy` and `limits` this is the `page_revision` of the analysis.

On a failure of these items or of the client items of section 1.7 (also item 6, UTF-8), the client shows the document as a normal page (it is a complete HTML document), does not store an RQP snapshot of it, and reports `MANIFEST_INVALID` (section 5).

Item 5 has two reasons. First, the `next_revision` check of section 2.4 starts from the block hashes of the snapshot; a server defect in the manifest would give a wrong base, and every later patch would fail or, worse, describe a page that the client does not have. Second, the client needs the block offsets of the snapshot to splice a later patch into it. The analysis is one pass over at most `maxDocumentBytes`. Item 5 also finds a manifest whose `policy` or `limits` changed without a new page revision. <Badge type="tip" text="FACT" /> The test `tools/rqp-schema/test/kat.test.mjs` shows a manifest that is consistent in itself (its page revision matches its block list) but not with the document, and manifests that add or remove `policy` or `limits` and keep the old page revision; only item 5 finds them.

## 2. Patch envelope

### 2.1 Example

A delta response (`Content-Type: application/vnd.rivqen.delta+json`, [negotiation](/engineering/protocol/negotiation)) from the page of section 1.1 to the same page with the price `130.00`:

```json
{
  "type": "rivqen.patch",
  "protocol": "rqp/1",
  "base_revision": "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8",
  "next_revision": "r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI",
  "template_revision": "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM",
  "operations": [
    { "op": "replace_block", "block_id": "price", "format": "html",
      "content": "130.00",
      "sha256": "bB9ScYcQQzKWqPCBW62MTsc9Ff_djve7Denf3YMoCUg" }
  ]
}
```

### 2.2 Fields

<Badge type="info" text="DESIGN" /> The envelope is a JSON object with exactly these members. There is no `page_id` and no `sequence`: these are fields of push channels only (H-9, WP-16).

| Member | Rule |
|---|---|
| `type` | The string `"rivqen.patch"` |
| `protocol` | The string `"rqp/1"`, the same representation as in the manifest |
| `base_revision` | The page revision that the patch applies to. The server takes it from the request: it is the client revision in `If-None-Match` (H-46, [negotiation](/engineering/protocol/negotiation)). |
| `next_revision` | The page revision of the server document after the patch (section 1.3) |
| `template_revision` | The template revision of the server document. The same as the client template revision; else the server sends the full document. |
| `operations` | Array of 1 to 256 operations (section 2.3) |

### 2.3 Operations

<Badge type="info" text="DESIGN" /> The template fixes the block list: the start and end tags of the blocks, with their `data-rq-block` ids, are template bytes (section 1.3, step 2). So two documents with the same `template_revision` have the same blocks (ids, formats, order). An operation changes only the content of a block.

`rqp/1` has **one operation**, `replace_block` (H-47):

| `op` | Members (exactly) | Effect |
|---|---|---|
| `replace_block` | `op`, `block_id`, `format`, `content`, `sha256` | The content of the block becomes the UTF-8 bytes of `content`. |

- There is no `insert_block`: a new block changes the template, so the server sends the full document.
- There is no `remove_block`: `replace_block` with `"content": ""` gives the same result for an `html` block (the block element stays, its hash becomes `47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU`, the SHA-256 of no bytes). Empty content of a `json` block is not a JSON text (M-25), so the client rejects it (section 2.4, step 8).
- A new operation in a later version needs a new capability token ([negotiation §3.3](/engineering/protocol/negotiation#_3-3-rq-capabilities)). A client of `rqp/1` rejects every other `op` value (schema, `PATCH_INVALID`).

Rules for the operation list:

1. `block_id` names a block of the client snapshot.
2. A block has at most one operation in a patch.
3. The operations are in document order of their blocks.
4. `format` is equal to the format of the block in the snapshot.
5. `content` is a JSON string, also for a `json` block (H-11). For a `json` block the string is the raw JSON text of the block: the exact characters between `<script type="application/json" data-rq-block="…">` and `</script>`, for example `"{\"items\":4}"`. It is never an embedded JSON value.
6. `sha256` is `B64U(SHA-256(UTF-8(content)))`: SHA-256 over the UTF-8 bytes of the **decoded** string value (H-10, H-11), not over the JSON text of the string with its quotes and escapes. The JSON strings `"\u003cb>"` and `"<b>"` decode to the same three characters, so they have the same hash.
7. The UTF-8 encoding of `content` is at most 1 MiB (`maxBlockBytes`, M-26). The schema limit `maxLength` counts code points, so the client also checks the bytes.

### 2.4 Client verification and apply

<Badge type="info" text="DESIGN" /> Let the client snapshot have the template revision `T`, the page revision `R` and the block list `L = [(id, format, h)]` in document order, where `h` is the **full** hash of the block content in the snapshot. The client verifies a patch in this order. Before step 1, the [diff engine §3](/engineering/core/diff-engine#_3-rqp-patch-validation) does the transport checks: a delta from a final URL without the cache key of the snapshot (H-58) or for a request that did not offer one is `PATCH_INVALID` ([negotiation §6.3](/engineering/protocol/negotiation#_6-3-response)), then the size, the JSON rules of section 3.2 and the schema.

1. `template_revision` is equal to `T`. Else `TEMPLATE_MISMATCH`.
2. `base_revision` is equal to `R`. Else `PATCH_BASE_MISMATCH`.
3. The operation list follows the rules 1 to 4 of section 2.3. Else `PATCH_INVALID`.
4. For each operation: the UTF-8 bytes of `content` are at most `maxBlockBytes` (else `PATCH_TOO_LARGE`), and `B64U(SHA-256(UTF-8(content)))` is equal to `sha256` (else `INTEGRITY_ERROR`).
5. Build the new list `L'`: a copy of `L` in which each block with an operation gets the `sha256` of its operation. Every other block keeps its full hash `h` from the snapshot.
6. Compute `R'` with section 1.3, steps 4 and 5, from the string `T`, the policy line of the snapshot manifest (H-55) and the list `L'`.
7. `R'` is equal to `next_revision`. Else `INTEGRITY_ERROR`.
8. **Markup analysis of the patched document (H-48, MUST).** Build the patched document on a copy: the original document of the snapshot with the content bytes of each patched block replaced by the UTF-8 bytes of `content`. If it is larger than `maxDocumentBytes`, stop with `PATCH_TOO_LARGE`. Run the markup analysis of [markup §2](/engineering/protocol/markup) on it. The result MUST be valid for RQP, MUST have the template revision `T` and the same blocks as `L` (`id`, `format`, order); else `PATCH_CONTENT_REJECTED`. The page revision of the result (section 1.3, with the policy line of the snapshot manifest) MUST be equal to `next_revision`; else `INTEGRITY_ERROR`.
9. **Attribute rule (H-48, MUST).** The content of each patched `html` block passes the attribute rule of section 4. Else `PATCH_CONTENT_REJECTED`.

On any failure the client does not change the page or the snapshot ([INV-13](/engineering/architecture/invariants)) and does one fallback load of the full document ([negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), item 5).

Steps 1 to 7 need only the hashes. Step 8 needs the bytes of the original document of the snapshot. After step 7 passes, the block hashes of the patched document are the hashes of `L'` (step 4 checked them), so a different page revision in step 8 means a changed template or block list; step 8 reports the first of these checks that fails.

**Why step 7 is required (H-44).** The client tells the server its blocks with 64-bit prefixes of the block hashes ([negotiation](/engineering/protocol/negotiation)), and the server sends only the blocks whose prefix differs. When a changed block has the same 64-bit prefix as the client block (by chance, or made on purpose by a party that controls block content), the server omits it. In step 5 the client then keeps its own full hash for that block, so `R'` is not equal to the server's `next_revision`, and the client loads the full document. A prefix collision therefore costs one full load; it never gives a wrong page. This holds only when the client uses its own full hashes in step 5, never the prefixes it sent. Step 7 also finds a server that omits a changed block for any other reason, and a client snapshot that differs from what the server assumed.

**Policy and limits (H-55).** A patch carries no `policy` and no `limits`. The client computes `R'` with the policy line of its snapshot. When the server changed `policy` or `limits` after the client stored the snapshot, `next_revision` has the new policy line, `R'` is different, and the client loads the full document (`INTEGRITY_ERROR`, one extra request). When only `policy` or `limits` changed and no block changed, the change set is empty and the server sends the full document at once ([negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server), step 6). So after a patch that passes step 7, the policy and limits of the snapshot are those of the server.

**Apply.** When all checks pass, the new snapshot is the patched document of step 8 (with the block offsets of its analysis), the block list `L'`, the page revision `next_revision`, the same template revision, and the same `policy` and `limits` (step 7 showed that they did not change). The content of a patched `json` block is the UTF-8 bytes of `content`. The page and the cache change together ([diff engine §4](/engineering/core/diff-engine#_4-apply-plan)).

### 2.5 Server rules

<Badge type="info" text="DESIGN" />

1. A server builds a patch only from a current document that is valid for RQP ([markup §2](/engineering/protocol/markup)) and whose `template_revision` is equal to the template revision of the client.
2. With the client block information ([negotiation](/engineering/protocol/negotiation), H-44) the server sends an operation for each block whose hash prefix differs. Without it, the server sends an operation for every block (H-7 fallback). Both give the same `next_revision`.
3. The server writes one `replace_block` operation per changed block, in document order (H-47). Empty new content is `"content": ""`.
4. `content` is the exact content bytes of the block in the current document, as a JSON string. The bytes are valid UTF-8 (M-02), so every content has a JSON string form.
5. When the envelope would be larger than its `max_patch_bytes` (section 1.5) or than `maxPatchBytes`, or would not be smaller than the full document, the server sends the full document ([negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server), step 7). The server SHOULD also send the full document when a changed `html` block fails the attribute rule (item 7).
6. Caching of the delta response (`Cache-Control: no-store`, no `ETag`; H-45) is defined in [negotiation](/engineering/protocol/negotiation).
7. **Attribute rule (H-56).** A full document can have event-handler attributes in a block: the attribute rule of section 4.2 is not a markup rule. The server SHOULD send the full document instead of a delta when the new content of an `html` block in the change set fails the attribute rule of section 4.2. Then the client gets the page in one request. With a delta, the client rejects it (section 2.4, step 9) and loads the full document (two requests). The client check stays (MUST): the client never depends on this server check. The result depends only on the content bytes of the block, so a server can compute it once for each block hash, in the markup pass of the current document. The other content checks (section 2.4, step 8) need no server rule: when the client snapshot is what the server assumes, the patched document is the current document, which is valid for RQP (item 1). Template authors avoid full loads when they keep `on*` attributes, `javascript:`, `vbscript:` and `data:` URLs and character references other than `&amp;` out of the attribute values in blocks that change ([template and data](/guide/concepts/template-and-data#_4-rules-for-good-blocks)).

## 3. Canonical bytes

### 3.1 Block content

<Badge type="info" text="DESIGN" /> The bytes that a block hash covers are the exact content bytes. There is no normalization for any format (H-11).

| Format | Hashed bytes |
|---|---|
| `html` | The content bytes as they are in the document (UTF-8). White space, line ends (LF and CRLF are different bytes), character references and attribute quotes are not changed. |
| `json` | The same: the raw bytes of the JSON text in the document. No JSON canonicalization (RFC 8785 JCS is not used), no re-serialization, no change of number formats or member order. |

In a patch, the hashed bytes are the UTF-8 encoding of the decoded `content` string (section 2.3, rule 6). After the apply step these are also the content bytes of the snapshot, so the next patch and the next request use the same hash.

### 3.2 JSON text of the manifest and the envelope

<Badge type="info" text="DESIGN" /> The manifest and the envelope are JSON texts (RFC 8259) with these extra rules. A client MUST reject a text that breaks one of them (`MANIFEST_INVALID` or `PATCH_INVALID`):

1. UTF-8, no byte order mark.
2. **No duplicate member names** in any object. <Badge type="tip" text="FACT" /> RFC 8259 §4: "The names within an object SHOULD be unique"; when they are not, "the behavior of software that receives such an object is unpredictable" (many parsers keep the last value, some the first, some fail). Two parsers could then see two different envelopes. A JSON Schema validator cannot find duplicates, because it validates the parsed value (test `tools/rqp-schema/test/examples.test.mjs`).
3. A `\u` escape of a high surrogate is followed by a `\u` escape of a low surrogate. A lone surrogate escape is invalid (as in M-25). So every string decodes to Unicode scalar values and has one UTF-8 form.
4. Integers (`ttl_seconds`, `max_patch_bytes`) are written as digits only: no sign, no fraction, no exponent (`60`, not `60.0` or `6e1`). The schema type `integer` accepts `60.0`; the parser rule is stricter.
5. The nesting depth is at most 3 for both documents; a parser can stop at `maxJsonDepth` ([resource limits](/engineering/core/resource-limits)).

White space between tokens is allowed. Member order is free.

## 4. Content rules

<Badge type="info" text="DESIGN" /> These rules apply to the content of patch operations (H-48). The client checks them in steps 8 and 9 of section 2.4. A failure is `PATCH_CONTENT_REJECTED`: the client shows nothing of the patch and loads the full document.

1. `html` content MUST NOT contain `<script>`, `<iframe>`, `<object>`, `<embed>`, inline event handlers (`on*` attributes), or an attribute value that starts with `javascript:`, `vbscript:` or `data:` after URL normalization (section 4.2; H-59, DL-036).
2. `json` content is data. The page renders it with its own code.
3. The client inserts `html` content only into SDK-owned regions. In React apps, prefer `json` blocks and state updates.

<Badge type="info" text="DESIGN" /> **What the rules give (H-59, DL-036).** Patched content brings no `script`, `iframe`, `object` or `embed` element, no inline event handler and no attribute value that starts with `javascript:`, `vbscript:` or `data:` after URL normalization (section 4.2, items 3 and 4) into the page. The rules are not an HTML sanitizer. They do **not** cover:

| Not covered | Example |
|---|---|
| A later URL in a value with more than one URL | `srcset="/a.png 1x, data:… 2x"`, `ping="/p javascript:x"`: item 4 reads only the start of the value |
| URLs in CSS | `style="background:url(javascript:x)"` |
| App-specific URL schemes | `href="intent:…"`, `href="myapp://pay"` |
| DOM clobbering | `id` or `name` values that shadow names that page code reads |
| Overlays through CSS | `style="position:fixed;inset:0"` over the real page |
| Full documents | A full document is the page of the server; the rules apply to patch content only |
| Script gadgets of a framework | Attribute values or text that page code reads as code or as a template |

The Content Security Policy of the page ([SEC-04](/engineering/security/controls)) and the settings of the framework control script gadgets, CSS and the use of `id` and `name` by page code. The native bridge MUST NOT be reachable through URL-scheme navigation ([SEC-02](/engineering/security/controls)), so an app-specific scheme in a link gives no bridge access.

### 4.1 Markup analysis of the patched document

<Badge type="info" text="DESIGN" /> The client MUST run the markup analysis of [markup §2](/engineering/protocol/markup) on the patched document (section 2.4, step 8). The result MUST be valid, MUST have the same template revision and the same blocks (`id`, `format`, order), and its page revision MUST be equal to `next_revision`. This one check covers:

- M-22 (forbidden elements, so the element part of rule 1) and M-23, M-24 in `html` content: content cannot close the block element early or leave an element open;
- M-25 for `json` content (valid JSON, depth at most 64; so also no empty `json` content);
- RCDATA of a `title` block and script data of a `json` block: content cannot hold the end tag of its block (`</title`, `</script`) or move the script data into another state (`<!--` with `<script`);
- M-13 (no manifest-like `script` in the content);
- M-26 (block size and depth) and the size of the new document (`maxDocumentBytes`).

The analysis is one pass over at most 5 MiB, and the client needs the block offsets of the new snapshot anyway for the next patch. <Badge type="tip" text="FACT" /> The test `tools/rqp-schema/test/kat.test.mjs` has patches with correct hashes and a correct `next_revision` whose content breaks M-22, M-23, M-25 or the `title` element; only this analysis rejects them.

### 4.2 Attribute rule

<Badge type="info" text="DESIGN" /> Attributes are not a markup rule: a full document can have an event-handler attribute in a block, and a patch cannot. The client MUST apply this rule to every start tag in the content of each patched `html` block, as the tokenizer of the analysis of section 4.1 reads it ([markup §2.3](/engineering/protocol/markup#_2-3-tokenization-and-token-stack)). The rule checks every attribute of the start tag, also a duplicate attribute that the tokenizer drops (M-03).

1. If the attribute name (in lower case, as the tokenizer gives it) starts with `on`, reject.
2. If the raw value of the attribute (markup §2.1: no character reference decoding) contains a `&` that does not start the five bytes `&amp;`, reject. This fails closed without a character reference decoder, as M-13 does: `&#106;`, `&colon;`, `&Tab;` and `&AMP;` are all rejected.
3. Make a copy of the raw value. In the copy, replace each `&amp;` with `&`. Remove every byte `09` (tab), `0A` (LF) and `0D` (CR). Then remove all bytes `00` to `20` at the start.
4. If the copy starts with `javascript:` (11 bytes), `vbscript:` (9 bytes) or `data:` (5 bytes), compared ASCII case-insensitively, reject (H-59).

Items 2 to 4 apply to every attribute, not only to URL attributes. So item 4 also rejects a `title` or `alt` text that starts with `data:`; put such text in the element content. An image in a block uses a URL path, not a `data:` URL. A `title` block has no start tags in its content (RCDATA), and a `json` block has no attributes, so the rule does not apply to them.

A server SHOULD apply the same rule, with the same three schemes, to the new content of each changed `html` block before it sends a delta, and send the full document when the rule rejects (section 2.5, item 7; H-56, H-59).

<Badge type="tip" text="FACT" /> Items 3 and 4 follow the WHATWG URL parser, which removes leading and trailing C0 control or space and removes all ASCII tab or newline before it reads the scheme (URL Standard, "basic URL parser"). So `" java\tscript:x"` is a `javascript:` URL for a browser and for this rule. The same normalization applies to `vbscript:` and `data:`.

| Raw attribute | Result |
|---|---|
| `onclick="x()"`, `ONLOAD=x` | Reject (item 1) |
| `href="javascript:x"`, `href=" JavaScript:x"` | Reject (item 4) |
| `href="vbscript:x"`, `href="data:text/html,x"` | Reject (item 4, H-59) |
| `src=" ⇥Da␊Ta:x"` (tab and LF inside) | Reject (item 4) |
| `href="java&#x09;script:x"`, `title="a &quot; b"`, `href="?a=1&b=2"` | Reject (item 2) |
| `href="/p?a=1&amp;b=2"`, `title="Tom &amp; Jerry"` | Accept |
| `data-x="javascript"`, `href="/javascript:x"`, `href="database:x"` | Accept |

## 5. Error codes

<Badge type="info" text="DESIGN" /> The codes come from the [error catalog](/engineering/core/errors) (H-50). Every code ends in the documented fallback: the client shows the page from the network ([INV-05](/engineering/architecture/invariants)).

| Code | Category | When |
|---|---|---|
| `MANIFEST_INVALID` | Protocol | A check of section 1.7 (client) or 1.8 fails, also the UTF-8 check (client item 6, H-62) and the markup analysis of the original document |
| `PATCH_TOO_LARGE` | ResourceLimit | Envelope larger than `E` (section 1.5), content larger than `maxBlockBytes`, or patched document larger than `maxDocumentBytes` |
| `PATCH_INVALID` | Protocol | A delta for a request that did not offer one; section 3.2; the schema (also an `op` other than `replace_block`); the operation rules of section 2.3; a final URL whose cache key (scheme, host, port, path, query, partition) is not the cache key of the snapshot (H-58) |
| `TEMPLATE_MISMATCH` | Protocol | Section 2.4, step 1 |
| `PATCH_BASE_MISMATCH` | Protocol | Section 2.4, step 2 |
| `INTEGRITY_ERROR` | Protocol | Section 2.4, step 4 (operation hash), step 7 (`next_revision` from the hashes; also after a change of `policy` or `limits` on the server) or step 8 (page revision of the patched document) |
| `PATCH_CONTENT_REJECTED` | SecurityViolation | Section 4: the markup analysis of the patched document (section 2.4, step 8) or the attribute rule (step 9). The client also counts a metric (errors rule 3) |
| `AUTH_PARTITION_CHANGED` | AuthChanged | The partition changed between the request and the response ([diff engine §3](/engineering/core/diff-engine#_3-rqp-patch-validation)), also for a `304` ([negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), item 1; DL-036) |

The client logs the code, the revisions and the sizes; never content, the URL query or the manifest text ([errors and logging](/engineering/architecture/errors-logging)).

## 6. Limits

| Item | Limit | Name in [resource limits](/engineering/core/resource-limits) |
|---|---|---|
| Blocks in a manifest | 256 | `maxBlocks` |
| Operations in a patch | 256 (one per block) | `maxBlocks` |
| Block id | 64 characters (M-15) | — |
| Content of one block | 1 MiB (UTF-8 bytes) | `maxBlockBytes` |
| Manifest text | 64 KiB | `maxManifestBytes` |
| Original and patched document | 5 MiB | `maxDocumentBytes` |
| Envelope | `min(maxPatchBytes, max_patch_bytes)` | `maxPatchBytes` |
| JSON depth | 3 used; parser limit 16 | `maxJsonDepth` |
| `origin`, `path` | 512, 4 096 characters | — |

## 7. Schemas and examples

| File | Content |
|---|---|
| `spec/rqp/schemas/manifest.schema.json` | `$id` `urn:rivqen:rqp:1:manifest`, JSON Schema Draft 2020-12 |
| `spec/rqp/schemas/patch.schema.json` | `$id` `urn:rivqen:rqp:1:patch`, JSON Schema Draft 2020-12 |
| `spec/rqp/examples/` | Valid and invalid instances (each invalid instance has one defect and the expected error) and two known-answer sets in `kat/`: the example page without and with `policy` and `limits` |
| `tools/rqp-schema/` | Tests: `npm ci --ignore-scripts`, then `npm test` |

<Badge type="info" text="DESIGN" /> The schemas reject unknown members (`additionalProperties: false`) and check the patterns of ids, hashes and revisions and the ranges of this page. A schema check is necessary, not sufficient: unique block ids, duplicate member names, the order and uniqueness of operations, byte sizes, hashes, `next_revision` and the content rules are client checks of sections 1.8, 2.4, 3.2 and 4. When this page and a schema disagree, this page is right.

<Badge type="tip" text="FACT" /> On 2026-10-12, `tools/rqp-schema` (Ajv 8.20.0, strict mode) passed 142 tests: both schemas are valid Draft 2020-12 schemas, 10 valid and 67 invalid instances give the expected results (`remove_block` and `insert_block` fail, H-47), and both known-answer sets agree with the Node.js markup candidate, also for the patched documents. The tests also cover the client checks of H-48 (sections 1.8 and 4.1), the policy line of H-55 (section 1.3), the full documents `kat/full.html` and `kat/full-policy.html` (section 1.7) and the `Rq-Have` value of the example page.

## Related

- [RQP markup](/engineering/protocol/markup)
- [RQP negotiation](/engineering/protocol/negotiation)
- [Diff engine](/engineering/core/diff-engine)
- [Resource limits](/engineering/core/resource-limits)
- [Web and React SDK](/engineering/platforms/web-react)
