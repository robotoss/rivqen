// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property tests of the Structured Field Values parser and the request fields:
//! no panic on any input, linear time on long adversarial inputs, and a
//! serialize → parse round trip through a test-only serializer written from
//! RFC 9651 §4.1.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use proptest::prelude::*;
use rivqen_reference::model::{Field, Request};
use rivqen_reference::rqp::request::parse;
use rivqen_reference::rqp::sf::{
    BareItem, Decimal, Dictionary, InnerList, Item, List, MAX_INTEGER, Member, Parameters,
    parse_dictionary, parse_item, parse_list,
};

// --- Test-only serializer (RFC 9651 §4.1) ---------------------------------

fn ser_bare(out: &mut String, b: &BareItem) {
    match b {
        BareItem::Integer(v) => write!(out, "{v}").unwrap(),
        BareItem::Decimal(d) => {
            let t = d.thousandths();
            let sign = if t < 0 { "-" } else { "" };
            let abs = t.unsigned_abs();
            let mut frac = format!("{:03}", abs % 1000);
            while frac.len() > 1 && frac.ends_with('0') {
                frac.pop();
            }
            write!(out, "{sign}{}.{frac}", abs / 1000).unwrap();
        }
        BareItem::String(s) => {
            out.push('"');
            for c in s.chars() {
                if c == '"' || c == '\\' {
                    out.push('\\');
                }
                out.push(c);
            }
            out.push('"');
        }
        BareItem::Token(t) => out.push_str(t),
        BareItem::ByteSequence(bytes) => write!(out, ":{}:", STANDARD.encode(bytes)).unwrap(),
        BareItem::Boolean(v) => out.push_str(if *v { "?1" } else { "?0" }),
        BareItem::Date(v) => write!(out, "@{v}").unwrap(),
        BareItem::DisplayString(s) => {
            out.push_str("%\"");
            for &byte in s.as_bytes() {
                if byte == b'%' || byte == b'"' || !(0x20..=0x7e).contains(&byte) {
                    write!(out, "%{byte:02x}").unwrap();
                } else {
                    out.push(char::from(byte));
                }
            }
            out.push('"');
        }
        other => panic!("no serializer for {other:?}"),
    }
}

fn ser_params(out: &mut String, p: &Parameters) {
    for (k, v) in p.iter() {
        out.push(';');
        out.push_str(k);
        if *v != BareItem::Boolean(true) {
            out.push('=');
            ser_bare(out, v);
        }
    }
}

fn ser_item(out: &mut String, i: &Item) {
    ser_bare(out, &i.bare_item);
    ser_params(out, &i.params);
}

fn ser_member(out: &mut String, m: &Member) {
    match m {
        Member::Item(i) => ser_item(out, i),
        Member::InnerList(l) => {
            out.push('(');
            for (n, i) in l.items.iter().enumerate() {
                if n > 0 {
                    out.push(' ');
                }
                ser_item(out, i);
            }
            out.push(')');
            ser_params(out, &l.params);
        }
    }
}

fn ser_list(l: &List) -> String {
    let mut out = String::new();
    for (n, m) in l.iter().enumerate() {
        if n > 0 {
            out.push_str(", ");
        }
        ser_member(&mut out, m);
    }
    out
}

fn ser_dict(d: &Dictionary) -> String {
    let mut out = String::new();
    for (n, (k, m)) in d.iter().enumerate() {
        if n > 0 {
            out.push_str(", ");
        }
        out.push_str(k);
        match m {
            Member::Item(i) if i.bare_item == BareItem::Boolean(true) => {
                ser_params(&mut out, &i.params);
            }
            _ => {
                out.push('=');
                ser_member(&mut out, m);
            }
        }
    }
    out
}

// --- Generators -----------------------------------------------------------

fn chars_from(set: &'static [u8], min: usize, max: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(set), min..=max)
        .prop_map(|v| v.into_iter().map(char::from).collect())
}

const KEY_REST: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_-.*";
const TOKEN_FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ*";
const TOKEN_REST: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&'*+-.^_`|~:/";

fn key() -> impl Strategy<Value = String> {
    (
        prop::sample::select(b"abcdefghijklmnopqrstuvwxyz*".as_slice()),
        chars_from(KEY_REST, 0, 6),
    )
        .prop_map(|(f, r)| format!("{}{r}", char::from(f)))
}

fn bare_item() -> impl Strategy<Value = BareItem> {
    prop_oneof![
        (-MAX_INTEGER..=MAX_INTEGER).prop_map(BareItem::Integer),
        (-Decimal::MAX_THOUSANDTHS..=Decimal::MAX_THOUSANDTHS)
            .prop_map(|t| BareItem::Decimal(Decimal::from_thousandths(t).unwrap())),
        prop::collection::vec(0x20u8..=0x7e, 0..12)
            .prop_map(|v| BareItem::String(String::from_utf8(v).unwrap())),
        (
            prop::sample::select(TOKEN_FIRST),
            chars_from(TOKEN_REST, 0, 8)
        )
            .prop_map(|(f, r)| BareItem::Token(format!("{}{r}", char::from(f)))),
        prop::collection::vec(any::<u8>(), 0..24).prop_map(BareItem::ByteSequence),
        any::<bool>().prop_map(BareItem::Boolean),
        (-MAX_INTEGER..=MAX_INTEGER).prop_map(BareItem::Date),
        prop::collection::vec(any::<char>(), 0..8)
            .prop_map(|v| BareItem::DisplayString(v.into_iter().collect())),
    ]
}

fn params() -> impl Strategy<Value = Parameters> {
    prop::collection::vec((key(), bare_item()), 0..4).prop_map(|v| v.into_iter().collect())
}

fn item() -> impl Strategy<Value = Item> {
    (bare_item(), params()).prop_map(|(bare_item, params)| Item { bare_item, params })
}

fn member() -> impl Strategy<Value = Member> {
    prop_oneof![
        item().prop_map(Member::Item),
        (prop::collection::vec(item(), 0..4), params())
            .prop_map(|(items, params)| Member::InnerList(InnerList { items, params })),
    ]
}

/// Field values made of the bytes that matter to the grammars, with long runs.
fn hostile_value(max_len: usize) -> impl Strategy<Value = String> {
    const ALPHABET: &[u8] = b" \t,;=()\"\\:?@%*-.0123456789aAzZ/+_W'!~\x7f\x00\r\n";
    let unit = prop_oneof![
        3 => prop::sample::select(ALPHABET).prop_map(|c| char::from(c).to_string()),
        1 => (prop::sample::select(ALPHABET), 1..512usize)
            .prop_map(|(c, n)| char::from(c).to_string().repeat(n)),
        1 => Just("é".to_owned()),
    ];
    prop::collection::vec(unit, 0..64).prop_map(move |v| {
        let mut s: String = v.concat();
        while s.len() > max_len {
            s.pop();
        }
        s
    })
}

const NAMES: [&str; 11] = [
    "Rq-Version",
    "Rq-Capabilities",
    "Rq-Have",
    "Rq-Nonce",
    "If-None-Match",
    "Accept",
    "Range",
    "If-Match",
    "If-Range",
    "If-Unmodified-Since",
    "X-Other",
];

/// 512 cases by default; `PROPTEST_CASES` raises it (the long run of sprint close).
fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

    #[test]
    fn sf_prop_item_round_trip(i in item()) {
        let mut text = String::new();
        ser_item(&mut text, &i);
        prop_assert_eq!(parse_item(&text), Ok(i), "{}", text);
    }

    #[test]
    fn sf_prop_list_round_trip(l in prop::collection::vec(member(), 0..5)) {
        let text = ser_list(&l);
        prop_assert_eq!(parse_list(&text), Ok(l), "{}", text);
    }

    #[test]
    fn sf_prop_dictionary_round_trip(
        d in prop::collection::vec((key(), member()), 0..5)
            .prop_map(|v| v.into_iter().collect::<Dictionary>())
    ) {
        let text = ser_dict(&d);
        prop_assert_eq!(parse_dictionary(&text), Ok(d), "{}", text);
    }

    #[test]
    fn sf_prop_parsers_never_panic(s in hostile_value(16 * 1024)) {
        // A result of any kind is fine; a panic fails the property.
        let _item = parse_item(&s).is_ok();
        let _list = parse_list(&s).is_ok();
        let _dict = parse_dictionary(&s).is_ok();
    }

    #[test]
    fn req_prop_parse_never_panics(
        lines in prop::collection::vec(
            (prop::sample::select(NAMES.as_slice()), hostile_value(6 * 1024)),
            0..8,
        )
    ) {
        let r = parse(&Request {
            method: "GET".to_owned(),
            url: "https://shop.example.com/catalog".to_owned(),
            fields: lines.iter().map(|(n, v)| Field::new(n, v)).collect(),
        });
        // Invariants of the contract.
        if let Some(h) = &r.have {
            if let Some(d) = &h.digests {
                prop_assert_eq!(d.len() % 8, 0);
            }
        }
        if let Some(b) = &r.base {
            prop_assert_eq!(b.len(), 46);
            prop_assert!(b.starts_with("r1."));
        }
    }
}

// --- Linear time ----------------------------------------------------------

/// 64 KiB adversarial values: long runs of the characters that each bare item
/// type and each separator reads. A quadratic parse of one of them takes seconds;
/// the limit is far above a linear parse, also in a debug build.
fn adversarial_values() -> Vec<String> {
    let n = 64 * 1024;
    let run = |s: &str| s.repeat(n / s.len());
    vec![
        format!(":{}A:", "=".repeat(n)),
        format!(":{}", "A".repeat(n)),
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
        run("W/\"x\", "),
        run("application/vnd.rivqen.delta+json;q=0, "),
        format!("application/vnd.rivqen.delta+json;x=\"{}", run("\\\"")),
        format!("application/vnd.rivqen.delta+json{}", run(" ; ")),
        format!("\"{}", "r1.".repeat(n / 3)),
    ]
}

const LIMIT: Duration = Duration::from_millis(400);

#[test]
fn sf_parsers_are_linear_on_adversarial_values() {
    for value in adversarial_values() {
        let start = Instant::now();
        let _item = parse_item(&value).is_ok();
        let _list = parse_list(&value).is_ok();
        let _dict = parse_dictionary(&value).is_ok();
        let took = start.elapsed();
        assert!(took < LIMIT, "{took:?} for {:?}…", &value[..24]);
    }
}

#[test]
fn req_parse_is_linear_on_adversarial_values() {
    for value in adversarial_values() {
        let fields: Vec<Field> = NAMES.iter().map(|n| Field::new(n, &value)).collect();
        let start = Instant::now();
        let _r = parse(&Request {
            method: "GET".to_owned(),
            url: "https://shop.example.com/catalog".to_owned(),
            fields,
        });
        let took = start.elapsed();
        assert!(took < LIMIT, "{took:?} for {:?}…", &value[..24]);
    }
}

#[test]
fn req_parse_is_linear_in_the_number_of_lines() {
    // 8192 lines of each field: the combined values are built once.
    let mut fields = Vec::new();
    for name in NAMES {
        for _ in 0..8192 {
            fields.push(Field::new(name, "a"));
        }
    }
    let start = Instant::now();
    let _r = parse(&Request {
        method: "GET".to_owned(),
        url: "https://shop.example.com/catalog".to_owned(),
        fields,
    });
    let took = start.elapsed();
    assert!(took < LIMIT, "{took:?}");
}

#[test]
fn sf_many_repeated_keys_are_not_quadratic() {
    // 16 384 distinct and repeated keys in one Dictionary and in Parameters.
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
