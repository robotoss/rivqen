// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property tests of the Structured Field Values codec: no panic and a result
//! on any input up to 8 KiB, and serialize → parse round trips for the shapes
//! that RQP sends (Token Inner Lists, Dictionaries of `t` and `b`, Byte
//! Sequences, Integers) and for random values of every type.
//!
//! 256 cases by default; `PROPTEST_CASES` raises it (the long run of sprint
//! close).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use rivqen_proto::sf::{
    BareItem, Decimal, Dictionary, InnerList, Item, List, MAX_INTEGER, Member, Parameters,
    parse_dictionary, parse_item, parse_list, serialize_dictionary, serialize_item, serialize_list,
};

// --- Generators -----------------------------------------------------------

fn chars_from(set: &'static [u8], min: usize, max: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(set), min..=max)
        .prop_map(|v| v.into_iter().map(char::from).collect())
}

const KEY_REST: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_-.*";
const TOKEN_FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ*";
const TOKEN_REST: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&'*+-.^_`|~:/";
const B64U: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn key() -> impl Strategy<Value = String> {
    (
        prop::sample::select(b"abcdefghijklmnopqrstuvwxyz*".as_slice()),
        chars_from(KEY_REST, 0, 6),
    )
        .prop_map(|(f, r)| format!("{}{r}", char::from(f)))
}

fn token() -> impl Strategy<Value = String> {
    (
        prop::sample::select(TOKEN_FIRST),
        chars_from(TOKEN_REST, 0, 12),
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
        token().prop_map(BareItem::Token),
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

/// Field values made of the bytes that matter to the grammars, with long
/// runs, up to `max_len` bytes.
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

/// Standard base64 with padding, from the base64url encoder of
/// `rivqen-integrity` (an independent encoder).
fn std_b64(bytes: &[u8]) -> String {
    let mut s: String = rivqen_integrity::b64u_encode(bytes)
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            other => other,
        })
        .collect();
    while !s.len().is_multiple_of(4) {
        s.push('=');
    }
    s
}

fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(256)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

    // --- No panic: any input gives Ok or Err ------------------------------

    #[test]
    fn sf_prop_any_bytes_give_a_result(bytes in prop::collection::vec(any::<u8>(), 0..=8 * 1024)) {
        let s = String::from_utf8_lossy(&bytes);
        // A result of any kind is fine; a panic fails the property.
        let _item = parse_item(&s).is_ok();
        let _list = parse_list(&s).is_ok();
        let _dict = parse_dictionary(&s).is_ok();
    }

    #[test]
    fn sf_prop_hostile_values_give_a_result(s in hostile_value(8 * 1024)) {
        let _item = parse_item(&s).is_ok();
        let _list = parse_list(&s).is_ok();
        let _dict = parse_dictionary(&s).is_ok();
    }

    #[test]
    fn sf_prop_parse_error_offset_is_inside_the_input(s in hostile_value(1024)) {
        for e in [parse_item(&s).err(), parse_list(&s).err().or(parse_dictionary(&s).err())]
            .into_iter()
            .flatten()
        {
            prop_assert!(e.offset() <= s.len());
        }
    }

    // --- Round trips of the RQP shapes ------------------------------------

    #[test]
    fn sf_prop_token_inner_list_round_trip(
        tokens in prop::collection::vec(token(), 0..8),
    ) {
        let list: List = vec![Member::InnerList(InnerList {
            items: tokens.into_iter().map(|t| Item::new(BareItem::Token(t))).collect(),
            params: Parameters::new(),
        })];
        let text = serialize_list(&list).unwrap();
        prop_assert_eq!(parse_list(&text), Ok(list), "{}", text);
    }

    #[test]
    fn sf_prop_have_dictionary_round_trip(
        t in chars_from(B64U, 43, 43),
        b in prop::collection::vec(any::<u8>(), 0..=2048),
    ) {
        let d: Dictionary = [
            ("t".to_owned(), Member::Item(Item::new(BareItem::Token(format!("t1.{t}"))))),
            ("b".to_owned(), Member::Item(Item::new(BareItem::ByteSequence(b)))),
        ]
        .into_iter()
        .collect();
        let text = serialize_dictionary(&d).unwrap();
        prop_assert_eq!(parse_dictionary(&text), Ok(d), "{}", text);
    }

    #[test]
    fn sf_prop_byte_sequence_round_trip(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let item = Item::new(BareItem::ByteSequence(bytes.clone()));
        let text = serialize_item(&item).unwrap();
        prop_assert_eq!(&text, &format!(":{}:", std_b64(&bytes)));
        prop_assert_eq!(parse_item(&text), Ok(item));
    }

    #[test]
    fn sf_prop_byte_sequence_without_padding_gives_the_same_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let padded = std_b64(&bytes);
        let bare = padded.trim_end_matches('=');
        prop_assert_eq!(
            parse_item(&format!(":{bare}:")).map(|i| i.bare_item),
            Ok(BareItem::ByteSequence(bytes.clone()))
        );
        // DL-089: one `=` of two is an incomplete padding.
        if padded.ends_with("==") {
            let partial = format!(":{}:", &padded[..padded.len() - 1]);
            prop_assert!(parse_item(&partial).is_err(), "{}", partial);
        }
    }

    #[test]
    fn sf_prop_integer_round_trip(v in -MAX_INTEGER..=MAX_INTEGER) {
        let item = Item::new(BareItem::Integer(v));
        let text = serialize_item(&item).unwrap();
        prop_assert_eq!(&text, &v.to_string());
        prop_assert_eq!(parse_item(&text), Ok(item));
    }

    // --- Round trips of every type ----------------------------------------

    #[test]
    fn sf_prop_item_round_trip(i in item()) {
        let text = serialize_item(&i).unwrap();
        prop_assert_eq!(parse_item(&text), Ok(i), "{}", text);
    }

    #[test]
    fn sf_prop_list_round_trip(l in prop::collection::vec(member(), 0..5)) {
        let text = serialize_list(&l).unwrap();
        prop_assert_eq!(parse_list(&text), Ok(l), "{}", text);
    }

    #[test]
    fn sf_prop_dictionary_round_trip(
        d in prop::collection::vec((key(), member()), 0..5)
            .prop_map(|v| v.into_iter().collect::<Dictionary>())
    ) {
        let text = serialize_dictionary(&d).unwrap();
        prop_assert_eq!(parse_dictionary(&text), Ok(d), "{}", text);
    }
}
