// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property tests of the HTTP reader: no panic and the limits on any byte input
//! (seeds: `fixtures/malformed/seeds/fields`), and a request in the byte format of
//! the runner writer (`server/conformance/src/raw-http.mjs`, `serializeRequest`)
//! reads back with the same method, target and field lines.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::format_push_string,
    clippy::similar_names
)]

use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_reference::http::reader::{
    MAX_FIELD_LINES, MAX_HEAD_BYTES, ReadError, RequestHead, is_field_value_byte, is_token,
    read_head, trim_ows,
};

/// Read `bytes` and check the invariants that hold for every input. Returns the
/// result and the number of bytes the reader consumed.
fn read_checked(bytes: &[u8]) -> (Result<RequestHead, ReadError>, usize) {
    let mut input = bytes;
    let result = read_head(&mut input);
    let consumed = bytes.len() - input.len();
    assert!(consumed <= MAX_HEAD_BYTES, "consumed {consumed} bytes");
    match &result {
        Ok(head) => {
            assert!(head.fields.len() <= MAX_FIELD_LINES);
            assert!(is_token(head.method.as_bytes()));
            assert!(head.target.starts_with('/'));
            assert!(
                head.target
                    .bytes()
                    .all(|b| (0x21..=0x7e).contains(&b) && b != b'#')
            );
            for f in &head.fields {
                assert!(is_token(f.name.as_bytes()));
                assert!(f.value.bytes().all(is_field_value_byte));
                assert_eq!(trim_ows(f.value.as_bytes()), f.value.as_bytes());
            }
            assert!(bytes[..consumed].ends_with(b"\r\n\r\n"));
        }
        Err(ReadError::Reject(r)) => {
            assert!(
                [400, 414, 431, 501, 505].contains(&r.status),
                "status {}",
                r.status
            );
        }
        Err(ReadError::Closed) => assert_eq!(bytes, b""),
        Err(e) => panic!("a byte slice gives no I/O error: {e:?}"),
    }
    (result, consumed)
}

/// [`read_checked`] when only the invariants matter.
fn check(bytes: &[u8]) {
    let (_result, _consumed) = read_checked(bytes);
}

fn seeds() -> Vec<(String, Vec<u8>)> {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/malformed/seeds/fields");
    let mut seeds: Vec<(String, Vec<u8>)> = fs::read_dir(&dir)
        .expect("seed directory")
        .map(|e| {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read(&path).unwrap())
        })
        .collect();
    seeds.sort();
    assert!(seeds.len() >= 10, "at least 10 field seeds");
    seeds
}

fn request_with_value(value: &[u8]) -> Vec<u8> {
    let mut req = b"GET /catalog HTTP/1.1\r\nHost: shop.example.com\r\nX-Seed: ".to_vec();
    req.extend_from_slice(value);
    req.extend_from_slice(b"\r\n\r\n");
    req
}

/// The expected result of a seed value inside a valid request, by RFC 9110 §5.5.
fn value_is_valid(value: &[u8]) -> bool {
    value.iter().all(|&b| is_field_value_byte(b)) && std::str::from_utf8(value).is_ok()
}

#[test]
fn reader_seed_values_are_read_or_refused_by_rfc_9110() {
    for (name, seed) in seeds() {
        let (result, _) = read_checked(&request_with_value(&seed));
        if value_is_valid(&seed) {
            let head = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(head.fields[1].value.as_bytes(), trim_ows(&seed), "{name}");
        } else {
            match result {
                Err(ReadError::Reject(r)) => assert_eq!(r.status, 400, "{name}"),
                other => panic!("{name}: {other:?}"),
            }
        }
    }
}

/// A token of RFC 9110 §5.6.2.
fn token() -> impl Strategy<Value = String> {
    "[!#$%&'*+.^_`|~0-9A-Za-z-]{1,20}"
}

/// A field name that the runner can send without changing the framing.
fn field_name() -> impl Strategy<Value = String> {
    token().prop_filter("not a framing or Host field", |n| {
        !["host", "content-length", "transfer-encoding"]
            .iter()
            .any(|f| n.eq_ignore_ascii_case(f))
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        rng_seed: RngSeed::Fixed(0x0052_5150),
        ..ProptestConfig::default()
    })]

    #[test]
    fn reader_never_panics_on_any_bytes(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        check(&bytes);
    }

    #[test]
    fn reader_never_panics_after_a_valid_request_line(
        line in prop::sample::select(vec![
            &b"GET / HTTP/1.1\r\n"[..],
            b"POST /x?y HTTP/1.0\r\n",
            b"\r\nHEAD /a HTTP/1.1\r\nHost: a\r\n",
        ]),
        tail in prop::collection::vec(any::<u8>(), 0..4096),
    ) {
        // Half of the bytes become the bytes of the line framing (CR, LF, colon,
        // space), so that many inputs reach the field line rules.
        let framing = |b: u8| match b {
            0x00..=0x1f => b'\r',
            0x20..=0x3f => b'\n',
            0x40..=0x5f => b':',
            0x60..=0x7f => b' ',
            _ => b,
        };
        let mut bytes = line.to_vec();
        bytes.extend(tail.iter().enumerate().map(|(i, &b)| if i % 2 == 0 { framing(b) } else { b }));
        check(&bytes);
        bytes.extend_from_slice(b"\r\n\r\n");
        check(&bytes);
    }

    #[test]
    fn reader_mutated_seeds_follow_rfc_9110(
        pick in any::<prop::sample::Index>(),
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8),
    ) {
        let all = seeds();
        let (_, seed) = pick.get(&all);
        let mut value = seed.clone();
        for (at, byte) in edits {
            let i = at.index(value.len() + 1);
            value.insert(i, byte);
        }
        let bytes = request_with_value(&value);
        let (result, _) = read_checked(&bytes);
        // A CR or LF inside the value changes the line structure; the rest follow
        // the value rule exactly.
        if !value.contains(&b'\r') && !value.contains(&b'\n') {
            prop_assert_eq!(result.is_ok(), value_is_valid(&value));
        }
    }

    #[test]
    fn reader_reads_back_what_the_runner_writes(
        method in token(),
        path in "/[!-\"$-~]{0,40}",
        fields in prop::collection::vec((field_name(), "[\t -~]{0,60}"), 0..40),
    ) {
        // serializeRequest: `<method> <target> HTTP/1.1`, then `<name>: <value>`
        // for each line, then the empty line. The runner adds Host first.
        let mut bytes = format!("{method} {path} HTTP/1.1\r\nHost: shop.example.com\r\n");
        for (name, value) in &fields {
            bytes.push_str(&format!("{name}: {value}\r\n"));
        }
        bytes.push_str("\r\n");
        let (result, consumed) = read_checked(bytes.as_bytes());
        let head = result.unwrap();
        prop_assert_eq!(consumed, bytes.len());
        prop_assert_eq!(&head.method, &method);
        prop_assert_eq!(&head.target, &path);
        prop_assert_eq!(head.fields.len(), fields.len() + 1);
        for (got, (name, value)) in head.fields[1..].iter().zip(&fields) {
            prop_assert_eq!(&got.name, name);
            prop_assert_eq!(got.value.as_bytes(), trim_ows(value.as_bytes()));
        }
    }
}
