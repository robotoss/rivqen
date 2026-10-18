# Rivqen server demo

A small Node.js server that shows the Rivqen protocol (RQP, `rqp/1`) from the server side: `data-rq-block` markup, inline manifest, `304`, block delta and safe fallback. The server keeps no state per client. It passes the server protocol fixtures (`fixtures/rqp/protocol`). It is reference behavior for the protocol work, not the server SDK.

Documentation: https://robotoss.github.io/rivqen/guide/examples/server

## Run

```bash
npm install
npm start          # http://127.0.0.1:8787/catalog
npm test           # 95 tests, with the 74 server protocol fixtures
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

- **Head rule (DL-031, DL-034).** The manifest element goes before the head end tag of the document, not before the first `</head>` text. The head start tag may follow only white space, comments, DOCTYPE and `html` start tags. Inside the head only head content may come (`base`, `link`, `meta`, `script`, `style`, `template`, `title`, `noscript`, `noframes`, `basefont`, `bgsound`, comments). An end tag other than `head`, `body`, `html` and `br` is ignored there. Without such a head end tag, the server sends the application response unchanged.
- **Rq-* fields (DL-030).** The server removes every `Rq-*` field of the application response in all outcomes. A full document and a delta carry one `Rq-Version: 1` line.
- **Charset (H-62).** The page is UTF-8. When the `Content-Type` of the application response has a `charset` parameter that is not a WHATWG label of UTF-8 (`unicode-1-1-utf-8`, `unicode11utf8`, `unicode20utf8`, `utf-8`, `utf8`, `x-unicode20utf8`; ASCII case-insensitive, quotes removed), the server sends the response unchanged. The full document always has exactly one `Content-Type` line with the value `text/html; charset=utf-8` (H-66), whatever the text of the application response was. The server reads the field as the rule says ([negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server)): one field line with one media type (else it sends the response unchanged), the parameter name is not trimmed, and, when there is no charset, the value with `; charset=utf-8` appended is read again (a quoted string that is not closed makes the page invalid; the appended text is not sent). A delta is not changed by this rule.
- **Validators (DL-032).** The full document has no `Last-Modified`; its ETag is the page revision.
- **Attribute rule (H-59).** A changed html block with an `on*` attribute, a bad character reference, or a URL that starts with `javascript:`, `vbscript:` or `data:` (after the normalization of manifest-patch section 4.2) gets the full document.
- **Logs (DL-033).** The request log has a route template (`/catalog`, `/healthz`, the demo routes, or `<unmatched>`), never the raw path or query.

## Protocol fixtures

```bash
node ../../tools/rqp-protocol/check.mjs --server src/rqp.js
```

The module `src/rqp.js` exports `handle(request, appResponse, pageBytes, config)`, the interface of the harness. The markup analysis calls `tools/rqp-markup/node` by a relative path, so the demo needs the repository checkout.

## License

Apache-2.0. See the repository `LICENSE`.
