# ADR-004: Cache storage format

- Status: **Proposed** (direction: human, 2026-11-15, H-146; the human accepts the ADR with the evidence of WP-07 S2)
- Date: 2026-11-22
- Owner: Architect, WP-07
- Reviewers: architecture lead, security reviewer

**Status:** <Badge type="info" text="DESIGN" /> for the options and the decision. The section "Verification" has the <Badge type="tip" text="FACT" /> evidence of WP-07 S2.

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
- Before the first step the driver checks that the content and the row of a `Replace` decode with its limits and that the sealed row fits the `sealed` column ([DL-211](/engineering/plan/decision-log), DL-213). `UpdateFields` checks its new row inside its transaction. A failed check is `STORAGE_UNAVAILABLE` and writes nothing.
- The precondition check is inside the SQLite transaction of `CommitMeta` (DL-200). A refused transaction removes its promoted file. A transaction that fails with a SQLite error keeps the file: SQLite can replay a commit at the next open, and recovery step 3 removes the file when no row names it.
- `UpdateFields` on a row that does not open removes the row in the same transaction and gives `Refused(Entry)`.
- `RevokePartition` first asks the key port to delete the key of the partition (ADR-011, crypto-shredding), then runs `CommitMeta`, then `RemoveOld`. `ClearSite` of all sites does the same with the key of `public`. When the key port fails, the driver still runs `CommitMeta` and `RemoveOld`: the rows go and the epoch grows. Then the call gives `STORAGE_UNAVAILABLE`, and the caller runs it again; the second run deletes the key. When the key is deleted and `CommitMeta` fails, the call gives `STORAGE_UNAVAILABLE` and the caller must run it again; until then the old rows stay, but no key opens them: a read gives `CACHE_CORRUPTED` and removes the entry.
- After a transaction removes a row, a failed unlink of its file is not an error of the call: the file is an orphan for recovery.
- On Unix the driver creates its directories with mode 0700 and its files, also `meta.sqlite`, with mode 0600.
- All calls of one driver are serialized by `&mut self`. A second driver on the same directory is not supported; the adapter opens one.

### 3. Metadata

SQLite settings: `journal_mode=WAL`, `synchronous=FULL`, `fullfsync=ON`, `checkpoint_fullfsync=ON`, `foreign_keys=ON`, `trusted_schema=OFF`, `cell_size_check=ON`, a busy timeout of 0 (one writer). The driver reads the settings back and does not open the store when one differs. On Apple targets SQLite syncs with `F_FULLFSYNC` only when `fullfsync` is on; on other platforms the two flags change nothing ([DL-209](/engineering/plan/decision-log)). `FULL` makes an applied revoke or site clear durable: with `NORMAL` a power loss could bring back the entries of a logged-out account.

| Table | Columns | Notes |
|---|---|---|
| `entry` | `partition` (blob: `00` for `public`, or `01` and the 32 id bytes), `key` (blob 32), `host` (text), `version` (integer), `file_id` (blob 16), `content_bytes` (integer), `stored_at`, `last_used_at` (integer), `sealed` (blob) | Primary key (`partition`, `key`). `sealed` holds the host, the store time, the protocol, the `Vary` entries, the validators, the freshness inputs and the page fields, sealed as ADR-011 defines |
| `partition_epoch` | `partition`, `epoch` | A row is never deleted |
| `disabled_key` | `partition`, `key`, `until` | Recovery removes the rows whose time is before `now` |
| `counter` | `name`, `value` | `version` (last entry version), `site_epoch`, `partition_epoch_base` (the epoch of a partition that has no row). Never reset while the database is kept |

- The clear columns are what the driver needs without a key: the address, the host for the site clear ([DL-184](/engineering/plan/decision-log)), sizes and times for the quota. Everything else is in `sealed`.
- Every open of a row (read, list, update) compares the host and the store time in `sealed` with the clear columns. A difference is `CACHE_CORRUPTED`. `content_bytes` is compared with the decoded file at a read. `version` and `last_used_at` are not authenticated (DL-214).
- The driver refuses a `Replace` whose host is not a lower-case ASCII host name or an IP literal (S1 security review L4).
- A decode failure of a row or of `sealed` is `CACHE_CORRUPTED`: the driver removes the entry and keeps every other entry (R-CACHE-06).
- When `meta.sqlite` is not a database, fails `PRAGMA quick_check` or has an unknown format, the driver removes the content of the schema directory and starts empty. No counter of the failed database is kept: the version counter, the site epoch and the base epoch of the partitions start at random 62-bit values, so a writer with an old epoch is always refused (S1 security review R2, DL-209). A marker file (`meta.reset`) makes a reset that did not finish run again. A busy database or an I/O error is `STORAGE_UNAVAILABLE` and never starts a reset.
- At open, `sqlite_schema` must hold exactly the four tables, with the text of their `CREATE TABLE` statements, and the automatic index of each primary key. Any other object (a trigger, a view, an index, a table) is handled as a database that fails its check. An epoch write or a counter write that does not change exactly one row is `STORAGE_UNAVAILABLE`, and nothing is committed.
- When `meta.sqlite` does not exist or has no tables, and the schema directory holds anything other than an empty `tmp/` and empty directories in `files/`, the driver takes the reset path with random counters. Only a fresh directory starts at 0.
- The reset marker is created with `create_new`. An existing marker that is not a regular file is `STORAGE_UNAVAILABLE`; it is never followed or truncated.
- A `sealed` column is at most 1 MiB. A value above the SQLite integer range is refused; a time above it is stored as the largest value.

### 4. Recovery

`recover(now)` runs before any other call:

1. Remove the directories of other schema versions.
2. Remove every file in `tmp/`.
3. Remove every file in `files/` that no row names.
4. Remove every row whose file is missing or has a length that no file with `content_bytes` of content can have. `content_bytes` is the size of the plain content, so a list needs no file read ([DL-212](/engineering/plan/decision-log)).
5. Remove the disabled-key rows whose time is before `now`.

A read checks more than recovery: the seal of the file and of the row, and that the opened row names the partition and the key of the request.

## Consequences

- Easier: an entry version is one rename; old and new never share a path; a site clear and a quota list need no key.
- Harder: `synchronous=FULL` costs one more sync for each commit (measured in the verification). SQLite is C code in the mobile library (+1.2 MB for each ABI, RSK-10); the driver stays behind the trait, so a driver without SQLite stays possible.
- Not covered: a power cut on a real device (the tests model it), secure delete on flash storage (ADR-011 uses key deletion), two processes on one directory.

## Verification

<Badge type="tip" text="FACT" /> Evidence of WP-07 S2 (2026-11-22, [sprint record](/engineering/plan/sprints/WP-07-S2) §4):

| Check | Evidence |
|---|---|
| Old or new at every step, for every transaction kind | The driver contract suite of `rivqen-testkit` passes on `FileStore` (`crates/rivqen-store/tests/driver_contract.rs`, 25 tests, in-process stop after each step) |
| Kill at every step | `driver_kill.rs`: 10 transaction kinds x 5 steps. A child process stops after the step, the parent ends it with `SIGKILL` (signal 9 checked), recovers and finds exactly the old or the new state; a second recovery removes nothing |
| A missing sync is found | Review of each sync against section 2 (security review, 2026-11-22): all present. A `SIGKILL` test cannot find a missing sync; the power-loss model of the fake covers the contract |
| Partitions never mix; revoke; site clear | The suite on the driver; `driver_partitions_*`, `driver_revoke_shreds`, `driver_commit_revoke_with_a_failing_key_port_still_revokes` |
| Corruption | Damaged file, damaged row, row of another key, missing file, moved file, file of another length, host or store time that differs: `CACHE_CORRUPTED`, the entry is removed, other entries unchanged (`driver_read_*`) |
| Cost | Host, release build, external SSD, median of 50: a `Replace` of 100 KiB 9.95 ms, a read 0.18 ms, a metadata commit 1.9 ms (0.33 ms without `fullfsync`). 50 entries of 100 KiB take 5.12 MB of files |
| Mutation | `rivqen-store::driver` 169 of 200, `meta` 160 of 179 |

<Badge type="danger" text="GATE" /> Not covered: a power cut on a real device and the cost on devices (WP-19).

## Related

- [Storage and memory](/engineering/cache/storage)
- [ADR-011: Cache encryption](/engineering/architecture/adr/ADR-011-cache-encryption)
- [ADR-001: Core boundaries](/engineering/architecture/adr/ADR-001-core-boundaries)
