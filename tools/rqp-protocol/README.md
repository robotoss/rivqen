# rqp-protocol — protocol fixtures, reference steps and conformance harness

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

This package makes and checks the golden fixtures of `rqp/1` negotiation, manifest and patch in `fixtures/rqp/protocol/` (WP-17 S2, task T-03). Normative text: `docs/engineering/protocol/negotiation.md` (section 4: server, section 6: client), `docs/engineering/protocol/manifest-patch.md` (sections 1.3, 1.7, 1.8, 2.4, 3.2, 4), `docs/engineering/core/diff-engine.md` section 3. Catalog: `docs/engineering/protocol/fixtures.md` section 4.1. Fixture format: `fixtures/rqp/protocol/README.md`.

The reference functions here are the source of the expected results. They are not an SDK: they are small, readable and slow, and they stop on input that they do not model.

## Contents

| Path | Content |
|---|---|
| `gen.mjs` | Generator. Writes the generated pages and every `expected.json` and `response.body`. `--check` changes nothing and fails on a stale, missing or extra file |
| `check.mjs` | Conformance harness. Runs a server or a client under test over the fixtures and compares the results |
| `src/server.mjs` | Reference server step: negotiation section 4, steps 1 to 9 (with the removal of the `Rq-*` fields of the application response, DL-030, without its `Last-Modified` on F, DL-032, and the charset rule, H-62), one function for each field rule of section 3 |
| `src/client.mjs` | Reference client step: request fields (section 6.2) and response handling (section 6.3) with the manifest checks and the delta checks in the order of diff-engine section 3; the binding of a `304`, a manifest and a delta to the request and to the cache key of the final URL (H-58); the allowlist of the final URL, the UTF-8 check of a full document and the credentials rule (H-62); the partition of a new snapshot (H-61) |
| `src/markup.mjs` | The markup analysis of the Node.js candidate (`tools/rqp-markup/node`), the head end tag of the document (manifest-patch section 1.7, DL-031, DL-034), the manifest elements, the attribute rule with `javascript:`, `vbscript:` and `data:` (manifest-patch section 4.2, H-59) |
| `src/sf.mjs` | Structured Field Values parser (RFC 9651 section 4.2): Item, List, Dictionary, all bare item types |
| `src/http.mjs` | Field lines, combined values, entity-tags, `Accept`, media type, the charset parameter of `Content-Type` and the UTF-8 labels of the Encoding Standard (H-62) |
| `src/json-strict.mjs` | JSON parser with the rules of manifest-patch section 3.2 (no duplicate member names, no lone surrogate, integers as digits, depth 16) |
| `src/schema.mjs` | A small validator for the JSON Schema keywords that `spec/rqp/schemas` uses. It reads the schema files. An unknown keyword is an error |
| `src/common.mjs` | Limits, error codes, hashes, the page revision with the policy line (H-55) |
| `src/pages.mjs` | Page recipes and the expected markup result of each page |
| `src/fixtures.mjs` | `case.json` reading, placeholders, response recipes of client cases, expected files |
| `test/` | `node --test` tests |

## Requirements

- Node.js 22 or later.
- No dependency. The package imports the Node.js markup candidate by a relative path (`../rqp-markup/node/src/`), which has no runtime dependency. It does not use Ajv: `src/schema.mjs` checks the same schema files, and its tests compare it with every example of `spec/rqp/examples` (the keyword and the instance path that Ajv 8.20.0 reports).

## Procedure: run the checks

1. Go to the repository root.
2. Run the tests:

   ```sh
   (cd tools/rqp-protocol && node --test "test/*.test.mjs")
   ```

3. Make sure that the fixtures are current:

   ```sh
   node tools/rqp-protocol/gen.mjs --check
   ```

4. Run the harness against the reference steps:

   ```sh
   node tools/rqp-protocol/check.mjs
   ```

Exit codes of `gen.mjs` and `check.mjs`: `0` success, `1` a stale file, a defective case or a failed fixture, `2` a usage error.

A case can have `"level": "should"` (today only `FX-RQ-P35-ATTRIBUTE-RULE` and `FX-RQ-P35-ATTRIBUTE-RULE-DATA`, H-56). The generator copies the member to `expected.json`; any other value is a defective case. `check.mjs` prints the failure of such a fixture as `FAIL [SHOULD] <name>: …` and counts it apart: the last line is `rqp-protocol check: <passed> passed, <failed> failed, <n> SHOULD failed`. A SHOULD failure also gives exit code `1`, so it is never hidden; a conforming server that chooses not to follow the SHOULD rule reports it as a known deviation.

## Procedure: check a server under test

1. Write an ES module that exports `handle(request, appResponse, pageBytes, config)`.
   - `request`: `{ method, target, headers }`. `headers` is a list of `[name, value]` lines with lower-case names, in order. A name can repeat.
   - `appResponse`: `{ status, headers }`, the application response before the RQP step.
   - `pageBytes`: a `Uint8Array`, the page bytes P.
   - `config`: `{ policy?, limits? }`, the manifest members of the server configuration. Most fixtures give `{}`.
   - Return `{ status, headers, body }` or a promise of it. `headers`: `[name, value]` lines, a plain object or a `Headers` object. `body`: a `Uint8Array`, a string or `null`.
2. Configure the server with the public origin `https://shop.example.com`. The path comes from `request.target`.
3. Do not apply a content coding. The fixtures expect the strong ETag.
4. Run:

   ```sh
   node tools/rqp-protocol/check.mjs --server path/to/module.mjs
   ```

5. Use `--only FX-RQ-P09` to run the fixtures whose name starts with a prefix.

## Procedure: check a client under test

1. Write an ES module that exports `clientStep(context, response)` and, if possible, `clientRequest(context)`.
   - `context`: `{ url, final_url, build, allow_http, allowlisted, credentials, public_origin, navigation, snapshot }`. `url` is the request URL and the cache key URL of the snapshot; `final_url` is the URL of the response after redirects (H-58). `allowlisted` is `false` when the origin of the final URL is not in the origin allowlist of the app; every other origin of the context is in the allowlist (RQP-05, H-62). `credentials` is `true` when the request had credentials, `false` when the client knows that it had none (no cookie, also none from the WebView cookie store, no `Authorization`, no HTTP authentication, no client certificate), and `null` when the client does not know; `null` counts as `true` (H-62). `public_origin` is `true` when the app declares the origin public (H-61). `navigation` is `first-visit`, `delta` or `fallback`. `snapshot` is `null` or `{ document, template_revision, page_revision, blocks, policy?, limits? }`, where `document` is the original document (without the manifest).
   - `response`: `{ status, headers, body }`.
   - `clientStep` returns `{ outcome, error, next, metric, snapshot }` (values below).
   - `clientRequest` returns the request field lines.
2. Run:

   ```sh
   node tools/rqp-protocol/check.mjs --client path/to/module.mjs
   ```

| Field | Values |
|---|---|
| `outcome` | `not-modified`, `patched`, `stored` (a full document with a valid manifest), `normal` (not RQP), `rejected` |
| `error` | `null` or a code of manifest-patch section 5 |
| `next` | `keep` (304), `show` (show this response), `fallback-load` (one GET without delta fields), `webview-load` (the WebView loads the URL; no second fallback). A `304` that the client cannot bind to its snapshot gives `normal` and `webview-load` with no error (H-58) |
| `metric` | `true` only for `PATCH_CONTENT_REJECTED` |
| `snapshot` | `null`, or the new snapshot: `url` (the canonical final URL: WHATWG serialization without the fragment) and `partition` (`request` or `public`), the cache key of the snapshot (H-58, H-61); `template_revision`, `page_revision`, `blocks` (`id`, `format`, `sha256`, and `content` when it has at most 4096 bytes), `document_sha256` (SHA-256 of the stored original document, base64url), `policy` and `limits` when the manifest has them |

## Comparison rules of check.mjs

Server fixtures (`fixtures.md` section 4.1 "Comparison of F", section 5):

| Part | Rule |
|---|---|
| Status | Exact |
| `headers_required` | The combined value (lines joined with `, `) is exactly the expected value. Names are case-insensitive |
| `headers_forbidden` | The field is absent |
| `headers_optional` (`304` only) | When the field is present: exactly one field line, and its value is exactly the expected value. Today only `rq-version: 1` (DL-036) |
| `Vary` of F and of `304` | No member `rq-*` and no `accept` (RQP-08) |
| `Content-Length` | When present with a body: equal to the body length |
| Body `none` | No body or an empty body (`304`, `HEAD`) |
| Body `app` | The page bytes, exact |
| Body `full` | The bytes before `insert_at` and after the manifest element are the page bytes; the element is exactly `<script type="application/rivqen-manifest+json">` M `</script>` at `insert_at`; M has no `<`; M passes the JSON rules of manifest-patch section 3.2 and is equal, as a JSON value, to `manifest`. White space and member order of M are free |
| Body `delta` | JSON rules and patch schema; `base_revision`, `next_revision`, `template_revision` exact; the operations are the expected `blocks` in order (`id`, `format`, `sha256`); each `sha256` is the hash of its `content` |

Client fixtures: `outcome`, `error`, `next`, `metric` exact; `snapshot` equal (block `content` is compared only where `expected.json` lists it); request fields `rq-version`, `rq-capabilities`, `if-none-match`, `rq-have` exact, no other `rq-*` field, and the same answer to "does `Accept` list the delta media type".

## Design

**Invariants of the reference steps.**

1. The expected result of a fixture comes only from `src/server.mjs` or `src/client.mjs`. A person writes `case.json` and never edits `expected.json` or `response.body`.
2. An input that the reference steps do not model throws `UnsupportedInput`: a content coding, `If-Match`, `If-Unmodified-Since`, `If-Range`, `If-Modified-Since` without `If-None-Match`. The generator reports it as a defective case. So no fixture depends on an unmodelled path.
3. The page revision of a manifest with `policy` or `limits` has the policy line (H-55). The markup candidate gives the revision without it, so both steps compute it again from the template revision and the block hashes.
4. The server step follows H-56 (SHOULD): a changed `html` block that fails the attribute rule gives F.
5. Pages and fixtures have no `policy` and no `limits`, except `FX-RQ-P36-*`, `FX-RQ-C19-*` and `FX-RQ-C24-*` (`"mode": "public"`).
6. The client models the cache key only by its URL (canonical final URL) and the partition choice. Partition keys, a partition change (`AUTH_PARTITION_CHANGED`, also for a `304`, DL-036), `Vary`, `no-store` and the freshness rules are not modelled; a context snapshot is in the partition of the request.
7. UTF-8 only (H-62). The server step passes a page through when the `Content-Type` of A has a charset that is not a UTF-8 label, or more than one field line or media type; it appends `; charset=utf-8` when A has no charset, and parses the result again (a value that does not give UTF-8 also passes the page through). The client step uses a full document only with a UTF-8 byte order mark or a UTF-8 charset. The charset parser follows the parameter steps of WHATWG MIME Sniffing "parse a MIME type" and the label table of the Encoding Standard; a seeded property test (`test/charset.test.mjs`) checks that every F of the server has a UTF-8 charset.
8. The allowlist is modelled by one flag for the origin of the final URL (`allowlisted`). The client step uses a full document only when the final request carried RQP fields; a `304` and a delta from another origin fail the cache key binding already (H-58, H-62).

**Resource limits.** All parsers are linear in their input. The server reads `Rq-Have` only up to 4096 bytes and `Rq-Capabilities` up to 1024 bytes before it parses them. The JSON parser stops at depth 16. The token-stack walk for the head end tag and the manifest elements stops at 256 entries (fails closed); the head rule adds constant work per token and reads the bytes between tokens once; for a document that is valid for RQP the markup rule M-26 keeps the stack below this limit. The client checks the envelope size before it parses the envelope.

**Failure modes.** Every parser returns "absent" or an error code for malformed input and never throws for a string or a byte input (seeded property tests in `test/sf.test.mjs`). A server under test that throws fails its fixture; the harness continues with the next fixture.

## Add a fixture

1. Make the directory `fixtures/rqp/protocol/FX-RQ-<P|C><NN>-<NAME>/`. `<NN>` is the catalog number.
2. Write `case.json` (server format: `fixtures/rqp/protocol/README.md`; client format: below).
3. Run `node tools/rqp-protocol/gen.mjs`.
4. Read the new `expected.json`. Make sure that it is the result that the catalog states.
5. Run `node tools/rqp-protocol/gen.mjs --check` and the tests.
6. Ask the protocol owner for a review.

Server `case.json` members in addition to the README format: `server_config` (`policy`, `limits` of the server configuration) and `client_members` (the `policy` and `limits` of the manifest that the client holds; the `{{r1:…}}` placeholder uses them).

### Client `case.json`

```json
{
  "id": "FX-RQ-C01-BASE-MISMATCH",
  "kind": "client",
  "description": "A delta with base_revision r1(page-b), not r1(page-a)",
  "rules": ["RQP-13"],
  "context": { "navigation": "delta", "snapshot": "page-a" },
  "response": {
    "status": 200,
    "headers": [["content-type", "application/vnd.rivqen.delta+json"], ["rq-version", "1"]],
    "body": { "delta": { "from": "page-a", "to": "page-b", "base_revision": "page-b" } }
  }
}
```

| Member | Rule |
|---|---|
| `context.url` | Default `https://shop.example.com/catalog` |
| `context.final_url` | Default: `context.url` (no redirect) |
| `context.build`, `context.allow_http` | Default `release`, `false` |
| `context.allowlisted` | Default `true`. `false`: the origin of `final_url` is not in the allowlist (H-62) |
| `context.credentials`, `context.public_origin` | `credentials`: `true`, `false` or `null` (unknown, the default; counts as `true`, H-62). `public_origin`: default `false` |
| `context.navigation` | `first-visit`, `delta` or `fallback` |
| `context.snapshot` | `null`, a page id, or `{ "page": id, "members": { "policy": …, "limits": … } }` |
| `response.headers` | Literal lines; the placeholders of the README are allowed |
| `response.body` | `null`; `{ "page": id }` (the page bytes); `{ "full": id, … }` (F of the page; options `members`, `manifest_of`, `manifest_type`, `manifest_position: "body"` (before `</body>`) or `"head-text"` (before the first `</head>` bytes in any context; default: the head end tag of the document), `manifest_count: 2`, `duplicate_member: true`); `{ "delta": { "from": id, … } }` (options `to` or `ops: [{ block_id, content, sha256? }]`, `base_revision`, `next_revision`, `template_revision` as page ids, `next_members`, `size` (exact envelope bytes), `duplicate_member: true`). Without options, a delta is consistent: correct hashes and a `next_revision` computed from the snapshot |

The generator writes `expected.json` (context with the snapshot values, the request fields, the response with `body_bytes` and `body_sha256`, the result) and `response.body` (exact bytes).
