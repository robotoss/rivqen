# Resource limits

Every input to the core is bounded. Limits protect the app from memory exhaustion, slow streams, decompression bombs and pathological HTML.

**Status:** <Badge type="info" text="DESIGN" /> Values are **initial test values**. WP-19 benchmarks set the defaults per device class. They are not promises.

## 1. Limits table

| Limit | Initial value | Applies to | On violation |
|---|---|---|---|
| `maxDocumentBytes` | 5 MiB (decoded) | Main HTML document | `DOCUMENT_TOO_LARGE` → fallback |
| `maxCompressedRatio` | 50 : 1 | gzip / br / zstd decoding | Abort decoding → fallback |
| `prefixBufferBytes` | 512 KiB | Bridge stream prefix | Backpressure (stop reading) |
| `maxChunkBytes` | 64 KiB | One stream chunk | Split chunk |
| `maxBlocks` | 256 | Data blocks per page | Protocol error |
| `maxBlockKeyBytes` | 128 | Block key length | Protocol error |
| `maxBlockBytes` | 1 MiB | One data block | Protocol error |
| `maxPatchBytes` | 256 KiB | One RQP patch envelope | `PATCH_TOO_LARGE` |
| `maxJsonDepth` | 16 | legacy data JSON, RQP envelopes | Protocol error |
| `maxHeaderBytes` | 16 KiB per header, 64 KiB total | Response headers parsed by the core | Protocol error |
| `maxActiveSessions` | 8 per engine | Concurrent sessions | Oldest background session is cancelled |
| `maxConcurrentPrefetch` | 2 | Prefetch requests | Queue |
| `timeoutConnect` | 5 s | Connection | Fallback |
| `timeoutFirstByte` | 10 s | Time to first byte | Fallback |
| `timeoutTotal` | 30 s | Whole document | Fallback |
| `memoryL1Bytes` | 8–32 MiB (device class) | In-memory snapshots | Evict LRU |
| `diskL2Bytes` | 64–256 MiB (device class) | On-disk snapshots | Evict LRU per partition |

## 2. Device classes

| Class | Typical RAM | L1 | L2 |
|---|---|---|---|
| Low | ≤ 3 GB | 8 MiB | 64 MiB |
| Mid | 4–6 GB | 16 MiB | 128 MiB |
| High | ≥ 8 GB | 32 MiB | 256 MiB |

On Android the adapter uses `ActivityManager.isLowRamDevice()` and `ComponentCallbacks2.onTrimMemory`. On iOS it uses memory warnings and `os_proc_available_memory()`.

## 3. Rules

1. Every limit is configurable by the host app within a safe range. The core rejects values outside the range.
2. A limit violation is never a crash. It is a typed error and a fallback.
3. Fuzz tests run with the limits at their configured values and assert bounded memory.

## Related

- [Errors](/engineering/core/errors)
- [Storage and memory](/engineering/cache/storage)
- [Security control SEC-07, SEC-23](/engineering/security/controls)
