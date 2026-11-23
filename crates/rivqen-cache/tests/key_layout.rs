// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The byte layout of the cache key (cache identity section 1, "Byte
//! layout"; DL-184). Each test writes the hash input by hand from the
//! documented layout and compares `cache_key` with SHA-256 of those bytes.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use std::fmt::Write as _;

use rivqen_cache::key::{KeyParts, Partition, Protocol, VaryDigest, VaryEntry, cache_key};
use rivqen_integrity::sha256;
use rivqen_model::ids::{CacheKey, PartitionId};

const ORIGIN: &str = "https://shop.example.com";
const URL: &str = "https://shop.example.com/catalog?a=1";

/// Appends `bytes` with its length as 8 bytes big-endian.
fn lp(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// `head ‖ body` as one length-prefixed field.
fn lp2(out: &mut Vec<u8>, head: u8, body: &[u8]) {
    let mut field = vec![head];
    field.extend_from_slice(body);
    lp(out, &field);
}

fn key_of(bytes: &[u8]) -> CacheKey {
    CacheKey::from_bytes(*sha256(bytes).as_bytes())
}

fn hex(key: &CacheKey) -> String {
    key.as_bytes().iter().fold(String::new(), |mut out, b| {
        write!(out, "{b:02x}").unwrap();
        out
    })
}

fn scoped(byte: u8) -> Partition {
    Partition::Scoped(PartitionId::from_bytes([byte; 32]))
}

fn parts(partition: Partition, vary: &[VaryEntry]) -> KeyParts<'_> {
    KeyParts {
        partition,
        origin: ORIGIN,
        canonical_url: URL,
        vary,
        protocol: Protocol::Rqp,
        locale: None,
    }
}

fn entry(name: &str, value: Option<u8>) -> VaryEntry {
    VaryEntry {
        name: name.to_owned(),
        value: value.map(|b| VaryDigest::from_bytes([b; 32])),
    }
}

/// The hash input up to and with the method, for `ORIGIN` and `URL`.
fn head(partition: Partition) -> Vec<u8> {
    let mut out = Vec::new();
    lp(&mut out, b"rivqen-cache-key-v1");
    match partition {
        Partition::Scoped(id) => lp2(&mut out, 0x01, id.as_bytes()),
        Partition::Public => lp(&mut out, &[0x02]),
    }
    lp(&mut out, ORIGIN.as_bytes());
    lp(&mut out, URL.as_bytes());
    lp(&mut out, b"GET");
    out
}

#[test]
fn key_known_answers() {
    // Scoped, one Vary entry, RQP, a locale.
    let vary = [entry("accept-language", Some(0xAA))];
    let key = cache_key(&KeyParts {
        locale: Some("en-GB"),
        ..parts(scoped(0x11), &vary)
    });
    let mut bytes = head(scoped(0x11));
    lp(&mut bytes, &1_u64.to_be_bytes());
    lp(&mut bytes, b"accept-language");
    lp2(&mut bytes, 0x01, &[0xAA; 32]);
    lp(&mut bytes, b"rqp");
    lp2(&mut bytes, 0x01, b"en-GB");
    assert_eq!(key, key_of(&bytes));
    // The same bytes hashed by `shasum -a 256` outside Rust.
    assert_eq!(
        hex(&key),
        "2f3788da519c948f438dfa129fccceb1b214762e98f3c0b4818e2b007ad9e88b"
    );

    // Public, no Vary, legacy, no locale.
    let key = cache_key(&KeyParts {
        partition: Partition::Public,
        origin: "https://news.example",
        canonical_url: "https://news.example/",
        vary: &[],
        protocol: Protocol::Legacy,
        locale: None,
    });
    let mut bytes = Vec::new();
    lp(&mut bytes, b"rivqen-cache-key-v1");
    lp(&mut bytes, &[0x02]);
    lp(&mut bytes, b"https://news.example");
    lp(&mut bytes, b"https://news.example/");
    lp(&mut bytes, b"GET");
    lp(&mut bytes, &0_u64.to_be_bytes());
    lp(&mut bytes, b"legacy");
    lp(&mut bytes, &[0x00]);
    assert_eq!(key, key_of(&bytes));
    assert_eq!(
        hex(&key),
        "430f212e1fae3eb7cf0671474a80769942f36ba947ab45008236d6ae246e8112"
    );
}

#[test]
fn key_layout_scoped_partition_is_tag_and_id() {
    let id = PartitionId::from_bytes(core::array::from_fn(|i| i as u8));
    let mut bytes = head(Partition::Scoped(id));
    lp(&mut bytes, &0_u64.to_be_bytes());
    lp(&mut bytes, b"rqp");
    lp(&mut bytes, &[0x00]);
    assert_eq!(
        cache_key(&parts(Partition::Scoped(id), &[])),
        key_of(&bytes)
    );
    // The partition field is 33 bytes: the length 33, the tag 0x01, the id.
    let start = 8 + "rivqen-cache-key-v1".len();
    assert_eq!(bytes[start..start + 8], 33_u64.to_be_bytes());
    assert_eq!(bytes[start + 8], 0x01);
}

#[test]
fn key_layout_public_has_its_own_tag() {
    let mut bytes = head(Partition::Public);
    lp(&mut bytes, &0_u64.to_be_bytes());
    lp(&mut bytes, b"rqp");
    lp(&mut bytes, &[0x00]);
    let public = cache_key(&parts(Partition::Public, &[]));
    assert_eq!(public, key_of(&bytes));
    // No id gives the public field: its length is 1, an id field has 33.
    for byte in [0x00, 0x01, 0x02, 0xFF] {
        assert_ne!(public, cache_key(&parts(scoped(byte), &[])));
    }
}

#[test]
fn key_layout_absent_vary_value_differs_from_any_digest() {
    let absent = [entry("cookie", None)];
    let zero = [entry("cookie", Some(0x00))];
    let key_absent = cache_key(&parts(scoped(1), &absent));
    let key_zero = cache_key(&parts(scoped(1), &zero));
    assert_ne!(key_absent, key_zero);

    let mut bytes = head(scoped(1));
    lp(&mut bytes, &1_u64.to_be_bytes());
    lp(&mut bytes, b"cookie");
    lp(&mut bytes, &[0x00]);
    lp(&mut bytes, b"rqp");
    lp(&mut bytes, &[0x00]);
    assert_eq!(key_absent, key_of(&bytes));

    let mut bytes = head(scoped(1));
    lp(&mut bytes, &1_u64.to_be_bytes());
    lp(&mut bytes, b"cookie");
    lp2(&mut bytes, 0x01, &[0x00; 32]);
    lp(&mut bytes, b"rqp");
    lp(&mut bytes, &[0x00]);
    assert_eq!(key_zero, key_of(&bytes));
}

#[test]
fn key_layout_absent_locale_differs_from_empty_locale() {
    let absent = cache_key(&parts(scoped(1), &[]));
    let empty = cache_key(&KeyParts {
        locale: Some(""),
        ..parts(scoped(1), &[])
    });
    assert_ne!(absent, empty);

    let mut bytes = head(scoped(1));
    lp(&mut bytes, &0_u64.to_be_bytes());
    lp(&mut bytes, b"rqp");
    lp(&mut bytes, &[0x01]);
    assert_eq!(empty, key_of(&bytes));
}

#[test]
fn key_layout_protocols_differ() {
    let rqp = cache_key(&parts(scoped(1), &[]));
    let legacy = cache_key(&KeyParts {
        protocol: Protocol::Legacy,
        ..parts(scoped(1), &[])
    });
    assert_ne!(rqp, legacy);
    let mut bytes = head(scoped(1));
    lp(&mut bytes, &0_u64.to_be_bytes());
    lp(&mut bytes, b"legacy");
    lp(&mut bytes, &[0x00]);
    assert_eq!(legacy, key_of(&bytes));
}

#[test]
fn key_layout_vary_count_comes_before_the_entries() {
    let two = [entry("accept", Some(3)), entry("cookie", None)];
    let mut bytes = head(scoped(1));
    lp(&mut bytes, &2_u64.to_be_bytes());
    lp(&mut bytes, b"accept");
    lp2(&mut bytes, 0x01, &[3; 32]);
    lp(&mut bytes, b"cookie");
    lp(&mut bytes, &[0x00]);
    lp(&mut bytes, b"rqp");
    lp(&mut bytes, &[0x00]);
    assert_eq!(cache_key(&parts(scoped(1), &two)), key_of(&bytes));
}

#[test]
fn key_layout_vary_entries_are_sorted_by_name() {
    let sorted = [entry("accept", None), entry("cookie", Some(7))];
    let reversed = [entry("cookie", Some(7)), entry("accept", None)];
    let key = cache_key(&parts(scoped(1), &sorted));
    assert_eq!(key, cache_key(&parts(scoped(1), &reversed)));

    let mut bytes = head(scoped(1));
    lp(&mut bytes, &2_u64.to_be_bytes());
    lp(&mut bytes, b"accept");
    lp(&mut bytes, &[0x00]);
    lp(&mut bytes, b"cookie");
    lp2(&mut bytes, 0x01, &[7; 32]);
    lp(&mut bytes, b"rqp");
    lp(&mut bytes, &[0x00]);
    assert_eq!(key, key_of(&bytes));
}

#[test]
fn key_layout_equal_names_sort_absent_first_then_by_digest() {
    // Not a valid Vary list (a name twice), but the key is still one
    // exact value for the set of entries.
    let given = [entry("x", Some(2)), entry("x", Some(1)), entry("x", None)];
    let key = cache_key(&parts(scoped(1), &given));

    let mut bytes = head(scoped(1));
    lp(&mut bytes, &3_u64.to_be_bytes());
    lp(&mut bytes, b"x");
    lp(&mut bytes, &[0x00]);
    lp(&mut bytes, b"x");
    lp2(&mut bytes, 0x01, &[1; 32]);
    lp(&mut bytes, b"x");
    lp2(&mut bytes, 0x01, &[2; 32]);
    lp(&mut bytes, b"rqp");
    lp(&mut bytes, &[0x00]);
    assert_eq!(key, key_of(&bytes));
}

#[test]
fn key_layout_name_case_is_kept() {
    let lower = [entry("cookie", None)];
    let upper = [entry("Cookie", None)];
    assert_ne!(
        cache_key(&parts(scoped(1), &lower)),
        cache_key(&parts(scoped(1), &upper))
    );
}

#[test]
fn key_layout_two_profiles_same_url_differ() {
    // Cache identity section 4, "Two profiles, same URL".
    assert_ne!(
        cache_key(&parts(scoped(1), &[])),
        cache_key(&parts(scoped(2), &[]))
    );
}

#[test]
fn key_layout_vary_accept_language_separates_languages() {
    // Cache identity section 4, "Vary: Accept-Language".
    let en = [entry("accept-language", Some(0x0E))];
    let de = [entry("accept-language", Some(0x0D))];
    assert_ne!(
        cache_key(&parts(scoped(1), &en)),
        cache_key(&parts(scoped(1), &de))
    );
}

#[test]
fn key_layout_query_order_is_significant() {
    // Cache identity section 4, "Query parameter order change" (default:
    // the canonical URL keeps the order; WP-08 gives the URL).
    let a = cache_key(&KeyParts {
        canonical_url: "https://shop.example.com/catalog?a=1&b=2",
        ..parts(scoped(1), &[])
    });
    let b = cache_key(&KeyParts {
        canonical_url: "https://shop.example.com/catalog?b=2&a=1",
        ..parts(scoped(1), &[])
    });
    assert_ne!(a, b);
}

#[test]
fn key_layout_debug_shows_four_bytes_only() {
    let key = cache_key(&parts(scoped(0x11), &[]));
    let text = format!("{key:?}");
    let full = hex(&key);
    assert_eq!(text, format!("CacheKey({})", &full[..8]));
}
