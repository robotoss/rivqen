# Example: Web

The web demo is a small React page that runs inside the Android or iOS demo web view. It shows how page code reads updated blocks through the JS bridge.

**Status:** <Badge type="info" text="DESIGN" /> planned for P6 (WP-12, WP-22) · code uses the planned API

[[toc]]

## 1. What the demo shows

| Screen | What happens in the page |
|---|---|
| **Live diff** | `useRivqenRegion('price')` gets the new value. React updates only the price text. |
| **Revision** | `useRivqenRevision()` shows the current page revision. |
| **Without Rivqen** | In a normal browser, the hooks return the server-rendered values. Nothing breaks. |

## 2. Planned code

```tsx
// Planned API — not released
import { RivqenProvider, useRivqenRegion, useRivqenRevision } from '@rivqen/react'

export function App() {
  return (
    <RivqenProvider allowOrigins={['http://127.0.0.1:8787']}>
      <Catalog />
    </RivqenProvider>
  )
}

function Catalog() {
  const price = useRivqenRegion<string>('price')
  const stock = useRivqenRegion<string>('stock')
  const revision = useRivqenRevision()
  return (
    <section>
      <p>Today: {price} EUR</p>
      <p>{stock}</p>
      <small>Revision {revision ?? 'none'}</small>
    </section>
  )
}
```

## 3. Rules the demo follows

1. Do not write update content with `innerHTML`. Render it with React.
2. Accept messages only from the native bridge of the allowed origin.
3. Keep server-rendered values as the default. The page must work without Rivqen.

## 4. What to measure

1. Time from patch arrival to the DOM update (`performance.mark`).
2. Number of React re-renders per patch.
3. LCP and CLS with and without Rivqen.

## Related

- [Getting started: Web and React](/guide/getting-started/web-react)
- [JS bridge contract](/engineering/platforms/js-bridge)
