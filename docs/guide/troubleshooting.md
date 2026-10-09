# Troubleshooting

Use this page when Rivqen does not give the result you expect. Each problem has a cause and a procedure.

::: info Planned
The error codes come from the planned API. See [Errors](/engineering/core/errors) for the full list.
:::

## The page always loads in full

| Possible cause | Check | Fix |
|---|---|---|
| The origin is not in the allowlist | Session outcome is `Fallback(ORIGIN_NOT_ALLOWED)` | Add the origin to `allowedOrigins`. |
| The server does not mark data blocks | The response has no markers | Add markers or the SSR helper. |
| The server sends `Cache-Control: no-store` | Response headers | Remove `no-store` for pages that may be cached. |
| The template changes on every request | `TemplateChanged` on every visit | Move changing content (timestamps, random IDs, CSRF tokens) into data blocks. |

## The page shows old data

1. Check the `Cache-Control` header of the page.
2. Check whether the session outcome is `ServedStale`.
3. If the page is sensitive, mark the route as sensitive in the configuration.

## A blank screen appears

A blank screen must not happen. Rivqen falls back to a normal load on errors.

1. Record the session events.
2. Report the problem with the event log (no cookies or tokens) as a GitHub issue.

## Data from another account appears

::: danger Stop and report
This is a security problem. Do not open a public issue. Follow the [security policy](https://github.com/robotoss/rivqen/blob/main/SECURITY.md).
:::

## The JS bridge does not connect

| Cause | Fix |
|---|---|
| The page origin is not in `bridgeOrigins` | Add the exact origin (scheme, host, port). |
| The page runs in an iframe | The bridge works only in the main frame unless the iframe origin is allowed. |
| The web SDK version does not match | Use the same minor version for the web SDK and the mobile SDK. |

## Error code quick reference

| Code | Meaning | What to do |
|---|---|---|
| `PATCH_BASE_MISMATCH` | An update did not match the page version | Nothing. Rivqen reloads the page. Report if frequent. |
| `CACHE_CORRUPTED` | A cache entry failed its check | Nothing. Rivqen deletes it. Report if frequent. |
| `TLS_ERROR` | Certificate problem | Fix the server certificate. Rivqen never bypasses it. |
| `MEMORY_BUDGET_EXCEEDED` | Memory limit reached | Reduce the page size or raise the budget. |
| `DOCUMENT_TOO_LARGE` | Page above the size limit | Reduce the page or raise `maxDocumentBytes`. |
