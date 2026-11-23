// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The sealed form of ADR-011 section 3: the byte layout with a known
//! answer, fresh nonces, tamper and move checks.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::io::Cursor;

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_store::crypto::{
    Aad, DataKey, FileId, HostKeys, KeyProvider, MAGIC, NONCE_BYTES, Role, SEAL_OVERHEAD,
    TAG_BYTES, open, seal,
};

/// Key, nonce and plain text of draft-irtf-cfrg-xchacha-03, appendix A.3.1
/// (<https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha-03#appendix-A.3.1>).
const KAT_KEY: [u8; 32] = [
    0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f,
    0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f,
];
const KAT_NONCE: [u8; 24] = [
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
    0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57,
];
const KAT_PLAIN: &[u8] = b"Ladies and Gentlemen of the class of '99: \
If I could offer you only one tip for the future, sunscreen would be it.";
/// The ciphertext of appendix A.3.1. The ciphertext does not depend on the
/// associated data, so the seal below has the same ciphertext.
const KAT_CIPHERTEXT: &str = "\
bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb\
731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452\
2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9\
21f9664c97637da9768812f615c68b13b52e";
/// The tag of the seal below, with the Rivqen associated data of
/// `kat_aad`. Computed outside this code: `HChaCha20` written from section
/// 2.2 of the draft and the `ChaCha20-Poly1305` of Node.js 26 (OpenSSL), as
/// section 2.3 of the draft defines `XChaCha20-Poly1305`. That program gives
/// the `HChaCha20` subkey of section 2.2.1 and the ciphertext and the tag
/// `c0875924c1c7987947deafd8780acf49` of appendix A.3.1 before it gives
/// this tag.
const KAT_TAG: &str = "7985f15bd14c44e85036eb887d9ab09f";
/// The associated data of `kat_aad`, as the same program wrote it.
const KAT_AAD: &str = "\
000000000000000e72697671656e2d7365616c2d7631\
000000000000000466696c65\
000000000000000400000001\
0000000000000021011111111111111111111111111111111111111111111111111111111111111111\
00000000000000202222222222222222222222222222222222222222222222222222222222222222\
0000000000000010000102030405060708090a0b0c0d0e0f";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn key_of(byte: u8) -> DataKey {
    DataKey::from_bytes(&mut [byte; 32])
}

fn file_id() -> FileId {
    let mut bytes = [0; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::try_from(i).unwrap();
    }
    FileId::from_bytes(bytes)
}

fn scoped(byte: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([byte; 32]))
}

fn kat_aad() -> Aad {
    Aad::new(
        Role::File,
        1,
        scoped(0x11),
        &CacheKey::from_bytes([0x22; 32]),
        &file_id(),
    )
}

/// A provider whose random bytes are `bytes`, then nothing.
fn fixed(bytes: &[u8]) -> HostKeys {
    HostKeys::with_random_source(Box::new(Cursor::new(bytes.to_vec())))
}

fn system() -> HostKeys {
    HostKeys::new().unwrap()
}

fn corrupted() -> StoreError {
    StoreError::new(StoreErrorKind::Corrupted, "open")
}

/// Appends `bytes` after its length as 8 bytes big-endian.
fn lp(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

#[test]
fn crypto_seal_layout() {
    // The associated data, written by hand from ADR-011 section 3.
    let mut aad = Vec::new();
    lp(&mut aad, b"rivqen-seal-v1");
    lp(&mut aad, b"file");
    lp(&mut aad, &1_u32.to_be_bytes());
    let mut partition = vec![0x01];
    partition.extend_from_slice(&[0x11; 32]);
    lp(&mut aad, &partition);
    lp(&mut aad, &[0x22; 32]);
    lp(&mut aad, file_id().as_bytes());
    assert_eq!(kat_aad().as_bytes(), aad.as_slice());
    assert_eq!(kat_aad().as_bytes(), hex(KAT_AAD).as_slice());

    let key = DataKey::from_bytes(&mut { KAT_KEY });
    let sealed = seal(&key, &mut fixed(&KAT_NONCE), &kat_aad(), KAT_PLAIN).unwrap();

    let mut expected = b"RQS1".to_vec();
    expected.extend_from_slice(&KAT_NONCE);
    expected.extend_from_slice(&hex(KAT_CIPHERTEXT));
    expected.extend_from_slice(&hex(KAT_TAG));
    assert_eq!(sealed, expected);
    assert_eq!(sealed.len(), KAT_PLAIN.len() + SEAL_OVERHEAD);
    assert_eq!(open(&key, &kat_aad(), &sealed).unwrap(), KAT_PLAIN);
}

#[test]
fn crypto_seal_layout_of_the_other_parts() {
    // `public` is the one byte 02; the role `row`; another schema version.
    let key = CacheKey::from_bytes([0x33; 32]);
    let id = FileId::from_bytes([0xee; 16]);
    let mut expected = Vec::new();
    lp(&mut expected, b"rivqen-seal-v1");
    lp(&mut expected, b"row");
    lp(&mut expected, &0x0102_0304_u32.to_be_bytes());
    lp(&mut expected, &[0x02]);
    lp(&mut expected, &[0x33; 32]);
    lp(&mut expected, &[0xee; 16]);
    let aad = Aad::new(Role::Row, 0x0102_0304, Partition::Public, &key, &id);
    assert_eq!(aad.as_bytes(), expected.as_slice());
    assert_eq!(format!("{aad:?}"), format!("Aad(len={})", expected.len()));
}

#[test]
fn crypto_seal_empty_plain_text() {
    let key = key_of(7);
    let sealed = seal(&key, &mut system(), &kat_aad(), b"").unwrap();
    assert_eq!(sealed.len(), SEAL_OVERHEAD);
    assert_eq!(&sealed[..4], &MAGIC);
    assert_eq!(open(&key, &kat_aad(), &sealed).unwrap(), b"");
}

#[test]
fn crypto_seal_nonce_is_fresh() {
    let key = key_of(7);
    let mut port = system();
    let first = seal(&key, &mut port, &kat_aad(), KAT_PLAIN).unwrap();
    let second = seal(&key, &mut port, &kat_aad(), KAT_PLAIN).unwrap();
    assert_ne!(first, second);
    assert_ne!(&first[4..4 + NONCE_BYTES], &second[4..4 + NONCE_BYTES]);
    assert_eq!(open(&key, &kat_aad(), &first).unwrap(), KAT_PLAIN);
    assert_eq!(open(&key, &kat_aad(), &second).unwrap(), KAT_PLAIN);
}

#[test]
fn crypto_seal_nonce_comes_from_the_port() {
    let key = key_of(7);
    let nonce = [0x5a; NONCE_BYTES];
    let sealed = seal(&key, &mut fixed(&nonce), &kat_aad(), b"x").unwrap();
    assert_eq!(&sealed[4..4 + NONCE_BYTES], &nonce);
}

/// A sealed value of 40 bytes of plain text and the key and the
/// associated data that open it.
fn sample() -> (DataKey, Aad, Vec<u8>) {
    let key = key_of(9);
    let aad = kat_aad();
    let sealed = seal(&key, &mut system(), &aad, &[0xab; 40]).unwrap();
    assert!(open(&key, &aad, &sealed).is_ok());
    (key, aad, sealed)
}

/// Every one-bit change of each byte in `range` makes open fail with the
/// one error.
fn assert_tamper(range: std::ops::Range<usize>) {
    let (key, aad, sealed) = sample();
    for at in range {
        for bit in 0..8 {
            let mut changed = sealed.clone();
            changed[at] ^= 1 << bit;
            assert_eq!(open(&key, &aad, &changed), Err(corrupted()), "byte {at}");
        }
    }
}

#[test]
fn crypto_tamper_magic() {
    assert_tamper(0..4);
}

#[test]
fn crypto_tamper_nonce() {
    assert_tamper(4..4 + NONCE_BYTES);
}

#[test]
fn crypto_tamper_ciphertext() {
    assert_tamper(4 + NONCE_BYTES..4 + NONCE_BYTES + 40);
}

#[test]
fn crypto_tamper_tag() {
    let (_, _, sealed) = sample();
    assert_tamper(sealed.len() - TAG_BYTES..sealed.len());
}

#[test]
fn crypto_tamper_short_input() {
    let (key, aad, sealed) = sample();
    for len in 0..sealed.len() {
        assert_eq!(open(&key, &aad, &sealed[..len]), Err(corrupted()), "{len}");
    }
    // A value of the length of the overhead with another magic.
    let mut bare = vec![0; SEAL_OVERHEAD];
    bare[..4].copy_from_slice(b"RQS2");
    assert_eq!(open(&key, &aad, &bare), Err(corrupted()));
}

#[test]
fn crypto_tamper_added_or_removed_bytes() {
    let (key, aad, sealed) = sample();
    let mut longer = sealed.clone();
    longer.push(0);
    assert_eq!(open(&key, &aad, &longer), Err(corrupted()));
    let mut shorter = sealed.clone();
    shorter.remove(4 + NONCE_BYTES);
    assert_eq!(open(&key, &aad, &shorter), Err(corrupted()));
}

#[test]
fn crypto_tamper_error_holds_no_detail() {
    let error = corrupted();
    assert_eq!(error.to_string(), "CACHE_CORRUPTED in open");
    assert_eq!(
        format!("{error:?}"),
        "StoreError { kind: Corrupted, op: \"open\" }"
    );
}

/// Seals with `from`, then opens with `to`: the value does not open.
fn assert_moved(from: &Aad, to: &Aad) {
    let key = key_of(9);
    let sealed = seal(&key, &mut system(), from, b"content").unwrap();
    assert_eq!(open(&key, from, &sealed).unwrap(), b"content");
    assert_eq!(open(&key, to, &sealed), Err(corrupted()));
}

fn aad(role: Role, schema: u32, partition: Partition, key: u8, file: u8) -> Aad {
    Aad::new(
        role,
        schema,
        partition,
        &CacheKey::from_bytes([key; 32]),
        &FileId::from_bytes([file; 16]),
    )
}

#[test]
fn crypto_move_other_partition() {
    assert_moved(
        &aad(Role::File, 1, scoped(1), 5, 6),
        &aad(Role::File, 1, scoped(2), 5, 6),
    );
}

#[test]
fn crypto_move_between_public_and_scoped() {
    let public = aad(Role::Row, 1, Partition::Public, 5, 6);
    let scoped = aad(Role::Row, 1, scoped(2), 5, 6);
    assert_moved(&public, &scoped);
    assert_moved(&scoped, &public);
}

#[test]
fn crypto_move_other_cache_key() {
    assert_moved(
        &aad(Role::File, 1, scoped(1), 5, 6),
        &aad(Role::File, 1, scoped(1), 4, 6),
    );
}

#[test]
fn crypto_move_other_role() {
    assert_moved(
        &aad(Role::File, 1, scoped(1), 5, 6),
        &aad(Role::Row, 1, scoped(1), 5, 6),
    );
    assert_moved(
        &aad(Role::Row, 1, scoped(1), 5, 6),
        &aad(Role::File, 1, scoped(1), 5, 6),
    );
}

#[test]
fn crypto_move_other_file_id() {
    assert_moved(
        &aad(Role::File, 1, scoped(1), 5, 6),
        &aad(Role::File, 1, scoped(1), 5, 7),
    );
}

#[test]
fn crypto_move_other_schema_version() {
    assert_moved(
        &aad(Role::File, 1, scoped(1), 5, 6),
        &aad(Role::File, 2, scoped(1), 5, 6),
    );
}

#[test]
fn crypto_move_other_data_key() {
    let aad = kat_aad();
    let sealed = seal(&key_of(1), &mut system(), &aad, b"content").unwrap();
    assert_eq!(open(&key_of(2), &aad, &sealed), Err(corrupted()));
}

/// A key port that always fails with `kind`.
struct Broken(StoreErrorKind);

impl KeyProvider for Broken {
    fn data_key(&mut self, _: Partition) -> Result<DataKey, StoreError> {
        Err(StoreError::new(self.0, "data_key"))
    }

    fn delete_key(&mut self, _: Partition) -> Result<(), StoreError> {
        Err(StoreError::new(self.0, "delete_key"))
    }

    fn random(&mut self, _: &mut [u8]) -> Result<(), StoreError> {
        Err(StoreError::new(self.0, "random"))
    }
}

#[test]
fn crypto_key_port_error_fails_seal() {
    let key = key_of(3);
    let unavailable = StoreError::new(StoreErrorKind::Unavailable, "random");
    // A port with no random bytes.
    assert_eq!(
        seal(&key, &mut fixed(&[]), &kat_aad(), b"content"),
        Err(unavailable)
    );
    // Too few bytes for a nonce.
    assert_eq!(
        seal(
            &key,
            &mut fixed(&[1; NONCE_BYTES - 1]),
            &kat_aad(),
            b"content"
        ),
        Err(unavailable)
    );
    // An error of the port is Unavailable, also when the port names
    // another kind.
    for kind in [StoreErrorKind::Unavailable, StoreErrorKind::Corrupted] {
        assert_eq!(
            seal(&key, &mut Broken(kind), &kat_aad(), b"content"),
            Err(unavailable)
        );
    }
}
