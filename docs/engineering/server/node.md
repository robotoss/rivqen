# Node.js server SDK

**Package:** `@rivqen/server` (working name). **Work package:** WP-13 (first server SDK).

**Status:** <Badge type="info" text="DESIGN" />

## 1. Upstream for comparison

<Badge type="tip" text="FACT" /> Upstream `sonic-nodejs` is a Koa 2 demo (`"koa": "^2.2.0"`) with a compression middleware that buffers the stream, calls the differ, then gzips the result (`upstream:sonic-nodejs/middleware/compress.js`, `common/diff.js`). The same logic is published on npm as `sonic_differ`. Notable behaviors: legacy processing runs only when the body is a stream and the client accepts gzip or deflate; the etag header name is configurable (`sonic-etag-key`); pages without markers always get full HTML. See [Divergences](/engineering/protocol/legacy-divergences).

## 2. Design

| Item | Design |
|---|---|
| Core API | `renderWithRivqen(request: Request, render: () => Promise<string \| ReadableStream>, options): Promise<Response>` on the Web Fetch API |
| Adapters | `@rivqen/server/express`, `@rivqen/server/fastify`, `@rivqen/server/next` (route handler and middleware helpers) |
| Runtimes | Node.js LTS lines (24, 26); Bun and Deno through the Fetch API, tested best-effort |
| Streaming | Bounded spool: buffer the document up to `maxDocumentBytes`; compute hashes when the document ends; abort on client disconnect |
| Compression | Not in the SDK. Let the framework or proxy compress after the SDK. |
| Hashing | `node:crypto` SHA-1 (legacy), SHA-256 (RQP) |
| Config | `modes`, `paths`, `etagHeaderName`, `cacheOffline` default, `maxDocumentBytes`, `profile` |

## 3. Rules

1. Detect legacy requests with `accept-diff: true` (canonical profile).
2. Keep all request state local to the call. No module-level mutable state.
3. Never change the response for non-Rivqen clients.
4. Set `Cache-Control: no-cache, private` on legacy responses unless the app sets a stricter value.
5. Pass the full conformance suite, directly and behind the proxy lab.

## Related

- [Server SDKs overview](/engineering/server/)
- [Getting started: Node.js](/guide/getting-started/server-node)
