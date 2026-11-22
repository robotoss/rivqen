// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The host key provider (ADR-011 section 2): one key for each partition,
//! a new key after a delete, `Unavailable` for every failure of the port.

#![allow(clippy::unwrap_used)]

use std::io::Cursor;

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_store::crypto::{Aad, DataKey, FileId, HostKeys, KeyProvider, Role, open, seal};

fn scoped(byte: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([byte; 32]))
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

fn sealed_with(key: &DataKey) -> Vec<u8> {
    seal(key, &mut HostKeys::new().unwrap(), &aad(), b"content").unwrap()
}

fn opens(key: &DataKey, sealed: &[u8]) -> bool {
    open(key, &aad(), sealed).is_ok()
}

fn unavailable(op: &'static str) -> StoreError {
    StoreError::new(StoreErrorKind::Unavailable, op)
}

#[test]
fn crypto_host_keys_same_partition_same_key() {
    let mut keys = HostKeys::new().unwrap();
    let first = keys.data_key(scoped(1)).unwrap();
    let second = keys.data_key(scoped(1)).unwrap();
    assert!(opens(&second, &sealed_with(&first)));
}

#[test]
fn crypto_host_keys_partitions_have_own_keys() {
    let mut keys = HostKeys::new().unwrap();
    let a = keys.data_key(scoped(1)).unwrap();
    let b = keys.data_key(scoped(2)).unwrap();
    let public = keys.data_key(Partition::Public).unwrap();
    let sealed = sealed_with(&a);
    assert!(!opens(&b, &sealed));
    assert!(!opens(&public, &sealed));
    assert!(!opens(&a, &sealed_with(&public)));
}

#[test]
fn crypto_host_keys_delete_gives_another_key() {
    let mut keys = HostKeys::new().unwrap();
    let old = keys.data_key(scoped(1)).unwrap();
    let other = keys.data_key(scoped(2)).unwrap();
    let sealed = sealed_with(&old);
    let sealed_other = sealed_with(&other);

    keys.delete_key(scoped(1)).unwrap();
    let new = keys.data_key(scoped(1)).unwrap();
    assert!(!opens(&new, &sealed));
    // The key of another partition stays.
    let other_again = keys.data_key(scoped(2)).unwrap();
    assert!(opens(&other_again, &sealed_other));
    // A delete of a key that does not exist is not an error.
    keys.delete_key(scoped(3)).unwrap();
}

#[test]
fn crypto_host_keys_delete_of_public() {
    let mut keys = HostKeys::new().unwrap();
    let old = keys.data_key(Partition::Public).unwrap();
    let sealed = sealed_with(&old);
    keys.delete_key(Partition::Public).unwrap();
    assert!(!opens(&keys.data_key(Partition::Public).unwrap(), &sealed));
}

#[test]
fn crypto_host_keys_keys_come_from_the_source() {
    let mut source = vec![0x31; 32];
    source.extend_from_slice(&[0x32; 32]);
    let mut keys = HostKeys::with_random_source(Box::new(Cursor::new(source)));
    let a = keys.data_key(scoped(1)).unwrap();
    let b = keys.data_key(scoped(2)).unwrap();
    assert!(opens(
        &DataKey::from_bytes(&mut [0x31; 32]),
        &sealed_with(&a)
    ));
    assert!(opens(
        &DataKey::from_bytes(&mut [0x32; 32]),
        &sealed_with(&b)
    ));
    // The source is empty now: a new key fails, a known key does not.
    assert_eq!(
        keys.data_key(scoped(3)).unwrap_err(),
        unavailable("data_key")
    );
    assert!(keys.data_key(scoped(1)).is_ok());
}

#[test]
fn crypto_host_keys_source_failure_is_unavailable() {
    // 31 bytes: too few for a key.
    let mut keys = HostKeys::with_random_source(Box::new(Cursor::new(vec![7; 31])));
    assert_eq!(
        keys.data_key(scoped(1)).unwrap_err(),
        unavailable("data_key")
    );
    let mut buf = [0; 4];
    assert_eq!(keys.random(&mut buf), Err(unavailable("random")));
}

#[test]
fn crypto_host_keys_locked_is_unavailable() {
    let mut keys = HostKeys::new().unwrap();
    let before = keys.data_key(scoped(1)).unwrap();
    keys.set_locked(true);
    assert_eq!(
        keys.data_key(scoped(1)).unwrap_err(),
        unavailable("data_key")
    );
    assert_eq!(keys.delete_key(scoped(1)), Err(unavailable("delete_key")));
    let mut buf = [0; 4];
    assert_eq!(keys.random(&mut buf), Err(unavailable("random")));

    // After the unlock the key is still there: the locked delete did nothing.
    keys.set_locked(false);
    let after = keys.data_key(scoped(1)).unwrap();
    assert!(opens(&after, &sealed_with(&before)));
}

#[test]
fn crypto_host_keys_random_reads_the_source() {
    let mut keys = HostKeys::with_random_source(Box::new(Cursor::new(vec![1, 2, 3, 4, 5])));
    let mut buf = [0; 3];
    keys.random(&mut buf).unwrap();
    assert_eq!(buf, [1, 2, 3]);
    keys.random(&mut buf[..2]).unwrap();
    assert_eq!(buf, [4, 5, 3]);
}

#[test]
fn crypto_host_keys_system_source_gives_random_bytes() {
    let mut keys = HostKeys::new().unwrap();
    let mut a = [0; 32];
    let mut b = [0; 32];
    keys.random(&mut a).unwrap();
    keys.random(&mut b).unwrap();
    assert_ne!(a, b);
    assert_ne!(a, [0; 32]);
}

#[test]
fn crypto_host_keys_debug_has_no_key() {
    let mut keys = HostKeys::with_random_source(Box::new(Cursor::new(vec![0xa5; 32])));
    keys.data_key(scoped(1)).unwrap();
    let text = format!("{keys:?}");
    assert_eq!(text, "HostKeys { keys: 1, locked: false, .. }");
}
