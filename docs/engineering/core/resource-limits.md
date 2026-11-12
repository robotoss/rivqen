# Resource limits

Every input to the core is bounded. Limits protect the app from memory exhaustion, slow streams, decompression bombs and pathological HTML.

**Status:** <Badge type="info" text="DESIGN" /> Values are **initial test values**. WP-19 benchmarks set the defaults per device class. They are not promises.

## 1. Limits table

| Limit | Initial value | Applies to | On violation |
|---|---|---|---|
| `maxDocumentBytes` | 5 MiB (decoded) | Main HTML document; the RQP document after a patch ([manifest and patch §2.4](/engineering/protocol/manifest-patch#_2-4-client-verification-and-apply), step 8) | `DOCUMENT_TOO_LARGE` → fallback; after a patch `PATCH_TOO_LARGE` |
| `maxCompressedRatio` | 50 : 1 | gzip / br / zstd decoding | Abort decoding → fallback |
| `prefixBufferBytes` | 512 KiB | Bridge stream prefix | Backpressure (stop reading) |
| `maxChunkBytes` | 64 KiB | One stream chunk | Split chunk |
| `maxBlocks` | 256 | Data blocks per page | Protocol error |
| `maxBlockKeyBytes` | 128 (legacy mode); 64 in RQP ([markup](/engineering/protocol/markup) M-15) | Block key length | Protocol error |
| `maxBlockBytes` | 1 MiB | One data block | Protocol error |
| `maxManifestBytes` | 64 KiB (H-50) | Text of the RQP page manifest ([manifest and patch](/engineering/protocol/manifest-patch)); a compact manifest is at most about 4,819 bytes (FACT, WP-17 S6 T-02a; H-103); the limit stays 64 KiB because JSON white space is allowed | `MANIFEST_INVALID` → normal load |
| `maxOpenElements` | 256 | Depth counter D of the RQP markup analysis ([markup](/engineering/protocol/markup) M-26, H-25, H-31); it also bounds the token stack (size ≤ D) and the WHATWG stack (≤ D + 2) | `RQP_MARKUP_LIMIT` → normal HTML without RQP |
| `maxPatchBytes` | 256 KiB | One RQP patch envelope. The server sends the full document when a delta is larger ([negotiation §4](/engineering/protocol/negotiation#_4-response-selection-server), step 7) | Client: `PATCH_TOO_LARGE`. Server: full document |
| `maxRqHaveBytes` | 4096 bytes, fixed by `rqp/1` | Request field `Rq-Have`, all field lines combined (H-44) | Client: never sends more. Server: field is absent, not parsed |
| `maxRqCapabilitiesBytes` | 1024 bytes, fixed by `rqp/1` | Request field `Rq-Capabilities`, all field lines combined | Server: field is absent, not parsed |
| `maxRqNonceBytes` | 128 bytes, fixed by `rqp/1` | Request field `Rq-Nonce`, the field value without leading and trailing white space (DL-063) | Server: no nonce, not parsed (the full document F) |
| `maxJsonDepth` | 16 | legacy data JSON, RQP envelopes | Protocol error |
| `maxHeaderBytes` | 16 KiB per header, 64 KiB total | Response field lines that the adapter gives to the core. Size of a line = name bytes + value bytes (no separator, no line end); a size equal to a limit passes. `ResponseHead::check` tests all lines before any field read; the response field reads are methods of the checked head (**FACT**, WP-05 S2 T-03) | `DOCUMENT_TOO_LARGE` for the whole head; no RQP field is read; normal load (never drop one field: [negotiation §6.3](/engineering/protocol/negotiation#_6-3-response) item 1; [DL-153](/engineering/plan/decision-log)) |
| `maxActiveSessions` | 8 per engine | Concurrent sessions | Oldest background session is cancelled |
| `maxConcurrentPrefetch` | 2 | Prefetch requests | Queue |
| `timeoutConnect` | 5 s | Connection | Fallback |
| `timeoutFirstByte` | 10 s | Time to first byte | Fallback |
| `timeoutTotal` | 30 s | Whole document | Fallback |
| `memoryL1Bytes` | 8–32 MiB (device class) | In-memory snapshots | Evict LRU |
| `diskL2Bytes` | 64–256 MiB (device class) | On-disk snapshots | Evict LRU per partition |

### 1.1 Safe ranges

<Badge type="tip" text="FACT" /> `rivqen_model::limits::Limit::range` holds one range (minimum and maximum, both allowed) per configurable limit. `Engine::new` in `rivqen-core` rejects a value outside its range (tests `core_config_out_of_range_rejected`, `core_config_both_ends_of_each_range_pass`).

<Badge type="info" text="DESIGN" /> The values come from WP-05 S2 (T-04) and are recorded in [DL-155](/engineering/plan/decision-log). "Proposal" in the reason column means that no document gives the number; WP-19 measurements can change it.

<Badge type="tip" text="FACT" /> In WP-05 the checked values are not used yet: the parsers of `rivqen-proto` use their own constants (`MAX_JSON_DEPTH`, `MAX_PATCH_BYTES` as the default, `MAX_BLOCK_BYTES` as a cap) and take the client limits as parameters. The session (WP-08) passes the values of the checked `EngineConfig` to them; a test pins the defaults to the constants (`limits_consistency.rs`).

| Limit | Min | Max | Reason |
|---|---|---|---|
| `maxDocumentBytes` | 1 MiB | 32 MiB | Min: one block of `maxBlockBytes` fits. Max: the largest L1 budget (§2); a larger document fits no memory cache. Proposal |
| `maxCompressedRatio` | 10 | 100 | Proposal. `maxDocumentBytes` also bounds the decoded size; the ratio only stops a bomb early |
| `prefixBufferBytes` | 64 KiB | 2 MiB | Min: one chunk of the largest `maxChunkBytes`. Max: proposal (16 sessions × 2 MiB stay at 32 MiB) |
| `maxChunkBytes` | 8 KiB | 64 KiB | Chunks are 8 to 64 KiB ([streaming](/engineering/core/streaming), ADR-009) |
| `maxBlocks` | 256 | 256 | Fixed: [markup](/engineering/protocol/markup) M-26 |
| `maxBlockKeyBytes` (legacy) | 128 | 1024 | Min: a lower value rejects keys that the default accepts. Max: proposal |
| `maxBlockKeyBytes` (RQP) | 64 | 64 | Fixed: M-15 block id grammar |
| `maxBlockBytes` | 1 MiB | 1 MiB | Fixed: M-26 |
| `maxManifestBytes` | 64 KiB | 64 KiB | Fixed: [manifest and patch](/engineering/protocol/manifest-patch) §1.8, item 1 (H-50) |
| `maxOpenElements` | 256 | 256 | Fixed: M-26 |
| `maxPatchBytes` | 1 KiB | 5 MiB | The range of the manifest hint `max_patch_bytes` (manifest and patch §1.5) |
| `maxJsonDepth` | 3 | 16 | Min: valid manifests and envelopes have depth 3 or less (manifest and patch §3.2, item 5). Max: the tree cap of DL-152 |
| `maxHeaderBytes` (line) | 8 KiB | 64 KiB | Min: a head that passes a default reverse proxy (4 or 8 KiB, DL-153) passes. Max: proposal |
| `maxHeaderBytes` (total) | 8 KiB | 256 KiB | Min: as per line. Max: proposal |
| `maxActiveSessions` | 1 | 16 | Min: one session. Max: proposal (stream memory per session, [streaming](/engineering/core/streaming) §5) |
| `maxConcurrentPrefetch` | 1 | 8 | Min: 0 would queue prefetch for ever. Max: proposal |
| `timeoutConnect` | 1 s | 30 s | Proposal; max: the default `timeoutTotal` |
| `timeoutFirstByte` | 1 s | 60 s | Proposal |
| `timeoutTotal` | 1 s | 120 s | Proposal; a finite max keeps "never an endless wait" (INV-05) |
| `memoryL1Bytes` | 8 MiB | 32 MiB | The device classes of §2 |
| `diskL2Bytes` | 64 MiB | 256 MiB | The device classes of §2 |

Pairs: a valid configuration also has `maxPatchBytes` ≤ `maxDocumentBytes` (manifest and patch §1.5: a patch larger than a full document has no use), `maxHeaderBytes` (line) ≤ `maxHeaderBytes` (total), and `timeoutConnect`, `timeoutFirstByte` ≤ `timeoutTotal` (`Limit::NOT_ABOVE`). The ranges alone give `maxChunkBytes` ≤ `prefixBufferBytes` and `maxBlockBytes`, `maxManifestBytes` ≤ `maxDocumentBytes`.

A value outside its range, or a pair in the wrong order, gives `CONFIG_INVALID` in operation `engine_new` (H-141, DL-158): a configuration outside the safe range is a bug of the host app. The cause is a `ConfigError` with the limit name and the rule. It never holds the value. The check runs the limits in the order of the table, then the pairs; the first failure wins.

## 2. Device classes

| Class | Typical RAM | L1 | L2 |
|---|---|---|---|
| Low | ≤ 3 GB | 8 MiB | 64 MiB |
| Mid | 4–6 GB | 16 MiB | 128 MiB |
| High | ≥ 8 GB | 32 MiB | 256 MiB |

On Android the adapter uses `ActivityManager.isLowRamDevice()` and `ComponentCallbacks2.onTrimMemory`. On iOS it uses memory warnings and `os_proc_available_memory()`.

## 3. Rules

1. Every limit is configurable by the host app within a safe range (§1.1). The core rejects values outside the range. Exception: a limit marked "fixed by `rqp/1`" is part of the protocol and is not configurable, because a different value on one side changes the result for valid requests. <Badge type="tip" text="FACT" /> `maxRqHaveBytes`, `maxRqCapabilitiesBytes` and `maxRqNonceBytes` are constants of `rivqen-proto` and are not in `ResourceLimits`. <Badge type="info" text="DESIGN" /> A limit that a MUST rule of the RQP markup or manifest fixes (M-15, M-26, manifest and patch §1.8) has a range of one value (§1.1), for the same reason.
2. A limit violation is never a crash. It is a typed error and a fallback.
3. Fuzz tests run with the limits at their configured values and assert bounded memory.

## Related

- [Errors](/engineering/core/errors)
- [Storage and memory](/engineering/cache/storage)
- [Security control SEC-07, SEC-23](/engineering/security/controls)
