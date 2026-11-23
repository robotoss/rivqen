# Storage and memory

This page defines how cache data is written to disk, how it is protected, and how Rivqen reacts to memory pressure.

**Status:** <Badge type="tip" text="FACT" /> for sections 1 to 3 (the reference driver of WP-07 S2) and for the items marked in section 4; <Badge type="info" text="DESIGN" /> for the platform directories, the key wrap and the memory signals. [ADR-004](/engineering/architecture/adr/ADR-004-cache-storage) (format) and [ADR-011](/engineering/architecture/adr/ADR-011-cache-encryption) (encryption) are Accepted (H-152, H-153, [WP-07 S2](/engineering/plan/sprints/WP-07-S2)).

[[toc]]

## 1. Disk layout

<Badge type="tip" text="FACT" /> The reference driver `rivqen_store::driver::FileStore` (crate `rivqen-store`, WP-07 S2) uses the layout of [ADR-004](/engineering/architecture/adr/ADR-004-cache-storage) §1:

```text
<root>/                      # given by the adapter; only the store uses it
└── schema-1/                # cache schema version
    ├── meta.sqlite          # metadata, WAL mode (+ -wal, -shm)
    ├── files/<hh>/<file-id>.rqf   # one sealed file per entry version
    └── tmp/<file-id>.tmp    # files that are not promoted yet
```

A file id is 16 random bytes. A file name has no partition, no key and no host. On Unix the directories have mode 0700 and the files mode 0600.

<Badge type="info" text="DESIGN" /> The base directory on each platform (WP-10, WP-11):

| Platform | Base directory | Backup |
|---|---|---|
| Android | `context.noBackupFilesDir` | Excluded from auto backup |
| iOS | `Library/Application Support/` with `isExcludedFromBackup` | Excluded from iCloud backup |

## 2. Atomic write procedure

<Badge type="tip" text="FACT" /> The driver commits in the five steps of `rivqen_cache::txn::CommitStep` (ADR-004 §2):

1. `WriteTemp`: write the sealed content to `tmp/`.
2. `SyncTemp`: sync the file.
3. `Promote`: rename it to `files/<hh>/` and sync both directories.
4. `CommitMeta`: check the preconditions and write the row in one SQLite transaction. This is the commit point.
5. `RemoveOld`: unlink the files that no row names.

At start, `recover` removes the directories of other schema versions, every file in `tmp/`, every file that no row names, every row whose file is missing or has a wrong length, and the expired disabled-key records.

<Badge type="tip" text="FACT" /> Fault tests: the driver contract suite stops the driver after each step, and `crates/rivqen-store/tests/driver_kill.rs` ends a child process with `SIGKILL` after each step of each transaction kind. The entry is the old or the new version, never a mix ([testing](/engineering/quality/testing)). Limit: a host test cannot cut the power; the syncs are checked by review.

## 3. Encryption

<Badge type="tip" text="FACT" /> Implemented in `rivqen_store::crypto` as [ADR-011](/engineering/architecture/adr/ADR-011-cache-encryption) defines. The key wrap on the platforms is design (WP-10, WP-11).

| Item | Design |
|---|---|
| Scope | Every entry: scoped entries with the key of their partition, `public` entries with the key of `public` (H-151) |
| Algorithm | AEAD: XChaCha20-Poly1305 from the crate `chacha20poly1305` (H-147) |
| Keys | One data key per partition, from the key port (`KeyProvider`). On a device the data key is wrapped by a platform key (Android Keystore / iOS Keychain with `ThisDeviceOnly`) |
| Nonce | Random 192-bit per seal, from the key port |
| Associated data | Role (`file` or `row`), schema version, partition, cache key and file id, each with its length: a file or a row that is moved does not open |
| Sealed metadata | The host, the store time, the protocol, the `Vary` entries, the validators, the freshness inputs and the page fields of a row |
| Logout | Delete the partition key first (crypto-shredding), then remove the rows, then the files. When the key store is locked, the rows still go and the epoch grows; the caller runs the revoke again |

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
