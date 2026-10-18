# Rivqen protocol: manifest and patch

This page defines the two RQP documents of `rqp/1`: the **page manifest** (what a page version contains) and the **patch envelope** (how a client goes from one page version to the next). It also defines the block hashes and the revisions.

**Status:** <Badge type="info" text="DESIGN" /> Normative for `rqp/1` from WP-17 S2. Human decisions ([WP-17 S1](/engineering/plan/sprints/WP-17-S1) §1, [WP-17 S2](/engineering/plan/sprints/WP-17-S2) §1): H-1 (inline manifest only), H-2 (full SHA-256, base64url), H-8 (revisions `t1.` and `r1.`), H-9 (no `page_id` and no `sequence` over HTTP), H-10 (hash in each operation), H-11 (JSON envelope, content as a string, no JCS), H-28 (ASCII case-insensitive match of the manifest type), H-44 (client block digests; the client verifies `next_revision`), H-45 (caching of full and delta responses), H-46 (`If-None-Match` carries the client revision), H-47 (only `replace_block`), H-48 (client markup analysis of the original and the patched document; attribute rule), H-49 (`http` only in debug builds; manifest position, replaced by H-72), H-50 (error codes, `maxManifestBytes`), H-51 (the same full document for every request), H-55 (`policy` and `limits` in the page revision), H-56 (the server sends the full document when a changed block fails the attribute rule), H-58 (binding to the final URL), H-59 (`vbscript:` and `data:` in the attribute rule), H-60 (privacy of the digests), H-61 (the `public` partition), H-62 (allowlisted final request, unknown credentials, UTF-8 only), H-63 (`Set-Cookie` gives the request partition), H-64 (the `public` partition only for a response whose header fields the native SDK saw in full), H-65 (the result of a delta on a `public` snapshot), H-66 (the exact `Content-Type` of the full document; the client gives every document to the WebView as UTF-8), H-67 (no `public` partition for a result that comes through a redirect), H-68 (no RQP for a URL with user information), H-69 (the SDK rejects an invalid allowlist entry); [WP-17 S3](/engineering/plan/sprints/WP-17-S3) §1: H-72 (the manifest immediately after the head start tag of the document; replaces the insertion point of H-49 and the head walk of DL-031 and DL-034), H-73 (the attribute rule also reads the start tags of a foreign region). H-71, H-74 and H-75 are markup rules ([markup](/engineering/protocol/markup)). Architect decisions: DL-030 (`Rq-*` fields of the application response), DL-031 (the head end tag of the document; replaced by H-72), DL-034 (the precise head rule; only its "before the head" item stays), DL-036 (the exact claim of the attribute rule), DL-037 (the details of the charset rule and of H-62), DL-038 (exact origin match of the allowlist, `Set-Cookie2`, two client charset cases), DL-039 (the details of H-64 to H-66; two entries of one cache key), DL-040 (bytes to the WebView, the origin from the URL object of the network stack, the metadata of a `public` entry after a `304`). JSON Schemas: `spec/rqp/schemas/`. Headers and the choice between a full document, a delta and `304`: [negotiation](/engineering/protocol/negotiation). Block markup: [markup](/engineering/protocol/markup).

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
| `format` | `"html"` or `"json"` (M-16). `json` is the format of both `json` types of M-16, `application/json` and `application/ld+json` (H-86); the type is in the template, so the manifest does not name it |
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
| `mode` | `"private"`: the snapshot belongs to the partition of the request ([cache identity](/engineering/cache/identity)). `"public"`: the client MAY store the snapshot in the shared `public` partition, but only when all of these are true (H-61, RQP-25): the app declares the origin public; the response did not come through a redirect (H-67, see "Redirects" below); the native SDK saw all header fields of the response (H-64, see "Credentials" below); the response has no `Cache-Control` directive `private` (also not with field names); the response has no `Set-Cookie` or `Set-Cookie2` field (H-63, DL-038); the response has the `Cache-Control` directive `public`, **or** the request had no credentials (see "Credentials" below; H-62). Else the snapshot belongs to the partition of the request. For the result of a delta on a `public` snapshot, see "Delta on a `public` snapshot" below (H-65). |
| `ttl_seconds` | Upper bound of the freshness lifetime of the snapshot: the client uses the smaller of the RFC 9111 freshness lifetime and `ttl_seconds`. With `0`, the client revalidates before it shows the snapshot. |
| `offline` | `true`: the client MAY show the stale snapshot on a network error when the HTTP headers allow it (`stale-if-error`, [freshness](/engineering/cache/policy) rule 6). `false`: the client never shows the snapshot on a network error. |

<Badge type="tip" text="FACT" /> RFC 9111 §3.5: "A shared cache MUST NOT use a cached response to a request with an Authorization header field to satisfy any subsequent request unless the response contains a Cache-Control field with a response directive that allows it to be stored by a shared cache". §5.2.2.9: `public` "permits a shared cache to reuse a response to a request containing an Authorization header field". <Badge type="info" text="DESIGN" /> The `public` partition is shared by all profiles of the app, so the client applies this rule to it, and treats `Cookie` like `Authorization` (H-61). The manifest alone never moves a snapshot out of the partition of the request: when a server sets `"mode": "public"` on a page with personal content, but the request had credentials and the response has no `Cache-Control: public`, the content stays in the partition of the user.

**Credentials (H-62, RQP-25).** <Badge type="info" text="DESIGN" /> The client MUST treat a request as credentialed unless it knows that no cookie, no `Authorization` field, no HTTP authentication and no client certificate was used for it. "No cookie" includes the cookies that the WebView cookie store adds to the request itself. A `Set-Cookie` field in the response also counts as credentials. When the client cannot tell (for example, the WebView sends the request and the client cannot see its fields), the request had credentials. So "a request without credentials" in the table above is a positive fact that the client knows, never a default. The rule only changes the second condition of the `public` partition; an explicit `Cache-Control: public` does not depend on it (H-61), but it does not override the next rule. **A client MUST NOT store a snapshot in the `public` partition when the response has a `Set-Cookie` field, also when the response has an explicit `Cache-Control: public` (H-63).** Reason: a page that sets a session cookie can carry personal content, so the rule fails closed. It is stricter than RFC 9111 on purpose. The cost is a lost shared snapshot only.

**Response fields not seen (H-64, RQP-25).** <Badge type="info" text="DESIGN" /> **The client MUST store a snapshot in the `public` partition only when the native SDK saw all header fields of the response, because it fetched the response itself.** For any other response (for example a response that the WebView loaded, or one that page script observed), the client acts as if the response had a `Set-Cookie` field, so the snapshot belongs to the partition of the request. <Badge type="tip" text="FACT" /> The Fetch Standard defines `Set-Cookie` and `Set-Cookie2` (byte-case-insensitive) as the forbidden response-header names, and the header list of a basic filtered response "excludes any headers in internal response's header list whose name is a forbidden response-header name" (checked on 2026-10-18; the standard was last updated on 6 October 2026). So page script never sees these fields, also for a same-origin response. <Badge type="info" text="DESIGN" /> So a client that sees only such a response cannot apply the rule of H-63, and the rule fails closed. The cost: a page that the WebView loads gets no shared snapshot.

**`Set-Cookie2` and field names (DL-038).** A `Set-Cookie2` field (RFC 2965, obsolete) counts as a `Set-Cookie` field in every rule of this section and of [cache identity](/engineering/cache/identity) rule 3. An SDK compares field names ASCII case-insensitively (RFC 9110 §5.1), so `set-cookie`, `Set-Cookie` and `SET-COOKIE2` are all such fields. An empty field value still counts.

**Redirects (H-67, RQP-25).** <Badge type="info" text="DESIGN" /> **A client MUST NOT store a snapshot in the `public` partition when the response came through a redirect**: one or more redirects in the chain, also a redirect to the same URL. This holds for a full document and for the result of a delta on a `public` snapshot. Reason: a redirect response can have a `Set-Cookie` field, and the network stack adds the cookie to the next request, so the final request can have credentials that the client does not see. The rule fails closed; the cost is that a page reached through a redirect gets no shared snapshot.

**Delta on a `public` snapshot (H-65, RQP-25).** <Badge type="info" text="DESIGN" /> A delta changes content that the server computes for this request. The result of a delta on a snapshot in the `public` partition stays in `public` only when the delta response meets the conditions of the table above and the client knows that the request had no credentials. A delta has `Cache-Control: no-store` ([negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server), step 7), so an explicit `Cache-Control: public` on a delta does not replace known absent credentials. The conditions include "no redirect" (H-67), so a delta response that came through a redirect never keeps the result in `public`. When a condition fails, the client stores the patched document as a snapshot in the partition of the request, and the `public` snapshot does not change. A `304` on a `public` snapshot has no content, so the snapshot stays as it is ([negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), item 1); a `304` that does not meet the conditions above does not update the stored metadata (freshness) of the `public` snapshot (DL-040, I-3). A snapshot in the partition of the request never moves to `public` through a delta.

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
| `path` | The path of the response URL as the WHATWG URL parser serializes it: it starts with `/`; non-ASCII bytes, the C0 controls and the characters space, `"`, `#`, `<`, `>`, `?`, `^`, `` ` ``, `{`, `}` are percent-encoded (the WHATWG path percent-encode set). The schema pattern of `path` (`spec/rqp/schemas/manifest.schema.json`) is a superset of this form: it also admits strings that the URL parser would not write. No query, no fragment. At most 4 096 characters. |

1. The client compares `origin` and `path` byte for byte with its own serialization of the **response URL** (the final URL after redirects), never with values from the manifest alone. On a difference the manifest is invalid (section 1.8). The client takes the response URL from the URL object that its network stack used for the final request, never from a string that another parser reads again (DL-040, L-3; [negotiation §6.1](/engineering/protocol/negotiation#_6-1-where-the-client-sends-rqp-fields), rule 1). A response URL with a user name or a password never gets RQP (H-68, rule 6 there), so the client does not use its manifest.
2. The query string is not part of the manifest, because a query can hold personal data. The client cache key keeps the query ([cache identity](/engineering/cache/identity)), so two URLs that differ only in the query have different snapshots with the same `path`.
3. The schemas accept `http` and `https` origins (H-49), so that a local development server can be tested. A client uses RQP for an `http` origin only in a **debug build** and only when the app sets an explicit flag for it. A **release build** uses RQP only for `https` origins: for an `http` URL it sends no `Rq-*` field and does a normal load ([negotiation §6.1](/engineering/protocol/negotiation#_6-1-where-the-client-sends-rqp-fields)). The rule applies to the final URL too: after a redirect to an `http` URL, a release build does not use the manifest (a normal page, no error) and does not accept a delta (H-58).
4. **Binding (H-58).** The client stores the new snapshot under the cache key of the final URL: the partition and the canonical URL with scheme, host, port, path and query ([cache identity](/engineering/cache/identity)). It applies a delta only to the snapshot with the cache key of the final URL of the delta ([negotiation §6.3](/engineering/protocol/negotiation#_6-3-response)). Rule 1 checks only origin and path, because the manifest has no query; the cache key adds the query and the partition.
5. The server takes `origin` from its configuration of the public origin of the site, not from the connection (a server behind a TLS proxy sees `http` and an internal host). `path` is the path of the target URI.

### 1.7 Delivery in the document

<Badge type="info" text="DESIGN" /> The server sends the manifest only inline, in the full document (H-1, [ADR-006](/engineering/architecture/adr/#adr-006)). There is no other delivery form.

**Server.**

1. The server inserts exactly these bytes, **exactly once** per document (H-51): `<script type="application/rivqen-manifest+json">`, then the manifest JSON text M, then `</script>`. The start tag has no other attribute and no white space other than the one space.
2. **Insertion point (H-72).** The insertion point is immediately after the `>` of the **head start tag of the document** in the original document (defined below, "Document head"). The server inserts no byte between that `>` and the manifest start tag. A document without a head start tag of the document is sent without RQP: the original document, no manifest, no `Rq-*` field, no ETag that RQP computed (as an invalid page, [negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server), step 2). Before the head start tag there are only white space, comments, DOCTYPE tokens, `html` start tags and ignored end tags, and `html` and `head` are not block names (markup M-16). So the insertion point is before the first block start tag, and the insertion never changes block content.
3. M follows section 3.2 and MUST NOT contain the byte `<` (`3C`). The schema patterns allow `<` in no string, so a serializer that writes no `<` in its syntax meets this rule. Then the content of the element cannot end early or change the script data state.
4. All other bytes of the original document stay unchanged. The hashes and revisions are those of the original document (section 1.3).
5. **One full document for every request** (H-51). The server inserts the manifest into the response for every request to a valid page, also for a request without RQP fields or with an unsupported `Rq-Version`. The full document does not depend on any request field ([negotiation §5.1](/engineering/protocol/negotiation#_5-1-full-document)).
6. **Stable bytes** (H-51, H-55). The ETag of the full document is the page revision ([negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server)). The page revision covers the template, the block contents and the members `policy` and `limits` (section 1.3). The other members come from the protocol version (`protocol`) and the target URI (`origin`, `path`; section 1.6). So the manifest value is a function of the page revision and the target URI by construction, and a change of `policy` or `limits` in the server configuration changes the page revision. One rule stays for the server: the serializer of M MUST be deterministic (the same manifest value gives the same bytes of M). Then one page revision always identifies one full document for one URI, as a strong validator requires.
7. **Content-Type (H-66).** The full document has exactly one field line `Content-Type: text/html; charset=utf-8`, whatever the `Content-Type` text of the application response was. The charset rule of [negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server) only decides if the page is passed through.

<Badge type="info" text="DESIGN" /> Example: `spec/rqp/examples/kat/full.html` is one valid full document of the example page: `kat/page.html` with the manifest element immediately after `<head>`. Another server can write M with other white space or member order (section 3.2); its full document has other bytes and the same page revision.

<Badge type="tip" text="FACT" /> A `script` element whose `type` is not a JavaScript MIME type, `module`, `importmap` or `speculationrules` is "a data block, which is not processed by the user agent" (WHATWG HTML §4.12.1; §4.12.1.1 step 13: "No script is executed"). The manifest element is such a data block, so it needs no CSP nonce.

**Client.**

1. The manifest element is the `script` start tag whose **first** `type` attribute has a raw value (markup §2.1, no character reference decoding) that is equal to `application/rivqen-manifest+json` by **ASCII case-insensitive byte comparison**: the value has exactly 32 bytes, and each byte is equal to the byte of the type after the bytes `41`–`5A` (A–Z) are mapped to `61`–`7A` (a–z) on both sides. No other mapping, no trimming of white space, no MIME parameters, no Unicode case folding (H-28). A value with a byte `80` or larger never matches.
2. A response with `Rq-Version: 1` has **exactly one** manifest element (H-51). No matching start tag, or more than one, means the document has no valid manifest (section 1.8).
3. **Position (H-72).** The manifest start tag is the first token after the head start tag of the document (below, "Document head"), with no gap: its `<` is the byte immediately after the `>` of that head start tag. Else the document has no valid manifest. This is also the case when the document has no head start tag of the document. White space between the head start tag and the manifest start tag is a gap, so the client rejects it too (`FX-RQ-C21-HEAD-WHITESPACE`): the server writes no byte there (server item 2), so a strict client loses no valid full document. The client finds the head start tag in the full document F with the walk of the server. The bytes of F before the insertion point are those of the original document, so the client finds the head start tag that the server found.
4. M is the bytes between the `>` of the start tag and the `<` of the next `</script` end tag.
5. The client gets the original document when it removes the bytes of the element (from the `<` of the start tag to the `>` of the end tag). The client stores the snapshot without the manifest element.
6. **UTF-8 only (H-62, RQP-26).** The client checks this item first. It uses the document only when the browser decodes it as UTF-8: the body starts with the UTF-8 byte order mark `EF BB BF`, or the `Content-Type` of the response has a UTF-8 charset by the charset rule of [negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server) (one field line, one media type, a charset parameter that is a UTF-8 label). Else the document has no valid manifest (section 1.8): the client shows it as a normal page and reports `MANIFEST_INVALID`. A server that follows negotiation §4 sends exactly `Content-Type: text/html; charset=utf-8` in every F (H-66) and passes a page with another charset through. The client stays as lenient as the browser for a byte order mark (DL-038): two `Content-Type` field lines without a byte order mark are not UTF-8 (`MANIFEST_INVALID`), and a byte order mark with `charset=iso-2022-jp` is UTF-8 (fixtures `FX-RQ-C26-*`). <Badge type="tip" text="FACT" /> In the HTML encoding sniffing algorithm, BOM sniffing comes first and a transport-layer encoding comes before the `<meta>` prescan (WHATWG HTML, "the encoding sniffing algorithm", steps 1 and 4), so either signal makes the browser decode UTF-8.
7. **Delivery to the WebView (H-66, RQP-26).** The client MUST give the full document and every snapshot (also a patched snapshot, and a snapshot that it shows offline or from its cache) to the WebView with the media type `text/html` and the encoding `utf-8`, and MUST decode the bytes only as UTF-8: never by a charset of the response, a `<meta>` element or detection. Item 6 decides only if the client uses the document for RQP; this item decides how the WebView reads every document that the client gives it. The platform adapter gives the **bytes** with the encoding `utf-8` (DL-040, L-2). When it must use an API that takes a string, it removes a leading `EF BB BF` before it decodes the bytes: a Java string and a Swift `String(decoding:as:)` keep the byte order mark as U+FEFF. That character is text before the `<head>` start tag: the HTML parser implies the head and the body, ignores the `<head>` start tag, and puts the manifest and the head content into the body of the WebView. Reason: the blocks, the revisions and the manifest position are computed on the UTF-8 bytes (security review round 3, L-2; code review round 3, finding 1). Details and platform notes: [negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), "Delivery to the WebView".

The client finds the element with the tokenizer of [markup §2.3](/engineering/protocol/markup#_2-3-tokenization-and-token-stack), so a manifest-like text in a comment or in raw text is not an element.

**Document head (H-72).** <Badge type="info" text="DESIGN" /> The server (server item 2) and the client (client item 3) find the **head start tag of the document** with the same walk: the tokenizer of [markup §2.3](/engineering/protocol/markup#_2-3-tokenization-and-token-stack) (M-04) from the start of the document. A **gap** is the bytes between the end of one token and the start of the next token: character data, and also the bytes of `</>`, which gives no token. **White space** is the bytes `09`, `0A`, `0C`, `0D` and `20`; a UTF-8 byte order mark at offset 0 also counts as white space here. An **ignored end tag** is an end tag whose name is not `head`, `body`, `html` or `br` (DL-034).

1. **Before the head.** Until the first `head` start tag, only these can come: gaps of white space, comments, DOCTYPE tokens, `html` start tags and ignored end tags. Then the first `head` start tag is the head start tag of the document. Any other token before it (another start tag, the end tag `</head>`, `</body>`, `</html>` or `</br>`, or the end of the input), or a gap with another byte, means: the document has no head start tag of the document.
2. **After the head start tag.** The walk stops at the head start tag of the document. The tokens after it do not change the insertion point: the head can have any content, and it does not need a `</head>` end tag. The markup analysis of [markup §2](/engineering/protocol/markup) still checks the whole document.

Tag names are in lower case, as the tokenizer gives them (`<HEAD>` is a `head` start tag). The attributes and the self-closing flag of the head start tag do not change the rule (`<head lang="en">`, `<head/>`): the insertion point is after its `>`.

| Document | Head start tag of the document |
|---|---|
| A normal page: DOCTYPE, `<html>`, `<head>`, head content | That `<head>` |
| A comment, a DOCTYPE and `<html lang="en">` before `<head>` | That `<head>` |
| A stray `</p>` before `<head>` | That `<head>`: `</p>` is an ignored end tag (DL-034) |
| `<head>`, no `</head>`; `<body>` closes the head | That `<head>`: no end tag is needed |
| `<head>` with `<noscript><img src="…"></noscript>` | That `<head>` |
| `<head>`, then a `template` in the body that holds `<head></head>` | The first `<head>`; the one in the template is template content |
| No `<head>` start tag; `<title>…</title></head>` | None: `<title>` came before a `head` start tag |
| Text before `<html>` or before `<head>` | None: a gap with text |
| `<meta charset="utf-8">` before `<head>` | None: the parser implies a head for head content |
| `</br>`, `</body>`, `</html>` or `</head>` before `<head>` | None: not an ignored end tag |
| `<head>` only in a comment | None |

<Badge type="tip" text="FACT" /> Item 1 follows the "initial", "before html" and "before head" insertion modes of the HTML parser (WHATWG HTML §13.2.6.4.1 to §13.2.6.4.3). In these modes, white space is ignored, a comment is inserted, a DOCTYPE token is processed or ignored, an `html` start tag creates the `html` element or adds its attributes, the end tags `head`, `body`, `html` and `br` act as "anything else", and "any other end tag" is a parse error and is ignored. "Anything else" implies the head (the parser inserts a head element "for a "head" start tag token with no attributes"), so a later `<head>` start tag is not the head of the DOM. For "A start tag whose tag name is "head"", "before head" says: "Insert an HTML element for the token. Set the head element pointer to the newly created head element. Switch the insertion mode to "in head"." In "in head", "A start tag whose tag name is "script"" inserts the element, switches the tokenizer to the script data state, sets the original insertion mode and switches the insertion mode to "text" (§13.2.6.4.4). In "text", the `</script>` end tag pops the element and switches back to the original insertion mode (§13.2.6.4.8). No step of item 1 and no step for the manifest element depends on the scripting mode: in "in head" only the `noscript` start tag does (§13.2.6.4.4), and the manifest comes before every `noscript` of the head. Checked on 2026-10-18 (HTML Living Standard, last updated on 16 October 2026).

<Badge type="info" text="DESIGN" /> **Parity.** From these facts:

- In both scripting views, the manifest element is the first child of the head element of the DOM: the scripting-enabled view of the WebView (markup M-04, DL-009) and the scripting-disabled view of a crawler or a native HTML parser.
- After the `</script>` of the manifest, the parser is in "in head" and the tokenizer is in the data state. This is the state after the head start tag of the original document, so the parser reads the rest of the full document as it reads the rest of the original document.
- The parity has one condition: the document is decoded as UTF-8 (client item 6, H-62). With another encoding the browser can see other tokens.

<Badge type="warning" text="RESEARCH" /> One-off checks of 2026-10-18 with parse5 8.0.1 (task WP-17 S3 T-08a; their scripts are not in the repository, so the numbers cannot be repeated from it), scripting enabled and disabled: in the 45 full documents of the protocol fixtures (the F of every page that the reference server accepts, every stored client response, and both known-answer full documents), the manifest element is the first child of the head and the only manifest element. In a sweep of 200 000 generated documents (white space, comments, DOCTYPE tokens, `html` start tags, end tags, text and head content before a head start tag; head content, `noscript`, body content and no `</head>` after it), the walk accepted 75 389 documents; for each, the manifest at the insertion point is the first child of the head in both views, and the client finds the same position. <Badge type="tip" text="FACT" /> The H-42 study found 0 disagreements for this rule (option HC in `evidence/markup-noscript/report.md`; scripts in `tools/rqp-markup/study/noscript`).

<Badge type="info" text="DESIGN" /> **Fail closed.** Rivqen finds no head start tag of the document for these tokens before the head, which the parser handles in another way: head content (the parser implies a head), text, `</br>`, `</body>`, `</html>`, `</head>` and `</>`. The document is then sent without RQP. A page whose minifier removed the `<head>` start tag also loses RQP. A later version can relax the rule. Clients of this version then show the newly allowed pages as normal pages (`MANIFEST_INVALID`), which is safe. Fixtures: `FX-RQ-P39-*`, `FX-RQ-C21-*` ([fixtures §4.1](/engineering/protocol/fixtures#_4-1-protocol-fixtures-fx-rq-p-fx-rq-c)).

<Badge type="info" text="DESIGN" /> **What H-72 replaces.** Before WP-17 S3, server item 2 inserted the manifest immediately before the head end tag of the document (H-49, DL-031), and a walk of the whole head found that end tag (DL-034). H-72 replaces the insertion point of H-49 and DL-031 and the head walk of DL-034. Only the "before the head" item stays, with the ignored end tags of DL-034. Reason (H-42, head part): with scripting disabled, a `noscript` start tag in the head switches the parser to "in head noscript". There, content such as an `img`, a `div` or text closes the `noscript` element and the head, so a manifest before `</head>` was in the body of that view. The study measured 54 of 773 corpus pages with this defect. With H-72, 786 pages get RQP (13 more pages, which have a head start tag but no head end tag by the old walk; 0 pages lost). A full document of the old form fails client item 3 (`FX-RQ-C21-OLD-POSITION`).

<Badge type="info" text="DESIGN" /> **Elements of the page after the manifest.**

- `<meta charset>`. The manifest comes before the `<meta charset>` element of the page, so with a long manifest that element ends after byte 1024 of the full document. <Badge type="tip" text="FACT" /> WHATWG HTML §4.2.5.4: "The element containing the character encoding declaration must be serialized completely within the first 1024 bytes of the document" (checked on 2026-10-18). <Badge type="info" text="DESIGN" /> This has no effect on decoding. The full document has `Content-Type: text/html; charset=utf-8` (server item 7, H-66). A byte order mark and a transport-layer encoding come before the `<meta>` prescan (client item 6). A `<meta charset>` changes the encoding only when "the confidence is currently tentative" (§13.2.6.4.4), and it is not with these signals. The client gives every document to the WebView as UTF-8 (client item 7). A conformance checker can still report an error for the full document. The original document and the snapshot do not change.
- `<meta http-equiv="Content-Security-Policy">`. The manifest comes before it. The manifest is a data block and is not executed (the FACT about data blocks above), so the policy does not need to cover it.

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

**Apply.** When all checks pass, the new snapshot is the patched document of step 8 (with the block offsets of its analysis), the block list `L'`, the page revision `next_revision`, the same template revision, and the same `policy` and `limits` (step 7 showed that they did not change). The content of a patched `json` block is the UTF-8 bytes of `content`. The new snapshot has the canonical URL of the snapshot; its partition is the partition of the snapshot, except for a snapshot in the `public` partition: the result stays in `public` only under the conditions of section 1.4, "Delta on a `public` snapshot"; else it goes to the partition of the request and the `public` snapshot does not change (H-65). The page and the cache change together ([diff engine §4](/engineering/core/diff-engine#_4-apply-plan)).

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

1. **Patch content rule (H-48, H-73, H-78; MUST).** The client MUST reject a patch when the new content of an `html` block has one of these:
   1. a start tag in HTML context whose name is in `PATCH_FORBIDDEN`: `base`, `basefont`, `bgsound`, `embed`, `iframe`, `link`, `meta`, `object`, `script`, `style`;
   2. a start tag in a foreign region (`svg`, `math`; markup M-05 item 4) whose name is not in `FOREIGN_ALLOWED` (below);
   3. an attribute that the attribute rule of section 4.2 rejects: an inline event handler (`on*`), or a value that is a `javascript:`, `vbscript:` or (except H-83) `data:` URL after URL normalization (H-59, DL-036, H-77).

   The client reads the start tags of the content as the markup analysis of section 4.1 tokenizes them ([markup §2.3](/engineering/protocol/markup#_2-3-tokenization-and-token-stack)): from the data state in HTML context, with the state changes of M-04 in HTML context and none in a foreign region. This is the same as reading the tokens of the content in the analysis of step 8. Names are compared in lower case, as the tokenizer gives them. A self-closing flag does not change the result (`<script/>` and `<link/>` reject). End tags do not matter. Error: `PATCH_CONTENT_REJECTED`.

   This rule is the only gate for these elements (H-78): since H-78 they are valid markup in a block, so the analysis of step 8 accepts a patched document that has them. The markup analysis still rejects the elements that M-22 keeps (section 4.1). A server SHOULD send the full document instead of a delta when the new content of a changed `html` block fails this rule (as H-56 for the attribute rule; section 2.5, item 7). The client check stays (MUST): the client never depends on the server check.
2. `json` content is data. The page renders it with its own code. This holds for both `json` types of M-16 (`application/json` and `application/ld+json`, H-86): a `json` block has no start tags in its content, so item 1 does not apply to it.
3. The client inserts `html` content only into SDK-owned regions. In React apps, prefer `json` blocks and state updates.

**`FOREIGN_ALLOWED`** (names as the tokenizer gives them, in lower case; human decision H-73): `svg`, `g`, `path`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, `rect`, `defs`, `symbol`, `use`, `title`, `desc`, `lineargradient`, `radialgradient`, `stop`, `clippath`, `mask`, `pattern`, `marker`, `text`, `tspan`, `filter`, `feblend`, `fecolormatrix`, `fecomponenttransfer`, `fecomposite`, `fedisplacementmap`, `fedropshadow`, `feflood`, `fefunca`, `fefuncb`, `fefuncg`, `fefuncr`, `fegaussianblur`, `femerge`, `femergenode`, `femorphology`, `feoffset`, `feturbulence`, `math`, `mrow`, `mi`, `mo`, `mn`, `ms`, `mtext`, `mspace`, `msub`, `msup`, `msubsup`, `mfrac`, `msqrt`, `mroot`, `mstyle`, `munder`, `mover`, `munderover`, `mmultiscripts`, `mprescripts`, `none`, `mtable`, `mtr`, `mtd`, `mpadded`, `mphantom`, `merror`, `semantics`, `annotation`.

<Badge type="info" text="DESIGN" /> `FOREIGN_ALLOWED` is not a WHATWG set. It is a security decision of the human (H-73), a patch rule since H-78. It has the SVG shapes, structure, paint servers, clipping and masking, text, the filter primitives without an external input, and the MathML Core presentation elements. It does not have the elements that run script or animate, or that navigate or load except as noted here: `a`, `image`, `feimage`, `foreignobject`, the SMIL elements (`animate`, `animatemotion`, `animatetransform`, `set`, `discard`, `mpath`), `script`, `style`, `textpath`, `cursor`, `view`, `maction` and `annotation-xml`. Every other name, also a name of a later SVG or MathML version, is not in the set, so the rule fails closed. Loads and navigation that stay: `use` can load a same-origin external SVG document (SVG 2 §5.5 allows no cross-origin and no `data:` request); an `href` on a MathML element can navigate in an engine that supports the MathML 3 attribute (MathML Core defines none; support NOT FOUND). The attribute rule (section 4.2, item 4) rejects `javascript:`, `vbscript:` and `data:` in both. Analysis: `evidence/markup-relax/report.md` §4. `PATCH_FORBIDDEN` has the elements that run script (`script`), load or embed another document or plug-in (`iframe`, `object`, `embed`), change the page style (`style`, `link`), change how the page reads URLs or fonts (`base`, `basefont`), play a sound (`bgsound`) or can change the encoding, refresh or navigate the page (`meta`) (H-78).

<Badge type="info" text="DESIGN" /> **What the rules give (H-59, DL-036, H-78).** Patched content brings into the page no element of `PATCH_FORBIDDEN` (item 1.1), no element that M-22 keeps (DOCTYPE, `html`, `head`, `body`, frames, `plaintext`, `select`, `option`, `optgroup`, `title`, `noscript`, `template`, `textarea`, `noembed`, `noframes`, `xmp`; section 4.1), no SVG or MathML element outside `FOREIGN_ALLOWED` (so no `a`, no SMIL animation element, no `foreignObject`, `image`, `script` or `style` in a foreign region; item 1.2, H-73), no inline event handler, and no attribute value that the URL parser reads as a `javascript:`, `vbscript:` or `data:` URL after character reference decoding, except `data:` raster images in `img`, `source` and `video` (section 4.2; H-77, H-83). The rules are not an HTML sanitizer. They do **not** cover:

| Not covered | Example |
|---|---|
| A later URL in a value with more than one URL, other than `srcset` | `ping="/p javascript:x"`: item 4 reads only the start of the value (`srcset` is split into candidates, H-83) |
| URLs in CSS | `style="background:url(javascript:x)"` |
| App-specific URL schemes | `href="intent:…"`, `href="myapp://pay"` |
| Requests and navigation by allowed elements | `<img src="/i.png">`, `<video>`, `<a href>`, `<form action>`: the rule rejects only the schemes of section 4.2, item 4, not the request |
| DOM clobbering | `id` or `name` values that shadow names that page code reads |
| SVG definitions that the page uses (H-73, H-76) | A block defines `<symbol id="icon">`, `<lineargradient id="g">`, `<filter id="f">`, `<clippath>`, `<mask>`, `<pattern>` or `<marker>` with an `id` that the page also uses. `getElementById` returns the first element with the id in tree order, so a `use href="#icon"` or `filter:url(#f)` of the page after the block can render the definition of the block |
| URLs in SVG presentation attributes | `fill`, `stroke`, `clip-path`, `mask`, `filter`, `marker-*` with `url(#…)`: they reference elements of the page or of another block (not `style`, but the same class) |
| `use` that copies other content | `<use href="#x">` copies page SVG or the SVG of another block into this block; the copy stays live, so a later patch of the other block changes this block. A same-origin external `use` document loads a resource (cross-origin and `data:` are not allowed by SVG 2 §5.5 and item 4) |
| Render cost | Nested `use` and large filter regions make rendering slow; a stored snapshot has the same cost on every load (platform risk A-08). A `data:` image is decoded on every load too (H-83) |
| Overlays through CSS | `style="position:fixed;inset:0"` over the real page |
| Full documents | A full document is the page of the server; the rules apply to patch content only. Since H-78 a block of a full document can have `script`, `style`, `iframe`, `object` and the other elements of `PATCH_FORBIDDEN`, and any SVG or MathML element that M-07 allows |
| Script gadgets of a framework | Attribute values or text that page code reads as code or as a template |

The Content Security Policy of the page ([SEC-04](/engineering/security/controls)) and the settings of the framework control script gadgets, CSS and the use of `id` and `name` by page code. A server SHOULD give the SVG definitions in blocks `id` values that no other part of the page uses, for example with a prefix of the block id (H-76). The native bridge MUST NOT be reachable through URL-scheme navigation ([SEC-02](/engineering/security/controls)), so an app-specific scheme in a link gives no bridge access.

### 4.1 Markup analysis of the patched document

<Badge type="info" text="DESIGN" /> The client MUST run the markup analysis of [markup §2](/engineering/protocol/markup) on the patched document (section 2.4, step 8). The result MUST be valid, MUST have the same template revision and the same blocks (`id`, `format`, order), and its page revision MUST be equal to `next_revision`. This one check covers:

- M-22 in `html` content: the elements that M-22 keeps (DOCTYPE, `html`, `head`, `body`, `frame`, `frameset`, `plaintext`, `select`, `option`, `optgroup`, `title`, `noscript`, `template`, `textarea`, `noembed`, `noframes`, `xmp`; H-78);
- M-23 and M-24 in `html` content: content cannot close the block element early or leave an element open; M-04 in the content: the raw text of a `script`, `style` or `iframe` in the content ends inside the content (M-20);
- M-07 in a foreign region of the content;
- M-25 for `json` content (valid JSON, depth at most 64; so also no empty `json` content);
- RCDATA of a `title` block and script data of a `json` block: content cannot hold the end tag of its block (`</title`, `</script`) or move the script data into another state (`<!--` with `<script`);
- M-13 (no manifest-like `script` in the content, also in `html` content since H-78);
- M-26 (block size and depth) and the size of the new document (`maxDocumentBytes`).

The analysis does **not** cover the patch content rule of section 4, item 1 (H-78): a patched `html` content with a `script`, an `iframe` or an SVG `a` element is valid markup. Only step 9 rejects it, so step 9 is a MUST for every patch. The analysis is one pass over at most 5 MiB, and the client needs the block offsets of the new snapshot anyway for the next patch. <Badge type="tip" text="FACT" /> The test `tools/rqp-schema/test/kat.test.mjs` has patches with correct hashes and a correct `next_revision` whose content breaks M-22, M-23, M-25 or the `title` element; only this analysis rejects them. <Badge type="warning" text="RESEARCH" /> Its M-22 case is a `script` element, which is valid markup since H-78; the test moves to step 9 with the reference (WP-17 S4, T-08).

### 4.2 Attribute rule

<Badge type="info" text="DESIGN" /> Attributes are not a markup rule: a full document can have an event-handler attribute in a block, and a patch cannot. The client MUST apply this rule to every start tag in the content of each patched `html` block, also to a start tag in a foreign region (`svg`, `math`; markup M-05 item 4 and M-07), as the tokenizer of the analysis of section 4.1 reads it ([markup §2.3](/engineering/protocol/markup#_2-3-tokenization-and-token-stack); H-73). The rule tokenizes the content as the analysis does: the state changes of M-04 apply only in HTML context, not in a foreign region. The rule checks every attribute of the start tag, also a duplicate attribute that the tokenizer drops (M-03). It reads the **raw value** (markup §2.1: no character reference decoding).

1. If the attribute name (in lower case, as the tokenizer gives it) starts with `on`, reject.
2. **Character reference at the scheme decision byte (human decision H-77).** Find the **decision byte** of the raw value: skip the bytes `00` to `20` at the start, then skip each byte `09`, `0A`, `0D` and each ASCII letter, ASCII digit, `+`, `-` and `.`. The decision byte is the first byte that is not skipped (none when the value ends first). If there is no decision byte, or it is not `&`, go to item 3. If it is `&`:
   1. If a numeric character reference starts at the `&` (read on the raw bytes as [markup M-13](/engineering/protocol/markup#_2-4-document-rules) says: decimal, or hexadecimal with `x` or `X`, leading zeros, `;` optional), reject when its value is `00`–`20`, `3A` (`:`), `2B` (`+`), `2D` (`-`), `2E` (`.`), an ASCII digit or an ASCII letter. Else go to item 3.
   2. Else, reject when the bytes after the `&` start with `Tab;`, `NewLine;`, `colon;`, `fjlig;`, `period;` or `plus;` (case-sensitive). Else go to item 3.

   The numeric reference MUST be read on the raw bytes, not on the copy of item 3.
3. Make a copy of the raw value. In the copy, remove every byte `09` (tab), `0A` (LF) and `0D` (CR). Then remove all bytes `00` to `20` at the start. (The copy keeps every character reference as it is; since H-77 it does not replace `&amp;`.)
4. **Schemes (H-59, H-83).** Compare the copy ASCII case-insensitively:
   1. If it starts with `javascript:` (11 bytes) or `vbscript:` (9 bytes), reject.
   2. If it starts with `data:` (5 bytes), reject, except when all of these are true (human decision H-83): the start tag is in HTML context; the element and the attribute are `img` with `src` or `srcset`, `source` with `src` or `srcset`, or `video` with `poster`; the copy starts with `data:image/` (11 bytes); and the copy does not start with `data:image/svg+xml` (18 bytes). So only raster image types pass: `data:image/png;base64,…` passes, `data:image/svg+xml,…`, `data:text/html,…`, `data:,x` and `data: image/png,…` (a space) reject.
   3. **`srcset` (H-83).** For the attribute `srcset` (of every element), do not use the whole value. Reject when the raw value has a character reference that decodes to ASCII white space, `,`, `(` or `)`: a numeric reference with the value `09`, `0A`, `0C`, `0D`, `20`, `2C`, `28` or `29`, or `&Tab;`, `&NewLine;`, `&comma;`, `&lpar;` or `&rpar;`. Else split the raw value into image candidates as the WHATWG algorithm "parse a srcset attribute" splits it: skip ASCII white space and commas; the URL is the next run of bytes that are not ASCII white space, without its final commas; the descriptors after it end at a comma that is not in parentheses. Apply items 2 to 4.2 to each candidate URL as if it were the value of `src` of the same element. A candidate that items 2 to 4.2 reject makes the attribute reject.

Items 2 to 4 apply to every attribute, not only to URL attributes. So item 4 also rejects a `title` or `alt` text that starts with `data:`, `javascript:` or `vbscript:`; put such text in the element content. A `title` block has no start tags in its content (RCDATA), and a `json` block has no attributes, so the rule does not apply to them.

A server SHOULD apply the same rule, with the same schemes and the same `data:` exception, to the new content of each changed `html` block before it sends a delta, and send the full document when the rule rejects (section 2.5, item 7; H-56, H-59).

<Badge type="tip" text="FACT" /> Items 3 and 4 follow the WHATWG URL parser, which removes leading and trailing C0 control or space and removes all ASCII tab or newline before it reads the scheme (URL Standard, "basic URL parser"). So `" java\tscript:x"` is a `javascript:` URL for a browser and for this rule. The same normalization applies to `vbscript:` and `data:`. The scheme is an ASCII letter followed by ASCII letters, digits, `+`, `-` and `.`, up to a `:`; every other character ends the scheme without a `:`, and the value is then not a URL with a scheme.

<Badge type="info" text="DESIGN" /> **Why item 2 is sound (H-77).** The browser decodes the character references of an attribute value before the URL parser reads it. Before the decision byte, the raw value has no `&`, so the decoded value has the same characters there. If the decision byte is not `&`, the decoded value has the same character at that place too, and the scheme check of item 4 on the copy gives the result of the URL parser. If the decision byte is `&`, its reference decides the scheme: a reference to a character that the URL parser strips or removes (`00`–`20`), to a scheme character (letters, digits, `+`, `-`, `.`) or to `:` can continue or complete a scheme, so item 2 rejects it. Any other reference, and a `&` that starts no reference, gives a character that ends the scheme without a `:`, so the value has no scheme and item 4 has nothing to find. <Badge type="tip" text="FACT" /> The named references that decode to such a character are exactly `&Tab;` (U+0009), `&NewLine;` (U+000A), `&colon;`, `&period;`, `&plus;` and `&fjlig;` (`fj`); all six need the `;` (`https://html.spec.whatwg.org/entities.json`, 2231 names, file of 2025-11-12, checked on 2026-10-18). Numeric values outside `01`–`7F` decode to U+FFFD or to a character that is not ASCII ([markup M-13](/engineering/protocol/markup#_2-4-document-rules)). <Badge type="info" text="DESIGN" /> **The trap of the audit.** The tokenizer ends a numeric reference at the first byte that is not a digit, also at a tab or LF, and the URL parser removes the tab or LF later. In `java&#x73⏎cript:x` (LF after `73`) the browser reads `s`, then removes the LF: `javascript:x`. On the copy of item 3, the LF is gone and the reference reads `&#x73c` (U+073C), which is not a scheme character. So an implementation that reads the reference on the copy passes this value; on the raw bytes, item 2 reads `73` (`s`) and rejects. <Badge type="tip" text="FACT" /> Check of S4 T-02 (2026-10-18): 300 000 generated values (half of them `javascript:`, `vbscript:` and `data:` with references, white space and controls), decoded by parse5 8.0.1 as attribute values and read with the scheme steps of the URL Standard: 127 690 are such URLs, and items 2 to 4 reject all of them (0 misses). The audit of 2026-10-19 found 0 misses in 7 000 000 values ([sprint record WP-17 S4](/engineering/plan/sprints/WP-17-S4) H-77).

<Badge type="info" text="DESIGN" /> **Why the `srcset` split is sound (H-83).** The WHATWG algorithm splits on ASCII white space and commas and reads parentheses in the descriptors; the browser runs it on the decoded value. A reference spans only bytes that are not white space, commas or parentheses (`&`, `#`, `x`, digits, letters, `;`). When no reference decodes to one of these characters, the decoded value has the white space, commas and parentheses at the same places as the raw value, so both splits give the same candidates, and item 2 handles the references at the start of each candidate URL. A `data:` URL can have commas (`data:image/png;base64,…`); they stay in the URL, because the URL ends only at white space. <Badge type="tip" text="FACT" /> "Collect a sequence of code points that are not ASCII white space from input given position, and let url be the result"; "If url ends with U+002C (,): Remove all trailing U+002C COMMA characters from url"; the descriptor tokenizer has a state "in parens" (WHATWG HTML Living Standard of 16 October 2026, §4.8.4.3.10 "Parsing a srcset attribute", checked on 2026-10-18). The named references that decode to white space, `,`, `(` or `)` are exactly `&Tab;`, `&NewLine;`, `&comma;`, `&lpar;` and `&rpar;` (entities.json, checked on 2026-10-18).

<Badge type="tip" text="FACT" /> **H-83 facts** (checked on 2026-10-18, T-01): the `img` element represents "a non-interactive, optionally animated, image resource that is neither paged nor scripted" (WHATWG HTML, §4.8.3 "The img element"); SVG 2 defines the secure animated and secure static processing modes, in which no script runs and no external resource loads (W3C SVG 2, `conform.html` §2.2.1, §2.2.4, §2.2.6, §2.3); the `poster` image of a `video` is fetched with the destination "image" (WHATWG HTML, `media.html#poster-frame`). <Badge type="warning" text="RESEARCH" /> How the browser and WebView engines implement these modes for `data:` images is NOT FOUND (only the specifications were checked). So the human excluded `data:image/svg+xml` (H-83): only raster images pass. `javascript:` and `vbscript:` stay rejected in every attribute, and every other use of `data:` stays rejected. A `data:` URL whose type is written in another form (for example `data:image/svg%2Bxml,…` or a space before the type) is not a raster type by name; item 4.2 rejects the space form and passes the percent form as an image type other than SVG (engine handling NOT FOUND).

| Raw attribute | Result |
|---|---|
| `onclick="x()"`, `ONLOAD=x` | Reject (item 1) |
| `href="javascript:x"`, `href=" JavaScript:x"` | Reject (item 4) |
| `href="vbscript:x"`, `href="data:text/html,x"` | Reject (item 4, H-59) |
| `src=" ⇥Da␊Ta:x"` (tab and LF inside) | Reject (item 4) |
| `href="java&#x09;script:x"`, `href="&#106;avascript:x"`, `href="java&colon;x"`, `href="&Tab;javascript:x"` | Reject (item 2: the decision byte is `&`, and the reference gives a tab, `j` or `:`) |
| `href="java&#x73⏎cript:x"` (LF after `73`, no `;`) | Reject (item 2 on the raw bytes: `73` is `s`; the trap of the audit) |
| `title="a &quot; b"`, `href="?a=1&b=2"`, `href="/p?a=1&amp;b=2"`, `title="Tom &amp; Jerry"`, `alt="&#169; 2026"` | Accept (H-77: the decision byte is not `&`, or the reference gives `"`, `&` or `©`) |
| `href="data&amp;:x"`, `href="&COLON;x"` | Accept (`&` ends the scheme; `&COLON;` is not a reference) |
| `data-x="javascript"`, `href="/javascript:x"`, `href="database:x"` | Accept |
| `<img src="data:image/png;base64,iVBO…">`, `<video poster="data:image/jpeg;base64,…">` | Accept (item 4.2, H-83) |
| `<img src="data:image/svg+xml,…">`, `<img src="data:text/html,x">`, `<a href="data:image/png,…">`, `<svg><image href="data:image/png,…">` | Reject (item 4.2) |
| `<img srcset="/a.png 1x, data:image/png;base64,iVBO… 2x">` | Accept (item 4.3: each candidate passes) |
| `<img srcset="/a.png 1x, data:image/svg+xml,… 2x">`, `<source srcset="/a.png 1x, javascript:x 2x">` | Reject (item 4.3) |
| `<img srcset="/a.png 1x,&#32;data:text/html,x 2x">` | Reject (item 4.3: a reference that decodes to white space) |

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

<Badge type="tip" text="FACT" /> On 2026-10-12, `tools/rqp-schema` (Ajv 8.20.0, strict mode) passed 142 tests: both schemas are valid Draft 2020-12 schemas, 10 valid and 67 invalid instances give the expected results (`remove_block` and `insert_block` fail, H-47), and both known-answer sets agree with the Node.js markup candidate, also for the patched documents. The tests also cover the client checks of H-48 (sections 1.8 and 4.1), the policy line of H-55 (section 1.3), the full documents `kat/full.html` and `kat/full-policy.html` (section 1.7) and the `Rq-Have` value of the example page. On 2026-10-18 the same 142 tests passed with the full documents of H-72 (the manifest immediately after `<head>`).

## Related

- [RQP markup](/engineering/protocol/markup)
- [RQP negotiation](/engineering/protocol/negotiation)
- [Diff engine](/engineering/core/diff-engine)
- [Resource limits](/engineering/core/resource-limits)
- [Web and React SDK](/engineering/platforms/web-react)
