# Cache and revalidation

Rivqen keeps a local copy of each page. On a repeat visit it shows the copy at once and asks the server if anything changed. This is called **revalidation**.

## 1. Where Rivqen stores pages

| Layer | Content | Size (initial) |
|---|---|---|
| Memory | Recently used pages | 8–32 MB per app, by device class |
| Disk | Pages from earlier sessions | 64–256 MB per app, by device class |

The SDK removes the least recently used pages when a limit is reached. On low memory, the SDK frees its memory cache at once.

## 2. How revalidation works

1. The app opens a page that is in the cache.
2. Rivqen shows the cached page.
3. Rivqen sends a request with the version of the cached page.
4. The server answers "not modified", "data changed" or "template changed".
5. Rivqen updates the page and the cache, or keeps them as they are.

## 3. Rules that always apply

| Rule | Result |
|---|---|
| The server sends `Cache-Control: no-store` | Rivqen does not store the page. |
| The page needs login | Rivqen does not cache it, unless your app **and** your server allow it. Then the cache is encrypted. |
| The user logs out | Rivqen removes access to that user's pages at once. |
| The cached copy is damaged | Rivqen deletes it and loads the page from the network. |

## 4. What you control

| Setting | Where | Effect |
|---|---|---|
| `Cache-Control` headers | Server | How long a page is fresh, whether it can be stored |
| `stale-while-revalidate` | Server | How long Rivqen may show an old page while it checks for a new one |
| Cache budgets | App configuration | Memory and disk limits |
| Authenticated caching | App configuration | Off by default |
| Clear cache | App code | `engine.clearCache(partition)` |

## Related

- [Offline mode](/guide/concepts/offline)
- [Security guarantees](/guide/concepts/security)
