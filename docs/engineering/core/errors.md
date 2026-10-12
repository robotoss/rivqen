# Errors

Rivqen errors are typed. Each error has a category, a stable code, and a **fallback hint** that tells the adapter what to do. Errors never contain secrets.

**Status:** <Badge type="info" text="DESIGN" /> RQP codes `MANIFEST_INVALID`, `PATCH_INVALID` and `PATCH_CONTENT_REJECTED`: decided by the human (H-50, [WP-17 S2](/engineering/plan/sprints/WP-17-S2)).

## 1. Categories

| Category | Meaning | Default behavior |
|---|---|---|
| `Network` | Connection, DNS, timeout, reset | Fallback to normal HTTPS load (if the network is back) or show cached page if policy allows |
| `Protocol` | Malformed header, body, marker or JSON | Fallback; invalidate the entry if the cache is the cause |
| `CacheCorruption` | Integrity or schema check failed on a cache entry | Delete the entry; fetch over HTTPS |
| `AuthChanged` | Partition changed during the session | Stop the session; drop access to the old snapshot |
| `UnsupportedCapability` | The platform or the server lacks a needed feature | Disable that optimization only |
| `Cancelled` | The user or app cancelled | No action |
| `SecurityViolation` | Origin, partition, TLS or content-policy check failed | Stop; never show the content; log a safe code |
| `Timeout` | A deadline passed | Fallback |
| `PlatformConstraint` | The OS refused an operation (background, memory) | Degrade; retry later |
| `ResourceLimit` | A size, memory or time budget was exceeded | Abort the operation; fallback |
| `Internal` | A bug: broken invariant or caught panic | Fallback; report |

## 2. Error codes

| Code | Category | Adapter behavior |
|---|---|---|
| `PROTOCOL_UNSUPPORTED` | UnsupportedCapability | Try the allowed downgrade, else normal HTTPS |
| `MANIFEST_INVALID` | Protocol | The RQP page manifest failed a client check ([manifest and patch §1.8](/engineering/protocol/manifest-patch#_1-8-client-checks-of-the-manifest)): no or two manifest elements, size, JSON, schema, URL, or the markup analysis of the original document. Show the document as a normal page; store no snapshot. Hint: `NormalLoad` |
| `PATCH_INVALID` | Protocol | An RQP delta is malformed or unsolicited: JSON rules, schema (also an unknown operation), operation rules, another URL, or a delta for a request that did not offer one ([negotiation §6.3](/engineering/protocol/negotiation#_6-3-response)). Reject the patch; one fallback load of the full document. Hint: `NormalLoad` |
| `PATCH_BASE_MISMATCH` | Protocol | Reject the patch; request a full snapshot |
| `TEMPLATE_MISMATCH` | Protocol | Full reload |
| `INTEGRITY_ERROR` | Protocol | Reject; full reload |
| `PATCH_CONTENT_REJECTED` | SecurityViolation | The patched content failed the content rules ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)). Show nothing of the patch; one fallback load of the full document from the network; count the metric (rule 3). Hint: `NormalLoad` |
| `CACHE_CORRUPTED` | CacheCorruption | Delete the entry; fetch over HTTPS |
| `AUTH_PARTITION_CHANGED` | AuthChanged | Stop the session; clear access to the old snapshot |
| `TLS_ERROR` | SecurityViolation | **Do not bypass.** Return the error to the UI. Do not show sensitive stale HTML unless policy allows it. |
| `ORIGIN_NOT_ALLOWED` | SecurityViolation | Do not load through Rivqen; normal load if allowed by the app |
| `WEBVIEW_CAPABILITY_MISSING` | UnsupportedCapability | Disable that optimization, not the whole navigation |
| `TRANSPORT_DISCONNECTED` | Network | HTTP revalidation with backoff; resync |
| `MEMORY_BUDGET_EXCEEDED` | ResourceLimit | Free L1 cache, stop prefetch, fallback |
| `PATCH_TOO_LARGE` / `DOCUMENT_TOO_LARGE` | ResourceLimit | Fallback |
| `TIMEOUT_FIRST_BYTE` / `TIMEOUT_TOTAL` | Timeout | Fallback |
| `INTERNAL_ERROR` | Internal | A broken invariant or a caught panic. Fallback; report with location and build id. |
| `LEGACY_MODE_USED` | Notice | Not an error: the deprecated legacy module served this session. One warning per origin per app start. |
| `SERVER_PASSTHROUGH` | Server | Server SDK error: the original response is sent unchanged. |

## 3. Rules

1. Every error maps to exactly one fallback hint: `None`, `RetryLater`, `NormalLoad`, `ShowCachedIfAllowed`, `Stop`.
2. Error messages are for developers. They contain a code, a category and safe context (session id, state, byte counts). They never contain URLs with query strings, headers, cookies, tokens or HTML.
3. A `SecurityViolation` is always reported as a metric, even when telemetry export is off (local counter only).
4. The adapter behavior of a code has priority over the default of its category. Example: `PATCH_CONTENT_REJECTED` is a `SecurityViolation`, but the rejected content is never shown, so a normal load of the page from the network is safe (H-50).
5. RQP has no generic `PROTOCOL_ERROR` code. A delta that the client did not ask for is an invalid patch (`PATCH_INVALID`).

## Related

- [Core API outcomes](/engineering/core/api#_3-outcomes)
- [Troubleshooting](/guide/troubleshooting)
- [Error handling and logging rules](/engineering/architecture/errors-logging)
