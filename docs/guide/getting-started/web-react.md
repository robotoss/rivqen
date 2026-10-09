# Web and React

This guide shows how a web page receives Rivqen data updates. The page runs inside a WebView that uses the Rivqen mobile SDK.

::: warning Planned API — not released
:::

## Requirements

- A page served by a Rivqen server SDK.
- React 18 or later for the React adapter. Plain JavaScript works with `@rivqen/web-core`.

## Step 1 — Install the packages

```bash
npm install @rivqen/web-core @rivqen/react
```

## Step 2 — Add the provider

1. Wrap your app in `RivqenProvider`.
2. List the origins that may talk to the native SDK.

```tsx
// Planned API — not released
import { RivqenProvider } from '@rivqen/react'

export function App() {
  return (
    <RivqenProvider allowOrigins={['https://shop.example.com']}>
      <ProductPage />
    </RivqenProvider>
  )
}
```

## Step 3 — Read data from a region

1. Use `useRivqenRegion` to read the current value of a data block.
2. Render it with React. Do not write the HTML into the DOM yourself.

```tsx
// Planned API — not released
import { useRivqenRegion, useRivqenRevision } from '@rivqen/react'

function ProductPage() {
  const price = useRivqenRegion<{ amount: string }>('price')
  const revision = useRivqenRevision()
  return (
    <main>
      <p className="price">{price?.amount}</p>
      <small>Revision {revision}</small>
    </main>
  )
}
```

## Step 4 — Plain JavaScript

```ts
// Planned API — not released
import { connect } from '@rivqen/web-core'

const client = connect({ allowOrigins: ['https://shop.example.com'] })
client.onUpdate((update) => {
  for (const block of update.blocks) {
    // Update only regions that your code owns.
  }
})
```

## Rules

1. Do not use `eval` or `innerHTML` with update content.
2. Keep one owner for each piece of state: React state **or** an SDK-owned region, not both.
3. If the page and the update have different revisions, let the SDK reload the page.

## Next steps

- [Engineering: Web and React SDK](/engineering/platforms/web-react)
