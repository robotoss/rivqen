# ADR-004: Cache storage format

- Status: **Proposed** (direction: human, 2026-11-15, H-146; the human accepts the ADR with the evidence of WP-07 S2)
- Date: 2026-11-22
- Owner: Architect, WP-07
- Reviewers: architecture lead, security reviewer

**Status:** <Badge type="info" text="DESIGN" /> for the options and the decision. The section "Verification" gets <Badge type="tip" text="FACT" /> lines at the close of WP-07 S2.

[[toc]]

## Context

<Badge type="info" text="DESIGN" /> The L2 cache must survive a process kill and a power loss without a mix of two versions of an entry (INV-14, R-CACHE-05). A logout must remove a partition (R-CACHE-04), and a site clear must remove a site (R-CACHE-09). The core has no I/O (ADR-001): it gives a driver one transaction at a time through `rivqen_cache::driver::StorageDriver`, and each transaction carries its preconditions ([DL-185](/engineering/plan/decision-log), [DL-200](/engineering/plan/decision-log)).

<Badge type="tip" text="FACT" /> Facts from WP-07 S1 ([sprint record](/engineering/plan/sprints/WP-07-S1)): POSIX `rename` is atomic but cannot replace a directory that is not empty; a new directory entry is durable only after an `fsync` of the parent directory; `File::sync_all` on Apple targets is `F_FULLFSYNC`; SQLite in WAL mode with `synchronous=NORMAL` can lose a commit at a power loss and stays consistent, `FULL` is durable. Bundled SQLite adds 1.2 MB for each Android ABI.

## Options

1. **One file for each entry version and SQLite for the metadata** (this ADR).
2. **A directory for each entry with several files** (the first draft of [storage](/engineering/cache/storage) §1). Cons: a directory cannot be replaced in one step; the old directory must be deleted before or after the rename, and both orders have a window with a mix or with no entry.
3. **Files only, no SQLite.** Not chosen by the human (H-146): epochs, versions and the quota list would need an own journal.

## Decision

Option 1, as the crate `rivqen-store` (DL-180), with `rusqlite` =0.42.0 and bundled SQLite.

### 1. Layout

```text
<root>/                      # given by the adapter (app private directory)
└── schema-1/                # SCHEMA_VERSION of rivqen-cache
    ├── meta.sqlite          # metadata (+ -wal, -shm)
    ├── files/<hh>/<file-id>.rqf   # one file for each entry version
    └── tmp/<file-id>.tmp    # files that are not promoted yet
```

- A file id is 16 random bytes, written as 32 lower-case hexadecimal characters. `<hh>` is its first byte. A file name has no partition, no key and no host.
- A file holds the content of one entry version (page, template, data blocks) in the sealed form of [ADR-011](/engineering/architecture/adr/ADR-011-cache-encryption). A file is written once and never changed.
- Another schema version is another directory. The driver removes the directories of other schema versions at recovery.

### 2. Commit procedure

The steps are the values of `rivqen_cache::txn::CommitStep`.

| Step | What the driver does | Durable after the step |
|---|---|---|
| `WriteTemp` | Writes the sealed content to `tmp/<file-id>.tmp` | Nothing |
| `SyncTemp` | `sync_all` on the file | The temporary file |
| `Promote` | Renames it to `files/<hh>/<file-id>.rqf`; `sync_all` on `files/<hh>` and on `tmp` | The file, with no metadata row |
| `CommitMeta` | One SQLite transaction (`BEGIN IMMEDIATE`): checks the preconditions, writes the row with the new version and the file id, removes the rows that the change removes, increments epochs | The change. This is the commit point |
| `RemoveOld` | Unlinks the files that no row names; no sync | Not required: recovery removes them |

- Only `Replace` writes a file. The other kinds do nothing in the first three steps.
- The precondition check is inside the SQLite transaction of `CommitMeta` (DL-200). A refused transaction removes its promoted file.
- `RevokePartition` first asks the key port to delete the key of the partition (ADR-011, crypto-shredding), then runs `CommitMeta`, then `RemoveOld`. A stop between the two leaves rows that no key can open: a read gives `CACHE_CORRUPTED` and removes the entry.
- All calls of one driver are serialized by `&mut self`. A second driver on the same directory is not supported; the adapter opens one.

### 3. Metadata

SQLite settings: `journal_mode=WAL`, `synchronous=FULL`, `foreign_keys=ON`, a busy timeout of 0 (one writer). `FULL` makes an applied revoke or site clear durable: with `NORMAL` a power loss could bring back the entries of a logged-out account.

| Table | Columns | Notes |
|---|---|---|
| `entry` | `partition` (blob: `00` for `public`, or `01` and the 32 id bytes), `key` (blob 32), `host` (text), `version` (integer), `file_id` (blob 16), `content_bytes` (integer), `stored_at`, `last_used_at` (integer), `sealed` (blob) | Primary key (`partition`, `key`). `sealed` holds the protocol, the `Vary` entries, the validators, the freshness inputs and the page fields, sealed as ADR-011 defines |
| `partition_epoch` | `partition`, `epoch` | A row is never deleted |
| `disabled_key` | `partition`, `key`, `until` | Recovery removes the rows whose time is before `now` |
| `counter` | `name`, `value` | `version` (last entry version), `site_epoch`. Never reset |

- The clear columns are what the driver needs without a key: the address, the host for the site clear ([DL-184](/engineering/plan/decision-log)), sizes and times for the quota. Everything else is in `sealed`.
- The driver refuses a `Replace` whose host is not a lower-case ASCII host name or an IP literal (S1 security review L4).
- A decode failure of a row or of `sealed` is `CACHE_CORRUPTED`: the driver removes the entry and keeps every other entry (R-CACHE-06).
- When `meta.sqlite` itself cannot be opened or fails `PRAGMA quick_check`, the driver removes the whole schema directory and starts empty, but keeps the counters when it can read them; when it cannot, the new store starts every epoch at a random 62-bit value, so a writer with an old epoch is always refused (S1 security review R2).

### 4. Recovery

`recover(now)` runs before any other call:

1. Remove the directories of other schema versions.
2. Remove every file in `tmp/`.
3. Remove every file in `files/` that no row names.
4. Remove every row whose file is missing or whose size differs from `content_bytes`.
5. Remove the disabled-key rows whose time is before `now`.

A read checks more than recovery: the seal of the file and of the row, and that the opened row names the partition and the key of the request.

## Consequences

- Easier: an entry version is one rename; old and new never share a path; a site clear and a quota list need no key.
- Harder: `synchronous=FULL` costs one more sync for each commit (measured in the verification). SQLite is C code in the mobile library (+1.2 MB for each ABI, RSK-10); the driver stays behind the trait, so a driver without SQLite stays possible.
- Not covered: a power cut on a real device (the tests model it), secure delete on flash storage (ADR-011 uses key deletion), two processes on one directory.

## Verification

<Badge type="danger" text="GATE" /> Evidence to collect in WP-07 S2:

| Check | Evidence |
|---|---|
| Old or new at every step, for every transaction kind | The driver contract suite of `rivqen-testkit` on the driver (in-process stop after each step) |
| Kill at every step | A child process that the parent stops with `SIGKILL` at each step; then recovery and the old-or-new check |
| A missing sync is found | A review of each sync against section 2, and the power-loss model of the fake (a `SIGKILL` test cannot find it) |
| Partitions never mix; revoke; site clear | The suite and the multi-partition tests on the driver |
| Corruption | Damaged file, damaged row, missing file, moved file: `CACHE_CORRUPTED`, other entries unchanged |
| Cost | Time of one commit with `FULL` on the host; size of the store for N entries |

## Related

- [Storage and memory](/engineering/cache/storage)
- [ADR-011: Cache encryption](/engineering/architecture/adr/ADR-011-cache-encryption)
- [ADR-001: Core boundaries](/engineering/architecture/adr/ADR-001-core-boundaries)
