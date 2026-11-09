// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Linear time of the Structured Field Values parser (DL-063; negotiation
//! section 7).
//!
//! The `sf_linear_release_*` tests measure 256 KiB inputs against 50 ms. They
//! need a release build, so they are ignored by default. Run them with:
//!
//! ```text
//! RUST_TEST_THREADS=1 cargo test -p rivqen-proto --release --test sf_linear -- --ignored sf_linear
//! ```
//!
//! The `sf_linear_debug_*` tests run in every build with 64 KiB inputs and a
//! wide limit: a quadratic parse of these inputs takes seconds.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use rivqen_proto::sf::{BareItem, parse_dictionary, parse_item, parse_list};

const KIB_256: usize = 256 * 1024;

/// Adversarial values of about `n` bytes: long runs of the characters that
/// each bare item type and each separator reads.
fn adversarial_values(n: usize) -> Vec<String> {
    let run = |s: &str| s.repeat(n / s.len());
    vec![
        format!(":{}A:", "=".repeat(n)),
        format!(":{}", "A".repeat(n)),
        format!(":{}=", "A".repeat(n)),
        format!("\"{}", run("\\\"")),
        format!("%\"{}", run("%c3%bc")),
        format!("1{}", run(";a")),
        format!("1{}", run(";a=1")),
        run("a=1, "),
        run("a, "),
        format!("({}", "a ".repeat(n / 2)),
        format!("(){}", run(";a")),
        " ".repeat(n),
        format!("1{}", " ".repeat(n)),
        format!("{}1", " ".repeat(n)),
        format!("a{}", "\t".repeat(n)),
        format!("a,{}b", " \t".repeat(n / 2)),
        run("1"),
        format!("1.{}", "0".repeat(n)),
        run("abcdefghijklmnopqrstuvwxyz0123456789*-._ "),
        run("*a"),
        run("\""),
        format!("{}é", "a".repeat(n)),
    ]
}

/// Parse `value` with all three parsers; the time of the slowest one.
fn slowest_parse(value: &str) -> Duration {
    let mut worst = Duration::ZERO;
    for parse in [
        |v: &str| parse_item(v).is_ok(),
        |v: &str| parse_list(v).is_ok(),
        |v: &str| parse_dictionary(v).is_ok(),
    ] {
        let start = Instant::now();
        let _ok = parse(value);
        worst = worst.max(start.elapsed());
    }
    worst
}

fn byte_sequence_256k() -> String {
    // 256 KiB of base64 content with complete padding.
    format!(":{}AA==:", "AAAA".repeat(KIB_256 / 4 - 1))
}

fn list_members_256k() -> String {
    let mut s = String::with_capacity(KIB_256 + 16);
    let mut i = 0_u32;
    while s.len() < KIB_256 {
        if i > 0 {
            s.push_str(", ");
        }
        write!(s, "m{i};p=?0").unwrap();
        i += 1;
    }
    s
}

#[test]
fn sf_linear_inputs_parse_as_expected() {
    let bs = parse_item(&byte_sequence_256k()).unwrap();
    let BareItem::ByteSequence(bytes) = bs.bare_item else {
        panic!("not a byte sequence");
    };
    assert_eq!(bytes.len(), (KIB_256 / 4 - 1) * 3 + 1);
    assert!(parse_list(&list_members_256k()).unwrap().len() > 20_000);
}

#[test]
fn sf_linear_debug_adversarial_values() {
    const LIMIT: Duration = Duration::from_secs(2);
    for value in adversarial_values(64 * 1024) {
        let took = slowest_parse(&value);
        assert!(took < LIMIT, "{took:?} for {:?}…", &value[..24]);
    }
}

#[test]
fn sf_linear_debug_repeated_keys() {
    const LIMIT: Duration = Duration::from_secs(2);
    // 16 384 members and parameters, 8 192 distinct keys, each one twice.
    let mut dict = String::new();
    let mut item = String::from("1");
    for i in 0..16_384u32 {
        if i > 0 {
            dict.push_str(", ");
        }
        write!(dict, "k{}=1", i % 8192).unwrap();
        write!(item, ";k{}", i % 8192).unwrap();
    }
    let start = Instant::now();
    assert_eq!(parse_dictionary(&dict).unwrap().len(), 8192);
    assert_eq!(parse_item(&item).unwrap().params.len(), 8192);
    let took = start.elapsed();
    assert!(took < LIMIT, "{took:?}");
}

const RELEASE_LIMIT: Duration = Duration::from_millis(50);

#[test]
#[ignore = "release build only: cargo test --release -- --ignored sf_linear"]
fn sf_linear_release_byte_sequence_256k() {
    let value = byte_sequence_256k();
    let took = slowest_parse(&value);
    assert!(took < RELEASE_LIMIT, "{took:?}");
}

#[test]
#[ignore = "release build only: cargo test --release -- --ignored sf_linear"]
fn sf_linear_release_list_members_256k() {
    let value = list_members_256k();
    let took = slowest_parse(&value);
    assert!(took < RELEASE_LIMIT, "{took:?}");
}

#[test]
#[ignore = "release build only: cargo test --release -- --ignored sf_linear"]
fn sf_linear_release_adversarial_values_256k() {
    for value in adversarial_values(KIB_256) {
        let took = slowest_parse(&value);
        assert!(took < RELEASE_LIMIT, "{took:?} for {:?}…", &value[..24]);
    }
}
