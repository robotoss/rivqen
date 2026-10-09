# Rivqen server demo

A small Node.js server that shows the Rivqen protocol (RQP) from the server side: `data-rq-block` markup, manifest, `304`, block patch and safe fallback. It is reference behavior for the protocol work, not the server SDK.

Documentation: https://robotoss.github.io/rivqen/guide/examples/server

## Run

```bash
npm install
npm start          # http://127.0.0.1:8787/catalog
npm test           # 14 unit tests
npm run measure    # bytes per visit; needs the server running
```

## Endpoints

| Method | Path | Result |
|---|---|---|
| GET | `/catalog` | Full HTML, `304`, or a patch (`application/vnd.rivqen.delta+json`) |
| POST | `/__demo/next-data` | Change price and stock (localhost only) |
| POST | `/__demo/next-template` | Change the template (localhost only) |
| GET | `/healthz` | Health and demo state |

## License

Apache-2.0. See the repository `LICENSE`.
