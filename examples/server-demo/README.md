# Rivqen server demo

A small Node.js server that shows the Rivqen protocol (RQP, `rqp/1`) from the server side: `data-rq-block` markup, inline manifest, `304`, block delta and safe fallback. The server keeps no state per client. It passes the server protocol fixtures (`fixtures/rqp/protocol`). It is reference behavior for the protocol work, not the server SDK.

Documentation: https://robotoss.github.io/rivqen/guide/examples/server

## Run

```bash
npm install
npm start          # http://127.0.0.1:8787/catalog
npm test           # 45 tests, with the 57 server protocol fixtures
npm run measure    # bytes per visit; needs the server running
```

## Endpoints

| Method | Path | Result |
|---|---|---|
| GET, HEAD | `/catalog` | Full HTML with the manifest, `304`, or a delta (`application/vnd.rivqen.delta+json`) |
| POST | `/__demo/next-data` | Change price and stock (localhost only) |
| POST | `/__demo/next-template` | Change the template (localhost only) |
| GET | `/healthz` | Health and demo state |

## Protocol fixtures

```bash
node ../../tools/rqp-protocol/check.mjs --server src/rqp.js
```

The module `src/rqp.js` exports `handle(request, appResponse, pageBytes, config)`, the interface of the harness. The markup analysis calls `tools/rqp-markup/node` by a relative path, so the demo needs the repository checkout.

## License

Apache-2.0. See the repository `LICENSE`.
