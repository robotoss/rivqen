// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! SHA-1 of the legacy protocol (H-143): the FIPS 180-2 known answers of
//! appendix A ("abc", the 448-bit message, one million `a`), the digest of
//! the empty message, and the text forms of [`Sha1Hex`].

#![cfg(feature = "legacy")]
#![allow(clippy::unwrap_used)]

use std::collections::HashSet;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_integrity::legacy::{SHA1_HEX_CHARS, Sha1Hex, sha1_hex};

#[test]
fn integrity_sha1_empty_message() {
    assert_eq!(
        sha1_hex(b"").as_str(),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
}

#[test]
fn integrity_sha1_abc() {
    assert_eq!(
        sha1_hex(b"abc").as_str(),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
}

#[test]
fn integrity_sha1_448_bit_message() {
    let message = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    assert_eq!(message.len() * 8, 448);
    assert_eq!(
        sha1_hex(message).as_str(),
        "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
    );
}

#[test]
fn integrity_sha1_one_million_a() {
    assert_eq!(
        sha1_hex(&vec![b'a'; 1_000_000]).as_str(),
        "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
    );
}

#[test]
fn integrity_sha1_display_is_the_full_hex_text() {
    let digest = sha1_hex(b"abc");
    assert_eq!(digest.to_string(), digest.as_str());
    assert_eq!(digest.to_string().len(), SHA1_HEX_CHARS);
}

#[test]
fn integrity_sha1_debug_shows_eight_characters() {
    let digest = sha1_hex(b"abc");
    assert_eq!(format!("{digest:?}"), "Sha1Hex(a9993e36…)");
}

#[test]
fn integrity_sha1_values_compare_and_hash_by_text() {
    let a = sha1_hex(b"a");
    let copy: Sha1Hex = a;
    assert_eq!(a, copy);
    assert_ne!(a, sha1_hex(b"b"));
    let set: HashSet<Sha1Hex> = [a, copy, sha1_hex(b"b")].into_iter().collect();
    assert_eq!(set.len(), 2);
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        rng_seed: RngSeed::Fixed(0x0006_5a01_2026_1112),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Every value is 40 lowercase hex characters.
    #[test]
    fn integrity_sha1_text_is_40_lowercase_hex(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let text = sha1_hex(&bytes).as_str().to_owned();
        prop_assert_eq!(text.len(), SHA1_HEX_CHARS);
        prop_assert!(text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }
}
