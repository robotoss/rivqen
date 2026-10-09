# Cryptography

This page defines where Rivqen uses cryptography and which rules apply. Rivqen does not invent cryptographic algorithms.

**Status:** <Badge type="info" text="DESIGN" /> Decision record: [ADR-011](/engineering/architecture/adr/#adr-011).

## 1. Uses

| Use | Algorithm | Purpose | Security role |
|---|---|---|---|
| legacy compatibility hashes | SHA-1 | Reproduce upstream `etag` / `html-sha1` / `template-tag` values | **None.** Compatibility only. Never used for security decisions. |
| RQP block hashes | SHA-256 | Detect corruption and mismatched canonicalization | Integrity against accidents, not authorship |
| Partition IDs | HMAC-SHA256 with a device key | Unlinkable partition names on disk | Privacy |
| Private cache | AEAD (AES-256-GCM or ChaCha20-Poly1305) | Confidentiality and integrity at rest | Security |
| Manifest signatures (optional, future) | Ed25519 | Authorship of offline packages | Security; needs its own threat model (Q-08) |
| Transport | TLS 1.2+ / TLS 1.3 via the platform | Confidentiality in transit | Security (platform-owned) |

::: warning A hash is not a signature
SHA-256 in a manifest detects accidental changes. It does not prove who wrote the manifest. TLS protects transport. A signature is needed only for content that travels outside TLS (for example offline packages).
:::

## 2. Key management

| Rule | Detail |
|---|---|
| Key storage | Platform key stores only: Android Keystore, iOS Keychain. |
| Key hierarchy | Platform key (non-exportable) → wraps per-partition data keys. |
| Rotation | Rotate a data key before its nonce counter limit and on schema change. |
| Logout | Delete the partition data key first, then files (crypto-shredding). |
| Backup | Keys are device-only. Encrypted cache is not restorable on another device by design. |
| Zeroization | Use `zeroize` for key buffers in Rust. Managed runtimes (Kotlin, Swift) cannot guarantee zeroization; keep key material out of them where possible. |
| Libraries | Audited, maintained crates only (RustCrypto AEAD crates or `ring`/`aws-lc-rs`). Choose in ADR-011 by audit status, license, size and platform support. |

## 3. Tests

- Tamper test: change one byte of an encrypted file → read fails → entry deleted → page fetched.
- Move test: copy an encrypted file to another entry → associated data mismatch → read fails.
- Logout test: after logout, no entry of the old partition can be decrypted.
- Key-loss test: platform key deleted (for example after biometric change) → cache cleared safely.
