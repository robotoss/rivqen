# ADR-011: Cache encryption

- Status: **Accepted** (human, 2026-11-23, H-153, with the evidence of [WP-07 S2](/engineering/plan/sprints/WP-07-S2); WP-18 checks it against the threat model and can open it again. Direction: H-146, H-147, H-151)
- Date: 2026-11-22
- Owner: Architect, WP-07
- Reviewers: security reviewer

**Status:** <Badge type="info" text="DESIGN" /> for the options and the decision. The section "Verification" has the <Badge type="tip" text="FACT" /> evidence of WP-07 S2.

[[toc]]

## Context

<Badge type="info" text="DESIGN" /> The L2 cache holds pages of the user on the device. R-CACHE-03 and SEC-19 require encryption of authenticated content, and a logout must make the content of a partition unreadable also when its files are still on the flash storage (R-CACHE-04).

<Badge type="tip" text="FACT" /> Facts from WP-07 S1 ([sprint record](/engineering/plan/sprints/WP-07-S1)): `chacha20poly1305` 0.11.0 is a stable release with one audit of its family (NCC Group, 2020); it adds 7 KB to the Android library. The S1 security review (L3) found that a credentialed response with `Cache-Control: public` can go to the `public` partition, which the first draft did not encrypt.

## Options

1. **XChaCha20-Poly1305, every entry** (this ADR; H-147, H-151).
2. AES-256-GCM. Not chosen by the human (H-147): a random 96-bit nonce has a limit of uses for each key.
3. Encrypt only entries of authenticated partitions (the first draft of [storage](/engineering/cache/storage) §3). Not chosen by the human (H-151): two code paths, and the gap of review finding L3.

## Decision

### 1. Scope

The driver seals every entry: the content file and the `sealed` column of its metadata row ([ADR-004](/engineering/architecture/adr/ADR-004-cache-storage) §3). No entry is stored in clear text.

### 2. Keys

| Key | Use | Source |
|---|---|---|
| Data key of a scoped partition | Entries of that partition | The key port, by `PartitionId` |
| Data key of `public` | Entries of the `public` partition | The key port |

- A data key is 32 random bytes. The key port creates it at the first use and keeps it; on a device the platform adapter wraps it with a key of the Android Keystore or the iOS Keychain (`ThisDeviceOnly`) (WP-10, WP-11). WP-07 has a host key provider for tests only (`HostKeys`, behind the feature `test-support` of `rivqen-store`, H-154).
- The driver holds a key only for the time of one call, in a buffer that is set to zero on drop (`zeroize`); the cipher crate has its `zeroize` feature on, so its own copy of the key is set to zero too ([DL-210](/engineering/plan/decision-log)). `Debug` of a key prints no byte.
- The key port also gives the random bytes for nonces and file ids. The driver has no random source of its own, so no new dependency.

Key port (Rust trait of `rivqen-store`; the platform form is WP-09):

| Call | Result |
|---|---|
| `data_key(partition)` | The key, created when it does not exist; an error is `STORAGE_UNAVAILABLE` |
| `delete_key(partition)` | The key is gone; a later `data_key` gives a new key |
| `random(buffer)` | Random bytes from the system source |

### 3. Sealed form

One format for a file and for the `sealed` column:

```text
"RQS1" ‖ nonce (24 bytes) ‖ ciphertext ‖ tag (16 bytes)
```

- Algorithm: XChaCha20-Poly1305 (`chacha20poly1305` =0.11.0). The nonce is 24 random bytes for each seal.
- Associated data, each part with a length of 8 bytes, big-endian, as in the cache key ([cache identity](/engineering/cache/identity) §1): `rivqen-seal-v1`, the role (`file` or `row`), the schema version (4 bytes, big-endian), the partition field of the cache key (`02`, or `01` and the id), the cache key, the file id.
- So a file or a row that is moved to another partition, key, role or entry version does not open. The entry version is bound through the file id: each `Replace` has a new file id, and `UpdateFields` seals the row again with the same file id.
- Plain text of a file: a format byte, the page, the template (absent or present) and the data blocks, each with a length.
- Plain text of a row (format 2): a format byte; the host (at most 253 bytes); the store time; the protocol; the `Vary` entries; the validators; the freshness inputs (the flags `no_store`, `no_cache`, `private`, `valid_private`, `public`, `must_revalidate`, then the numbers); the page fields. Only format 2 decodes, and each value has one encoding. The host and the store time are copies of the clear columns of ADR-004 §3: the seal authenticates them, and the driver compares them at every open of a row ([DL-213](/engineering/plan/decision-log)).
- Both decoders read untrusted bytes: bounded, no panic, with fuzz targets. The driver stores nothing that its own decoders refuse.
- Any failure to open (wrong magic, short input, tag mismatch, decode error) is `CACHE_CORRUPTED`. The error names no reason that helps an attacker and holds no byte of the data.

### 4. Logout and site clear

- `RevokePartition`: the driver asks the key port to delete the key first, then commits the removal, then unlinks the files (ADR-004 §2). After the key is gone, files that a power loss brings back cannot be opened.
- When the key port fails (a locked key store), the driver still removes the rows and increments the epoch, and gives `STORAGE_UNAVAILABLE`: the caller runs the revoke again to delete the key. Until then the unlinked files could come back at a power loss with a key that still exists; no row names them, so recovery removes them.
- The `public` key is deleted when the app turns the `public` partition off or clears all sites.
- A site clear of one domain removes rows and files; it does not change a key.

## Consequences

- Easier: one code path; the metadata of a row beyond the address, the host, sizes and times is not readable without the key.
- Harder: every read needs the key port, so a locked key store makes the cache unavailable (`STORAGE_UNAVAILABLE`, normal load); the host name of an entry stays in clear text (DL-184).
- Not covered: an attacker with the unlocked device and the app's own access to the key store; secure delete on flash storage (the design relies on key deletion); the key wrap on the platforms (WP-10, WP-11); the review against the threat model (WP-18).

## Verification

<Badge type="tip" text="FACT" /> Evidence of WP-07 S2 (2026-11-22, [sprint record](/engineering/plan/sprints/WP-07-S2) §4):

| Check | Evidence |
|---|---|
| Round trip | `crypto_codec_prop_file_round_trip`, `crypto_codec_prop_row_round_trip` (512 cases each), `crypto_seal_prop_round_trip` (128 cases) |
| Known answer | `crypto_seal_layout`: the ciphertext of draft-irtf-cfrg-xchacha-03 appendix A.3.1, and a tag for the associated data of this ADR that a second implementation computed (HChaCha20 from the draft and the ChaCha20-Poly1305 of OpenSSL) |
| Tamper | `crypto_tamper_*`: every bit of the magic, the nonce, the ciphertext and the tag, and every shorter length: `CACHE_CORRUPTED` |
| Move | `crypto_move_*`: another partition, cache key, role, file id, schema version or data key does not open; `driver_partitions_moved_file_is_corrupted`, `driver_partitions_moved_row_is_corrupted` |
| Logout | `driver_revoke_shreds`: after `RevokePartition` the old files that stay on disk do not open with the new key of the partition. `driver_commit_revoke_with_a_failing_key_port_still_revokes`: a locked key store does not stop the removal |
| Nonces | `crypto_seal_nonce_is_fresh`, `crypto_seal_nonce_comes_from_the_port` |
| No key in output | `crypto_key_debug_writes_no_byte`; `DataKey` has no `Clone`; errors hold a kind and a fixed operation name |
| Decoders | `store_file_decode` 252,853,878 runs and `store_row_decode` 189,375,683 runs in 1 hour each, no crash |
| Mutation | `rivqen-store::crypto` 136 of 151 |

<Badge type="danger" text="GATE" /> Not covered: the key wrap on the platforms (WP-10, WP-11) and the review against the threat model (WP-18).

## Related

- [ADR-004: Cache storage format](/engineering/architecture/adr/ADR-004-cache-storage)
- [Storage and memory](/engineering/cache/storage)
- [Crypto](/engineering/security/crypto)
