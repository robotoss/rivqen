# Data model

This page defines the main data structures of the Rust core. Adapters, fixtures and tests use the same names.

**Status:** <Badge type="info" text="DESIGN" /> Field names can change in WP-05. The meaning of each field is the contract.

[[toc]]

## 1. Identifiers

| Type | Format | Meaning | Rules |
|---|---|---|---|
| `SessionId` | 128-bit random, opaque | One navigation session | Never reused. Not derived from the URL. |
| `Generation` | `u64`, monotonic per session | Version of the session state | Increments on reload, account change, cancel. Events with an old generation are ignored. |
| `RequestId` | `u64`, monotonic per engine | One HTTP request or channel message | Correlates actions and events. |
| `NavigationId` | opaque string | One navigation as seen by the page | Sent to the web SDK in bridge messages. |
| `PartitionId` | 256-bit keyed hash | Account / tenant / environment scope | Computed by the adapter from non-secret IDs and a device key. Never a token. |
| `CacheKey` | 256-bit hash | One cache entry | See [Cache identity](/engineering/cache/identity). |
| `Revision` | string, ≤ 128 bytes | Version of a page or template | legacy: opaque, derived from `etag` / `template-tag`. RQP: content-derived `t1.…` / `r1.…`, 46 ASCII characters ([manifest and patch §1.3](/engineering/protocol/manifest-patch#_1-3-hashes-and-revisions)); compared byte for byte. |
| `V1SessionKey` | string | Upstream-compatible session/cache id derived from the URL | Only in legacy mode. See [legacy client behavior](/engineering/protocol/legacy-client). |

## 2. Request context

`RequestContext` is the input to `open`. The adapter builds it. The core never reads platform state directly.

| Field | Type | Notes |
|---|---|---|
| `url` | `SafeUrl` | Parsed, normalized, `https` only. `http` only in a debug build when the app sets an explicit flag (H-49). |
| `method` | enum | `GET` only for document sessions. |
| `partition` | `PartitionId` | Required. There is no default partition for authenticated pages. |
| `policy` | `SessionPolicy` | Protocol modes, cache rules, timeouts, limits for this session. |
| `trust` | `TrustPolicy` | Allowed origins, bridge origins, downgrade rules. |
| `capabilities` | `PlatformCapabilities` | What the adapter and WebView can do (streaming, H3, WebSocket…). |
| `clock` | handle | Injected time source (fake in tests). |

## 3. Cache objects

```mermaid
flowchart TD
  Entry[CacheEntry] --> Meta[EntryMeta]
  Entry --> Snap[Snapshot]
  Snap --> Tpl[TemplateBody]
  Snap --> Data[DataBlocks]
  Snap --> Html[RenderedHtml optional]
  Meta --> Val[Validators]
  Meta --> Pol[Freshness policy]
```

| Type | Fields (main) | Notes |
|---|---|---|
| `CacheEntry` | `key`, `meta`, `snapshot_ref` | One logical page in one partition. |
| `EntryMeta` | `created_at`, `last_validated_at`, `expires_at`, `validators`, `policy`, `size`, `schema_version` | Stored in the metadata database. |
| `Validators` | `etag`, `template_tag`, `last_modified`, `rqp_page_revision`, `rqp_template_revision` | legacy and RQP fields are separate. `rqp_page_revision` comes from the manifest, not from the `ETag` header. `rqp_template_revision` is the value that the client computes (H-103). |
| `Snapshot` | `template`, `data`, `rendered_html?`, `integrity` | Immutable once committed. |
| `TemplateBody` | bytes + `template_revision` | Template with placeholders. |
| `DataBlocks` | ordered map `block_id → BlockValue` | Order is significant for re-rendering. |
| `BlockValue` | `Html(bytes)` or `Json(value)` | legacy supports `Html` only. |
| `CacheTransaction` | list of writes + new meta + old snapshot ref | Applied atomically by the adapter's storage driver. |

## 4. Protocol objects

### 4.1 legacy protocol

| Type | Fields | Notes |
|---|---|---|
| `V1Request` | `accept_diff`, `if_none_match`, `template_tag`, `sdk_marker?`, cookies (by adapter) | Headers exactly as specified in [legacy wire contract](/engineering/protocol/legacy-wire). |
| `V1ResponseHead` | `status`, `etag?`, `template_tag?`, `template_change?`, `cache_offline?`, `content_type`, `csp?` | Parsed, not trusted. |
| `V1DataResponse` | `data: map<key, html>`, `template_tag`, `html_sha1?`, `diff?` | Body of a data-only response. |

### 4.2 Rivqen protocol

| Type | Fields | Notes |
|---|---|---|
| `Capabilities` | set of tokens | Sent by the client in `Rq-Capabilities` as one Inner List of Tokens (H-53); `rqp/1` defines `block-patch`. The server does not announce capabilities ([negotiation §3.3](/engineering/protocol/negotiation#_3-3-rq-capabilities)). |
| `PageManifest` | `protocol`, `origin`, `path`, `page_revision`; optional `policy`, `limits` | No block list and no template revision (H-103): the client computes them. See [RQP manifest and patch](/engineering/protocol/manifest-patch). |
| `RqHave` | template revision (member `t`), `digest_prefixes` (member `b`; 8 bytes per block, document order) | Request field `Rq-Have` (H-44, H-106), at most 4096 bytes. Built from the snapshot. |
| `BlockDescriptor` | `id`, `format` (`html`/`json`), `sha256` | Computed by the client, not sent in the manifest (H-103). SHA-256 (base64url) over the exact content bytes; no normalization (H-2, H-11). |
| `PatchEnvelope` | `next_revision`, `operations[]` | No `type`, `protocol`, base or template revision (H-104). The client checks the result through `next_revision`. Validated before apply ([diff engine §3](/engineering/core/diff-engine#_3-rqp-patch-validation)). No `page_id` and no `sequence` over HTTP (H-9). |
| `PatchOp` | `block_id`, `content` | The only operation of `rqp/1`: replace the content of a block (H-47). No `op`, `format` or `sha256` (H-104); the client computes the hash. The client rejects content with script or event handlers ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)). |

## 5. Events and actions

The core is an event-driven state machine. The adapter sends **events**. The core returns **actions**.

| Event | Sent when |
|---|---|
| `WebViewReady` | The WebView can accept a document. |
| `CacheResult(snapshot?)` | The storage driver finished a lookup. |
| `HttpResponseHead(meta)` | Status and headers arrived. |
| `HttpBodyChunk(chunk)` | A body chunk arrived (bounded size). |
| `HttpCompleted` / `HttpFailed(error)` | The body ended or failed. |
| `BridgeReady` | The web SDK in the page connected. |
| `PageRendered` | The page reported first useful render. |
| `NavigationCancelled` | The user or app left the page. |
| `PartitionChanged` | The account changed (logout, switch). |
| `MemoryPressure(level)` | The OS asked to reduce memory. |
| `Tick(now)` | Timer event from the adapter's clock. |

| Action | The adapter must |
|---|---|
| `SendHttpRequest(spec)` | Send this request with the platform stack. |
| `ShowCachedDocument(ref)` | Load this cached document into the WebView. |
| `StreamDocument(token)` | Feed the document stream to the WebView. |
| `ApplyDataPatch(validated)` | Deliver this validated patch to the web SDK. |
| `CommitCache(txn)` | Execute this cache transaction atomically. |
| `InvalidateCache(key)` | Remove this entry. |
| `LoadNormally(url)` | Fall back to a normal WebView load. |
| `EmitMetric(metric)` | Record this metric locally. |
| `ScheduleTimer(deadline)` | Send a `Tick` at this time. |

## 6. Serialization

| Data | Format | Canonical form |
|---|---|---|
| Fixtures | JSON + raw HTTP files | UTF-8, LF line ends, sorted keys in JSON |
| RQP manifest and patch | JSON | JSON Schemas `spec/rqp/schemas/` (WP-17) and the JSON rules of [manifest and patch §3.2](/engineering/protocol/manifest-patch#_3-2-json-text-of-the-manifest-and-the-envelope); hashes over the exact block bytes, not over JSON text |
| Cache metadata | SQLite rows | `schema_version` column; migrations are forward-only |
| FFI payloads | Generated bindings (UniFFI) or C structs | See [FFI boundary](/engineering/core/ffi) |

## Related

- [Core API](/engineering/core/api)
- [Session state machine](/engineering/core/session-fsm)
- [Cache identity](/engineering/cache/identity)
