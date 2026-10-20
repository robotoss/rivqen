# Rivqen server demo

A small Node.js server that shows the Rivqen protocol (RQP, `rqp/1`) from the server side: `data-rq-block` markup, inline manifest, `304`, block delta and safe fallback. The server keeps no state per client. It passes the server protocol fixtures (`fixtures/rqp/protocol`). It is reference behavior for the protocol work, not the server SDK.

Documentation: https://robotoss.github.io/rivqen/guide/examples/server

## Run

```bash
npm install
npm start          # http://127.0.0.1:8787/catalog
npm test           # 104 tests, with the 91 server protocol fixtures
npm run measure    # bytes per visit; needs the server running
```

## Endpoints

| Method | Path | Result |
|---|---|---|
| GET, HEAD | `/catalog` | Full HTML with the manifest, `304`, or a delta (`application/vnd.rivqen.delta+json`) |
| POST | `/__demo/next-data` | Change price and stock (localhost only, see below) |
| POST | `/__demo/next-template` | Change the template (localhost only, see below) |
| GET | `/healthz` | Health and demo state |

### POST protection

The two POST endpoints reject cross-site requests (DL-033). A request needs the header `Rq-Demo-Action: 1`. When the request has an `Origin` header, it must equal the origin of the demo (the `ORIGIN` setting). Otherwise the server answers `403` and changes nothing.

```bash
curl -s -X POST -H 'Rq-Demo-Action: 1' http://127.0.0.1:8787/__demo/next-data
```

`npm run measure` sends the header.

### Host check

Every route answers only a request whose `Host` header equals the host of the configured origin, compared without case. The default port of the origin scheme (`:443` for https, `:80` for http) is the same authority as no port, so both forms are accepted (RFC 9110 sections 4.2.1 and 4.2.2); any other port gives `421`. Otherwise the server answers `421` with no body and changes nothing (DL-036 I4). The `Origin` check compares with the normalized origin (a trailing slash or an upper-case host in the `ORIGIN` setting is fine). `npm run measure` sends the host of `ORIGIN` (or of `DEMO_URL`).

## Behavior that follows the review fixes (S2)

- **Head rule (H-72, WP-17 S3; H-79a, WP-17 S4).** The manifest element goes immediately after the `>` of the head start tag of the document, with no byte between them. When the first head content (`base`, `basefont`, `bgsound`, `link`, `meta`, `noframes`, `script`, `style` or `title`) comes before any head start tag, the head is implied, and the manifest element goes immediately before that start tag (H-79a). `noscript`, `template`, text or body content first gives no insertion point. The head start tag may follow only white space, comments, DOCTYPE tokens, `html` start tags and end tags other than `head`, `body`, `html` and `br` (these are ignored, DL-034). A `<head>` text in a comment or in a `template` does not count. What comes after the head start tag does not matter: the head does not need a `</head>` end tag. Without such a head start tag, the server sends the application response unchanged. This replaces the S2 rule "before the head end tag" (DL-031): with scripting disabled, a `noscript` in the head can close the head early, and the manifest was then in the body.
- **Rq-* fields (DL-030).** The server removes every `Rq-*` field of the application response in all outcomes. A full document and a delta carry one `Rq-Version: 1` line.
- **Charset (H-62).** The page is UTF-8. When the `Content-Type` of the application response has a `charset` parameter that is not a WHATWG label of UTF-8 (`unicode-1-1-utf-8`, `unicode11utf8`, `unicode20utf8`, `utf-8`, `utf8`, `x-unicode20utf8`; ASCII case-insensitive, quotes removed), the server sends the response unchanged. The full document always has exactly one `Content-Type` line with the value `text/html; charset=utf-8` (H-66), whatever the text of the application response was. The server reads the field as the rule says ([negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server)): one field line with one media type (else it sends the response unchanged), and the parameter name is not trimmed. A value without a charset (also one with a quoted string that is not closed) gives the full document with the exact value (DL-039). A delta is not changed by this rule.
- **Validators (DL-032).** The full document has no `Last-Modified`; its ETag is the page revision.
- **Patch content rule (H-56, H-59, H-73, H-77, H-78, H-83; WP-17 S4).** The rule reads every start tag of the block, also in an `svg` or `math` region (the text-mode state changes apply only in HTML context). A changed html block gets the full document when it has a start tag `script`, `style`, `link`, `meta`, `noscript` (H-96), `iframe`, `object`, `embed`, `base`, `basefont` or `bgsound` in HTML context (`PATCH_FORBIDDEN`); a `template` is allowed and the rule reads the start tags of its contents (H-94), a start tag outside `FOREIGN_ALLOWED` in a region, an `on*` attribute, a character reference at the scheme decision byte that can continue or complete a scheme (H-77; read on the raw bytes), or a URL that starts with `javascript:`, `vbscript:` or `data:` after the normalization of manifest-patch section 4.2. A raster `data:image/…` URL (not SVG, no `%` or `&` in the type) is allowed in `src` and `srcset` of `img` and `source` and in `poster` of `video`; every `srcset` candidate is checked (H-83). The rule is a second implementation of the rule of `tools/rqp-protocol` (DL-036, DL-046); `test/differential.test.js` compares the two.
- **Logs (DL-033).** The request log has a route template (`/catalog`, `/healthz`, the demo routes, or `<unmatched>`), never the raw path or query.

## Protocol fixtures

```bash
node ../../tools/rqp-protocol/check.mjs --server src/rqp.js
```

The module `src/rqp.js` exports `handle(request, appResponse, pageBytes, config)`, the interface of the harness. The markup analysis calls `tools/rqp-markup/node` by a relative path, so the demo needs the repository checkout.

## License

Apache-2.0. See the repository `LICENSE`.
