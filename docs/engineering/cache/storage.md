# Storage and memory

This page defines how cache data is written to disk, how it is protected, and how Rivqen reacts to memory pressure.

**Status:** <Badge type="info" text="DESIGN" /> ADR-004 (format) and ADR-011 (encryption) are proposed; their direction is decided (H-146, H-147, [WP-07 S1](/engineering/plan/sprints/WP-07-S1)). The exact layout and write procedure are in [ADR-004](/engineering/architecture/adr/ADR-004-cache-storage) and the sealed form in [ADR-011](/engineering/architecture/adr/ADR-011-cache-encryption) (both Proposed, WP-07 S2); sections 1 and 2 below are the first draft and change when the ADRs are accepted.

[[toc]]

## 1. Disk layout

```text
<app private dir>/rivqen/
├── schema-1/                   # cache schema version
│   ├── meta.sqlite             # metadata, WAL mode
│   └── p/<partition-hex>/      # one directory per partition
│       └── <cache-key-hex>/
│           ├── template.bin    # encrypted if authenticated
│           ├── data.bin
│           ├── page.bin        # optional rendered HTML
│           └── manifest.json   # revisions, sizes, hashes
└── tmp/                        # incomplete writes, cleaned at start
```

| Platform | Base directory | Backup |
|---|---|---|
| Android | `context.noBackupFilesDir` (or `cacheDir` for public entries) | Excluded from auto backup |
| iOS | `Library/Caches/` (public) or `Library/Application Support/` with `isExcludedFromBackup` (private) | Excluded from iCloud backup |

## 2. Atomic write procedure

1. Write each new file to `tmp/` with a random name.
2. Call `fsync` on each file if the entry needs durability.
3. Rename the files into a new entry directory.
4. Commit the metadata row in one SQLite transaction.
5. Delete the old entry directory after the commit.
6. At startup, delete everything in `tmp/` and every directory that has no metadata row.

<Badge type="danger" text="GATE" /> Fault-injection tests kill the process after each step. The entry must be the old or the new version, never a mix.

## 3. Encryption

| Item | Design |
|---|---|
| Scope | Every entry: scoped entries with the key of their partition, `public` entries with the key of `public` (H-151, [ADR-011](/engineering/architecture/adr/ADR-011-cache-encryption)) |
| Algorithm | AEAD: XChaCha20-Poly1305 from the crate `chacha20poly1305` (H-147, ADR-011) |
| Keys | One data key per partition. The data key is wrapped by a platform key (Android Keystore / iOS Keychain with `ThisDeviceOnly`). |
| Nonce | Random 192-bit per file (H-147). The 192-bit nonce needs no per-key counter |
| Associated data | `cache_key ‖ file role ‖ schema_version` (prevents moving files between entries) |
| Logout | Delete the partition key first (crypto-shredding), then delete files |

::: warning No "secure delete" promise
Flash storage uses wear leveling. Overwriting a file does not guarantee that old bytes are gone. Rivqen relies on **key deletion**, not on overwriting.
:::

## 4. Memory management

| Event | Android | iOS | Core action |
|---|---|---|---|
| Moderate pressure | `onTrimMemory(TRIM_MEMORY_RUNNING_LOW)` / `BACKGROUND` | Memory warning | Halve L1; stop prefetch |
| Critical pressure | `TRIM_MEMORY_RUNNING_CRITICAL` / `COMPLETE` | Memory warning while backgrounded | Clear L1; cancel background sessions |
| Process kill | Process death | Jetsam | Nothing to do; L2 is consistent by design |

<Badge type="tip" text="FACT" /> `rivqen_cache::memory::L1Cache` keeps its values within a byte budget: `get` marks a value as the most recently used, and `insert` evicts the least recently used values until the new one fits. A value larger than the budget gives `OverBudget`; an older value of the same key is removed, and no other value changes. On `Moderate` pressure it evicts the least recently used values until at most half of the budget (rounded down) is in use; on `Critical` it clears. `rivqen_cache::quota::plan_eviction` names the L2 entries to remove for a new entry: the least recently used entries of the partition of the writer first, then of the other partitions, in the order of last use and then of the key. `rivqen_cache::inflight::InFlight` gives one leader for each request partition and key (R-CACHE-07); it never uses `public` as a key, so two profiles never share one network request (DL-202). The signals of the platforms and the wiring to sessions are WP-08, WP-10 and WP-11.

## 5. Library candidates

| Need | Candidate | Note |
|---|---|---|
| Ref-counted byte chunks | `bytes` | Copies may still happen at the FFI boundary |
| Small vectors | `smallvec` | Only if profiles show benefit |
| Scoped arenas | `bumpalo` | Never keep arena references beyond one operation |
| Concurrent memory cache | `moka` | Compare with a simple sharded LRU and binary size |
| Metadata | `rusqlite` with bundled SQLite | Atomic transactions, WAL |
| Memory-mapped reads | `memmap2` | Risky with concurrent invalidation; needs a platform audit |
| Allocator | System allocator | Do not replace the global allocator inside an SDK |

Measure `allocated_bytes`, `peak_live_bytes`, `ffi_copy_bytes`, `retained_cache_bytes`, allocations per page, RSS/PSS and cleanup time. The goal is less memory **without** loss of correctness, not the largest number of libraries.

## Related

- [Dependencies](/engineering/core/dependencies)
- [Cryptography](/engineering/security/crypto)
