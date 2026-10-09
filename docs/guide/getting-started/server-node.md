# Server: Node.js

This guide adds Rivqen to a Node.js server that renders HTML.

::: warning Planned API — not released
:::

## Requirements

- A maintained Node.js LTS version (see [Support matrix](/guide/support-matrix)).
- A framework with Fetch `Request`/`Response`, Express, or Fastify.

## Step 1 — Install

```bash
npm install @rivqen/server
```

## Step 2 — Mark data blocks in your HTML

1. Add the `data-rq-block` attribute to each element whose content changes.

```html
<p class="price" data-rq-block="price">120.00</p>
```

## Step 3 — Add the middleware

::: code-group

```ts [Express]
// Planned API — not released
import express from 'express'
import { rivqen } from '@rivqen/server/express'

const app = express()
app.use(rivqen())
```

```ts [Fastify]
// Planned API — not released
import Fastify from 'fastify'
import { rivqenPlugin } from '@rivqen/server/fastify'

const app = Fastify()
await app.register(rivqenPlugin)
```

```ts [Fetch API]
// Planned API — not released
import { renderWithRivqen } from '@rivqen/server'

export default async function handler(request: Request): Promise<Response> {
  return renderWithRivqen(request, () => renderPage())
}
```

:::

## Step 4 — Set cache headers

1. Add `Cache-Control: private` to personal pages.
2. Add `Cache-Control: no-store` to pages that must never be cached.

## Step 5 — Check normal visitors

1. Open the page in a desktop browser.
2. Check that the HTML is the same as before the middleware. The middleware changes responses only for Rivqen clients.

## Next steps

- [Engineering: Node.js server SDK](/engineering/server/node)
