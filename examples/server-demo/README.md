# Rivqen server demo

A small Node.js server that shows the Rivqen protocol (RQP, `rqp/1`) from the server side: `data-rq-block` markup, inline manifest, `304`, block delta and safe fallback. The server keeps no state per client. It passes the server protocol fixtures (`fixtures/rqp/protocol`). It is reference behavior for the protocol work, not the server SDK.

Documentation: https://robotoss.github.io/rivqen/guide/examples/server

## Run

```bash
npm install
npm start          # http://127.0.0.1:8787/catalog
npm test           # 65 tests, with the 67 server protocol fixtures
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

## Behavior that follows the review fixes (S2)

- **Head rule (DL-031, DL-034).** The manifest element goes before the head end tag of the document, not before the first `</head>` text. The head start tag may follow only white space, comments, DOCTYPE and `html` start tags. Inside the head only head content may come (`base`, `link`, `meta`, `script`, `style`, `template`, `title`, `noscript`, `noframes`, `basefont`, `bgsound`, comments). An end tag other than `head`, `body`, `html` and `br` is ignored there. Without such a head end tag, the server sends the application response unchanged.
- **Rq-* fields (DL-030).** The server removes every `Rq-*` field of the application response in all outcomes. A full document and a delta carry one `Rq-Version: 1` line.
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
