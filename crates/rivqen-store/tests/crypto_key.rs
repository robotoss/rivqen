// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The key type and the file id (ADR-011 section 2, ADR-004 section 1;
//! errors-logging section 4): no key byte in any output, zero on drop, no
//! copy; the hex name of a file id.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::io::Cursor;
use std::marker::PhantomData;

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_model::ids::CacheKey;
use rivqen_store::crypto::{Aad, DataKey, FileId, HostKeys, Role, open, seal};

/// `true` when `T` implements `Clone` (an inherent constant wins over the
/// constant of the blanket trait only when its bound holds).
trait NotClone {
    const IS_CLONE: bool = false;
}
impl<T> NotClone for Probe<T> {}
struct Probe<T>(PhantomData<T>);
impl<T: Clone> Probe<T> {
    const IS_CLONE: bool = true;
}

fn aad() -> Aad {
    Aad::new(
        Role::File,
        1,
        Partition::Public,
        &CacheKey::from_bytes([1; 32]),
        &FileId::from_bytes([2; 16]),
    )
}

#[test]
fn crypto_key_debug_writes_no_byte() {
    let key = DataKey::from_bytes(&mut [0xa5; 32]);
    let text = format!("{key:?} {key:#?}");
    assert_eq!(format!("{key:?}"), "DataKey(<redacted>)");
    assert!(!text.contains("a5"), "{text}");
    assert!(!text.contains("165"), "{text}");
}

#[test]
fn crypto_key_from_bytes_clears_the_source() {
    let mut bytes = [0x42; 32];
    let key = DataKey::from_bytes(&mut bytes);
    assert_eq!(bytes, [0; 32]);
    // The key holds the bytes from before the clear.
    let sealed = seal(&key, &mut HostKeys::new().unwrap(), &aad(), b"x").unwrap();
    let same = DataKey::from_bytes(&mut [0x42; 32]);
    assert_eq!(open(&same, &aad(), &sealed).unwrap(), b"x");
}

#[test]
fn crypto_key_is_zero_on_drop_and_has_no_clone() {
    fn zeroize_on_drop<T: zeroize::ZeroizeOnDrop>() {}
    zeroize_on_drop::<DataKey>();
    const { assert!(!<Probe<DataKey>>::IS_CLONE) };
    // The probe itself works.
    const { assert!(<Probe<FileId>>::IS_CLONE) };
}

#[test]
fn crypto_key_file_id_hex() {
    let mut bytes = [0; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::try_from(i * 17).unwrap();
    }
    let id = FileId::from_bytes(bytes);
    let text = id.to_hex();
    assert_eq!(text, "00112233445566778899aabbccddeeff");
    assert_eq!(FileId::from_hex(&text), Some(id));
    assert_eq!(id.as_bytes(), &bytes);
    let all = FileId::from_bytes([0xff; 16]);
    assert_eq!(FileId::from_hex(&all.to_hex()), Some(all));
    assert_eq!(all.to_hex(), "f".repeat(32));
}

#[test]
fn crypto_key_file_id_hex_has_one_form() {
    let good = "00112233445566778899aabbccddeeff";
    for bad in [
        "",
        &good[..31],
        &good[..30],
        &format!("{good}0"),
        &format!("{good}00"),
        "00112233445566778899AABBCCDDEEFF",
        "00112233445566778899aabbccddeefg",
        "0011223344556677 899aabbccddeeff",
        "g0112233445566778899aabbccddeeff",
        "/0112233445566778899aabbccddeeff",
        ":0112233445566778899aabbccddeeff",
        "`0112233445566778899aabbccddeeff",
        "00112233445566778899aabbccddeé",
    ] {
        assert_eq!(FileId::from_hex(bad), None, "{bad:?}");
    }
}

#[test]
fn crypto_key_file_id_debug_prefix_only() {
    let id = FileId::from_hex("0123456789abcdef0011223344556677").unwrap();
    assert_eq!(format!("{id:?}"), "FileId(01234567...)");
}

#[test]
fn crypto_key_file_id_comes_from_the_port() {
    let bytes: Vec<u8> = (100..116).collect();
    let mut port = HostKeys::with_random_source(Box::new(Cursor::new(bytes.clone())));
    let id = FileId::new(&mut port).unwrap();
    assert_eq!(id.as_bytes().as_slice(), bytes.as_slice());
    // The source is empty now: the port fails, and the error is Unavailable.
    assert_eq!(
        FileId::new(&mut port),
        Err(StoreError::new(StoreErrorKind::Unavailable, "random"))
    );
}

#[test]
fn crypto_key_file_ids_differ() {
    let mut port = HostKeys::new().unwrap();
    let a = FileId::new(&mut port).unwrap();
    let b = FileId::new(&mut port).unwrap();
    assert_ne!(a, b);
}
