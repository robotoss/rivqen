# Offline mode

Rivqen can show a page without network, but only when the server allows it and the page is in the cache.

## 1. When a page opens offline

All of these must be true:

1. The page is in the cache for the current account.
2. The server allowed offline use of the page (see section 2).
3. Your app did not mark the page as "online only".

If one condition is false, the app shows its normal offline error.

## 2. How the server allows offline use

| Protocol | Server signal |
|---|---|
| HTTP (all modes) | `Cache-Control` with `stale-if-error` |
| Legacy protocol | The `cache-offline` header. See [legacy wire contract](/engineering/protocol/legacy-wire#cache-offline). |
| Rivqen protocol | The `policy` object of the page manifest |

## 3. Show the user that a page is old

Rivqen tells your app when it shows an old copy. Your app can show a label such as "Offline — updated 2 hours ago".

```kotlin
// Planned API — not released
session.events.collect { event ->
    if (event is SessionEvent.ServedStale) showOfflineBanner(event.age)
}
```

::: warning Sensitive pages
Do not allow offline use for pages with balances, orders or other data that must be current. Mark these routes as sensitive in the app configuration.
:::

## 4. Offline packages

An offline package is a versioned set of pages and resources that the app downloads in advance from a trusted origin. Rivqen checks the integrity of each file. This feature is planned for a later release.
