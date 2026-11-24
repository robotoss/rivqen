// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The key of one entry per URL (H-160), the `Vary` entries of a commit
//! with keyed digests (CL-12, DL-127, H-147) and `Debug` without content
//! (WP-08 S2 T-02).
#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use hmac::{Hmac, KeyInit, Mac};
use rivqen_cache::key::{KeyParts, Partition, Protocol, VaryDigest, VaryEntry, cache_key};
use rivqen_cache::txn::{Epoch, Expect};
use rivqen_integrity::legacy::sha1_hex;
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::Secret;
use rivqen_model::time::UnixTime;
use rivqen_session::legacy::record::{
    LegacyRecord, NewRecord, NotVisible, RequestValues, WriteBasis, entry_key, vary_entries,
};
use rivqen_session::legacy::types::VaryKey;
use rivqen_template::legacy::DataBlock;
use sha2::Sha256;

fn key_bytes() -> [u8; 32] {
    core::array::from_fn(|n| u8::try_from(n).unwrap())
}

fn vary_key() -> VaryKey {
    VaryKey(Secret::new(key_bytes()))
}

fn partition(byte: u8) -> PartitionId {
    PartitionId::from_bytes([byte; 32])
}

fn url(text: &str) -> rivqen_url::ParsedUrl {
    rivqen_url::parse(text).unwrap()
}

fn line(name: &str, value: &str) -> FieldLine {
    FieldLine::new(name, value)
}

fn vary(value: &str) -> Vec<FieldLine> {
    vec![line("Vary", value)]
}

/// The digest of `value` for `name` by the formula of the module
/// documentation, with an HMAC made from the 32-byte key as a slice.
fn expected_digest(name: &str, value: &[u8]) -> VaryDigest {
    let tag = b"rivqen-vary-v1";
    let mut mac = Hmac::<Sha256>::new_from_slice(&key_bytes()).unwrap();
    mac.update(&(tag.len() as u64).to_be_bytes());
    mac.update(tag);
    mac.update(&(name.len() as u64).to_be_bytes());
    mac.update(name.as_bytes());
    mac.update(value);
    VaryDigest::from_bytes(mac.finalize().into_bytes().into())
}

fn entries(
    response: &[FieldLine],
    session: &[FieldLine],
    platform: &[FieldLine],
) -> Option<Vec<VaryEntry>> {
    let values = RequestValues {
        session_fields: session,
        platform_fields: platform,
    };
    vary_entries(response, &values, &vary_key())
}

/// Lower-case hex of `bytes`.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        write!(out, "{b:02x}").unwrap();
        out
    })
}

fn digest_of(name: &str, session: &[FieldLine], platform: &[FieldLine]) -> Option<VaryDigest> {
    let found = entries(&vary(name), session, platform).unwrap();
    assert_eq!(found.len(), 1);
    found[0].value
}

// ---- entry_key ----

#[test]
fn legacy_record_key_is_the_cache_key_without_vary() {
    let parsed = url("https://Shop.example:443/a/%7e?b=1#frag");
    let expected = cache_key(&KeyParts {
        partition: Partition::Scoped(partition(1)),
        origin: "https://shop.example",
        canonical_url: "https://shop.example/a/~?b=1",
        vary: &[],
        protocol: Protocol::Legacy,
        locale: None,
    });
    assert_eq!(entry_key(partition(1), &parsed), expected);
}

#[test]
fn legacy_record_key_equal_for_one_canonical_url() {
    let a = entry_key(partition(1), &url("https://shop.example/p?x=1"));
    let b = entry_key(partition(1), &url("HTTPS://SHOP.example:443/p?x=1#top"));
    assert_eq!(a, b);
}

#[test]
fn legacy_record_key_differs_by_url_partition_and_protocol() {
    let base = entry_key(partition(1), &url("https://shop.example/p?x=1"));
    for other in [
        entry_key(partition(1), &url("https://shop.example/p?x=2")),
        entry_key(partition(1), &url("https://shop.example/p")),
        entry_key(partition(1), &url("https://shop.example/q?x=1")),
        entry_key(partition(1), &url("https://shop.example:8443/p?x=1")),
        entry_key(partition(1), &url("http://shop.example/p?x=1")),
        entry_key(partition(2), &url("https://shop.example/p?x=1")),
    ] {
        assert_ne!(base, other);
    }
    let rqp = cache_key(&KeyParts {
        partition: Partition::Scoped(partition(1)),
        origin: "https://shop.example",
        canonical_url: "https://shop.example/p?x=1",
        vary: &[],
        protocol: Protocol::Rqp,
        locale: None,
    });
    assert_ne!(base, rqp);
}

#[test]
fn legacy_record_key_keeps_every_query_parameter() {
    let a = entry_key(partition(1), &url("https://shop.example/p?utm=1&x=1"));
    let b = entry_key(partition(1), &url("https://shop.example/p?x=1"));
    assert_ne!(a, b);
}

// ---- vary_entries ----

#[test]
fn legacy_record_vary_without_a_vary_field_is_empty() {
    let session = [line("cookie", "sid=1")];
    assert_eq!(entries(&[], &session, &[]), Some(Vec::new()));
    assert_eq!(entries(&vary(" , "), &session, &[]), Some(Vec::new()));
}

#[test]
fn legacy_record_vary_star_gives_no_commit() {
    assert_eq!(entries(&vary("*"), &[], &[]), None);
    assert_eq!(entries(&vary("cookie, *"), &[], &[]), None);
    let two_lines = [line("vary", "cookie"), line("VARY", "*")];
    assert_eq!(entries(&two_lines, &[], &[]), None);
}

#[test]
fn legacy_record_vary_member_that_is_not_a_name_gives_no_commit() {
    assert_eq!(entries(&vary("cookie, a b"), &[], &[]), None);
    assert_eq!(entries(&vary("\"cookie\""), &[], &[]), None);
}

#[test]
fn legacy_record_vary_drops_the_three_legacy_request_names() {
    let session = [
        line("accept-diff", "true"),
        line("if-none-match", "abc"),
        line("template-tag", "def"),
    ];
    let response = vary("Accept-Diff, IF-NONE-MATCH, template-tag");
    assert_eq!(entries(&response, &session, &[]), Some(Vec::new()));
    let response = vary("template-tag, Cookie");
    let found = entries(&response, &session, &[]).unwrap();
    assert_eq!(
        found.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        ["cookie"]
    );
}

#[test]
fn legacy_record_vary_names_are_lower_case_sorted_and_unique() {
    let response = [
        line("Vary", "X-Theme, Accept-Language"),
        line("vary", "COOKIE, accept-language"),
    ];
    let found = entries(&response, &[], &[]).unwrap();
    let names: Vec<&str> = found.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["accept-language", "cookie", "x-theme"]);
}

#[test]
fn legacy_record_vary_absent_field_has_no_value() {
    let session = [line("cookie", "sid=1")];
    let found = entries(&vary("accept-language, cookie"), &session, &[]).unwrap();
    assert_eq!(found[0].name, "accept-language");
    assert_eq!(found[0].value, None);
    assert_eq!(found[1].value, Some(expected_digest("cookie", b"sid=1")));
}

#[test]
fn legacy_record_vary_empty_value_is_present() {
    let empty = digest_of("cookie", &[line("cookie", "")], &[]);
    assert_eq!(empty, Some(expected_digest("cookie", b"")));
    assert_ne!(empty, None);
    assert_ne!(empty, digest_of("cookie", &[line("cookie", " ")], &[]));
}

/// A known answer computed outside Rust (Python `hmac` and `openssl dgst
/// -mac HMAC`): key bytes 0x00 to 0x1f, name `cookie`, value
/// `sid=1, theme=dark`.
#[test]
fn legacy_record_vary_digest_known_answer() {
    let digest = digest_of(
        "cookie",
        &[line("Cookie", "sid=1")],
        &[line("cookie", "theme=dark")],
    )
    .unwrap();
    let hex = hex(digest.as_bytes());
    assert_eq!(
        hex,
        "0ec0e3607fed903d5ef6b4283efa0f989fa7b6d469bb97be462f7c1e40e47fe2"
    );
}

#[test]
fn legacy_record_vary_session_fields_come_before_platform_fields() {
    let session = [line("x-a", "1")];
    let platform = [line("x-a", "2")];
    assert_eq!(
        digest_of("x-a", &session, &platform),
        Some(expected_digest("x-a", b"1, 2"))
    );
    assert_eq!(
        digest_of("x-a", &platform, &session),
        Some(expected_digest("x-a", b"2, 1"))
    );
}

#[test]
fn legacy_record_vary_lines_of_one_name_join_in_order() {
    let session = [
        line("X-A", "1"),
        line("other", "z"),
        line("x-a", "2"),
        line("x-a", "3"),
    ];
    let platform = [line("x-b", "q"), line("X-a", "4")];
    let joined = digest_of("x-a", &session, &platform);
    assert_eq!(joined, Some(expected_digest("x-a", b"1, 2, 3, 4")));
    // One line with the joined text has the same value.
    assert_eq!(joined, digest_of("x-a", &[line("x-a", "1, 2, 3, 4")], &[]));
}

#[test]
fn legacy_record_vary_request_names_match_case_insensitively() {
    assert_eq!(
        digest_of("cookie", &[line("COOKIE", "v")], &[]),
        Some(expected_digest("cookie", b"v"))
    );
    assert_eq!(
        digest_of("cookie", &[line("cookie2", "v"), line("cooki", "v")], &[]),
        None
    );
}

#[test]
fn legacy_record_vary_digest_input_is_injective() {
    // The name has a length: ("a", "bc") and ("ab", "c") differ.
    let a = digest_of("a", &[line("a", "bc")], &[]).unwrap();
    let ab = digest_of("ab", &[line("ab", "c")], &[]).unwrap();
    assert_ne!(a, ab);
    // One value under two names differs.
    let x = digest_of("x-a", &[line("x-a", "v")], &[]).unwrap();
    let y = digest_of("x-b", &[line("x-b", "v")], &[]).unwrap();
    assert_ne!(x, y);
}

#[test]
fn legacy_record_vary_digest_depends_on_the_key() {
    let session = [line("cookie", "sid=1")];
    let values = RequestValues {
        session_fields: &session,
        platform_fields: &[],
    };
    let one = vary_entries(&vary("cookie"), &values, &vary_key()).unwrap();
    let other = vary_entries(&vary("cookie"), &values, &VaryKey(Secret::new([7; 32]))).unwrap();
    assert_ne!(one[0].value, other[0].value);
    assert!(one[0].value.is_some() && other[0].value.is_some());
}

// ---- Debug ----

const SECRET: &str = "s3cr3t-value";

#[test]
fn legacy_record_debug_of_request_values_has_counts_only() {
    let session = [line("cookie", SECRET)];
    let platform = [line("accept-language", SECRET), line("x", SECRET)];
    let text = format!(
        "{:?}",
        RequestValues {
            session_fields: &session,
            platform_fields: &platform,
        }
    );
    assert!(!text.contains(SECRET), "{text}");
    assert!(text.contains("session_fields: 1"), "{text}");
    assert!(text.contains("platform_fields: 2"), "{text}");
}

#[test]
fn legacy_record_debug_of_vary_entries_has_no_digest() {
    let found = entries(&vary("cookie"), &[line("cookie", SECRET)], &[]).unwrap();
    let digest = found[0].value.unwrap();
    let text = format!("{found:?}");
    let hex = hex(digest.as_bytes());
    assert!(!text.contains(&hex[..8]), "{text}");
    assert!(!text.contains(SECRET), "{text}");
    assert!(!format!("{:?}", vary_key()).contains("00, 01"));
}

#[test]
fn legacy_record_debug_of_records_has_lengths_only() {
    let page = format!("<p>{SECRET}</p>");
    let template = format!("<p>{{v}}{SECRET}</p>");
    let data = [DataBlock {
        key: "{v}".to_owned(),
        value: SECRET.to_owned(),
    }];
    let fields = [line("content-type", SECRET)];
    let response_fields = [line("cache-control", SECRET)];
    let new = NewRecord {
        host: "secret-host.example",
        vary: &[],
        page: &page,
        template: &template,
        data: &data,
        fields: &fields,
        etag: sha1_hex(page.as_bytes()),
        template_tag: sha1_hex(template.as_bytes()),
        response_fields: &response_fields,
        request_time: UnixTime(1),
        response_time: UnixTime(2),
    };
    let text = format!("{new:?}");
    assert!(!text.contains(SECRET), "{text}");
    assert!(!text.contains("secret-host"), "{text}");
    assert!(!text.contains(sha1_hex(page.as_bytes()).as_str()), "{text}");
    assert!(
        text.contains(&format!("page_len: {}", page.len())),
        "{text}"
    );

    let record = LegacyRecord {
        version: rivqen_cache::txn::EntryVersion(9),
        page: page.clone(),
        template: template.clone(),
        data: data.to_vec(),
        fields: fields.to_vec(),
        vary: Vec::new(),
        etag: sha1_hex(page.as_bytes()),
        template_tag: sha1_hex(template.as_bytes()),
    };
    let text = format!("{record:?}");
    assert!(!text.contains(SECRET), "{text}");
    assert!(
        !text.contains(&sha1_hex(page.as_bytes()).as_str()[..8]),
        "{text}"
    );
    assert!(text.contains("EntryVersion(9)"), "{text}");
    assert!(
        text.contains(&format!("template_len: {}", template.len())),
        "{text}"
    );
}

#[test]
fn legacy_record_debug_of_write_basis_has_short_ids_only() {
    let basis = WriteBasis {
        partition: partition(0xab),
        key: CacheKey::from_bytes([0xcd; 32]),
        partition_epoch: Epoch(1),
        site_epoch: Epoch(2),
        expect: Expect::Absent,
    };
    let text = format!("{basis:?}");
    assert!(!text.contains(&"cd".repeat(5)), "{text}");
    assert!(!text.contains(&"ab".repeat(5)), "{text}");
    assert_eq!(format!("{:?}", NotVisible::OtherVariant), "OtherVariant");
}
