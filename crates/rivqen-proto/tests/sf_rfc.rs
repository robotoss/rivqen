// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! RFC 9651 test vectors written from the examples of RFC 9651 §3 and §4.2
//! (`sf_rfc_*`), one test per failure rule of the parsing steps of §4.2
//! (`sf_fail_*`), the Byte Sequence padding rule of DL-089 item 7
//! (`sf_byte_sequence_padding_*`) and the serialization of §4.1
//! (`sf_serialize_*`). Ported from `server/rust-reference/tests/sf_rfc.rs`
//! (Rivqen code).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_proto::sf::{
    BareItem, Decimal, Dictionary, InnerList, Item, List, MAX_INTEGER, Member, Parameters,
    parse_dictionary, parse_item, parse_list, serialize_dictionary, serialize_item, serialize_list,
};

fn token(s: &str) -> BareItem {
    BareItem::Token(s.to_owned())
}

fn string(s: &str) -> BareItem {
    BareItem::String(s.to_owned())
}

fn decimal(thousandths: i64) -> BareItem {
    BareItem::Decimal(Decimal::from_thousandths(thousandths).unwrap())
}

fn params(entries: &[(&str, BareItem)]) -> Parameters {
    entries
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn item(bare_item: BareItem) -> Item {
    Item {
        bare_item,
        params: Parameters::new(),
    }
}

fn item_p(bare_item: BareItem, p: &[(&str, BareItem)]) -> Item {
    Item {
        bare_item,
        params: params(p),
    }
}

fn m_item(bare_item: BareItem) -> Member {
    Member::Item(item(bare_item))
}

fn inner(items: Vec<Item>, p: &[(&str, BareItem)]) -> Member {
    Member::InnerList(InnerList {
        items,
        params: params(p),
    })
}

fn bare(input: &str) -> BareItem {
    parse_item(input)
        .unwrap_or_else(|e| panic!("{input:?}: {e}"))
        .bare_item
}

fn dict_entries(input: &str) -> Vec<(String, Member)> {
    parse_dictionary(input)
        .unwrap_or_else(|e| panic!("{input:?}: {e}"))
        .iter()
        .map(|(k, v)| (k.to_owned(), v.clone()))
        .collect()
}

fn entries(e: Vec<(&str, Member)>) -> Vec<(String, Member)> {
    e.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

// --- RFC 9651 §3 examples -------------------------------------------------

#[test]
fn sf_rfc_3_1_list_of_tokens() {
    assert_eq!(
        parse_list("sugar, tea, rum").unwrap(),
        vec![
            m_item(token("sugar")),
            m_item(token("tea")),
            m_item(token("rum"))
        ]
    );
}

#[test]
fn sf_rfc_3_1_list_on_two_lines_is_the_same() {
    // "Example-List: sugar, tea" and "Example-List: rum", joined with ", ".
    assert_eq!(
        parse_list("sugar, tea, rum").unwrap(),
        parse_list(&["sugar, tea", "rum"].join(", ")).unwrap()
    );
}

#[test]
fn sf_rfc_3_1_1_inner_lists() {
    assert_eq!(
        parse_list(r#"("foo" "bar"), ("baz"), ("bat" "one"), ()"#).unwrap(),
        vec![
            inner(vec![item(string("foo")), item(string("bar"))], &[]),
            inner(vec![item(string("baz"))], &[]),
            inner(vec![item(string("bat")), item(string("one"))], &[]),
            inner(vec![], &[]),
        ]
    );
}

#[test]
fn sf_rfc_3_1_1_inner_lists_with_parameters() {
    assert_eq!(
        parse_list(r#"("foo"; a=1;b=2);lvl=5, ("bar" "baz");lvl=1"#).unwrap(),
        vec![
            inner(
                vec![item_p(
                    string("foo"),
                    &[("a", BareItem::Integer(1)), ("b", BareItem::Integer(2))]
                )],
                &[("lvl", BareItem::Integer(5))]
            ),
            inner(
                vec![item(string("bar")), item(string("baz"))],
                &[("lvl", BareItem::Integer(1))]
            ),
        ]
    );
}

#[test]
fn sf_rfc_3_1_2_parameters() {
    assert_eq!(
        parse_list(r#"abc;a=1;b=2; cde_456, (ghi;jk=4 l);q="9";r=w"#).unwrap(),
        vec![
            Member::Item(item_p(
                token("abc"),
                &[
                    ("a", BareItem::Integer(1)),
                    ("b", BareItem::Integer(2)),
                    ("cde_456", BareItem::Boolean(true)),
                ]
            )),
            inner(
                vec![
                    item_p(token("ghi"), &[("jk", BareItem::Integer(4))]),
                    item(token("l")),
                ],
                &[("q", string("9")), ("r", token("w"))]
            ),
        ]
    );
}

#[test]
fn sf_rfc_3_1_2_boolean_parameters() {
    assert_eq!(
        parse_item("1; a; b=?0").unwrap(),
        item_p(
            BareItem::Integer(1),
            &[
                ("a", BareItem::Boolean(true)),
                ("b", BareItem::Boolean(false))
            ]
        )
    );
}

#[test]
fn sf_rfc_3_2_dictionary() {
    assert_eq!(
        dict_entries(r#"en="Applepie", da=:w4ZibGV0w6ZydGU=:"#),
        entries(vec![
            ("en", m_item(string("Applepie"))),
            (
                "da",
                m_item(BareItem::ByteSequence("Æbletærte".as_bytes().to_vec()))
            ),
        ])
    );
}

#[test]
fn sf_rfc_3_2_dictionary_boolean_members() {
    assert_eq!(
        dict_entries("a=?0, b, c; foo=bar"),
        entries(vec![
            ("a", m_item(BareItem::Boolean(false))),
            ("b", m_item(BareItem::Boolean(true))),
            (
                "c",
                Member::Item(item_p(BareItem::Boolean(true), &[("foo", token("bar"))]))
            ),
        ])
    );
}

#[test]
fn sf_rfc_3_2_dictionary_decimal_and_inner_list() {
    assert_eq!(
        dict_entries("rating=1.5, feelings=(joy sadness)"),
        entries(vec![
            ("rating", m_item(decimal(1500))),
            (
                "feelings",
                inner(vec![item(token("joy")), item(token("sadness"))], &[])
            ),
        ])
    );
}

#[test]
fn sf_rfc_3_2_dictionary_mixed() {
    assert_eq!(
        dict_entries("a=(1 2), b=3, c=4;aa=bb, d=(5 6);valid"),
        entries(vec![
            (
                "a",
                inner(
                    vec![item(BareItem::Integer(1)), item(BareItem::Integer(2))],
                    &[]
                )
            ),
            ("b", m_item(BareItem::Integer(3))),
            (
                "c",
                Member::Item(item_p(BareItem::Integer(4), &[("aa", token("bb"))]))
            ),
            (
                "d",
                inner(
                    vec![item(BareItem::Integer(5)), item(BareItem::Integer(6))],
                    &[("valid", BareItem::Boolean(true))]
                )
            ),
        ])
    );
}

#[test]
fn sf_rfc_3_2_dictionary_on_two_lines_is_the_same() {
    assert_eq!(
        parse_dictionary("foo=1, bar=2").unwrap(),
        parse_dictionary(&["foo=1", "bar=2"].join(", ")).unwrap()
    );
}

#[test]
fn sf_rfc_3_3_items() {
    assert_eq!(parse_item("5").unwrap(), item(BareItem::Integer(5)));
    assert_eq!(
        parse_item("5; foo=bar").unwrap(),
        item_p(BareItem::Integer(5), &[("foo", token("bar"))])
    );
}

#[test]
fn sf_rfc_3_3_bare_item_examples() {
    assert_eq!(bare("42"), BareItem::Integer(42));
    assert_eq!(bare("4.5"), decimal(4500));
    assert_eq!(bare(r#""hello world""#), string("hello world"));
    assert_eq!(bare("foo123/456"), token("foo123/456"));
    assert_eq!(
        bare(":cHJldGVuZCB0aGlzIGlzIGJpbmFyeSBjb250ZW50Lg==:"),
        BareItem::ByteSequence(b"pretend this is binary content.".to_vec())
    );
    assert_eq!(bare("?1"), BareItem::Boolean(true));
    assert_eq!(bare("@1659578233"), BareItem::Date(1_659_578_233));
    assert_eq!(
        bare(r#"%"This is intended for display to %c3%bcsers.""#),
        BareItem::DisplayString("This is intended for display to üsers.".to_owned())
    );
}

#[test]
fn sf_rfc_4_2_string_split_over_two_lines_keeps_the_join() {
    // RFC 9651 §4.2: a String split over two lines gets the join inside it.
    assert_eq!(bare(&["\"foo", "bar\""].join(", ")), string("foo, bar"));
}

// --- RFC 9651 §4.2 top-level steps ---------------------------------------

#[test]
fn sf_rfc_4_2_top_level_discards_spaces_only() {
    assert_eq!(bare("  1  "), BareItem::Integer(1));
    assert!(parse_item("\t1").is_err());
    assert!(parse_item("1\t").is_err());
    assert_eq!(parse_list("   ").unwrap(), vec![]);
    assert!(parse_dictionary("  ").unwrap().is_empty());
}

#[test]
fn sf_fail_4_2_non_ascii_and_trailing_text() {
    assert!(parse_item("\"é\"").is_err());
    assert!(parse_list("a, é").is_err());
    assert!(parse_item("1 2").is_err());
    assert!(parse_item("").is_err());
}

#[test]
fn sf_fail_reports_the_offset_and_no_content() {
    let e = parse_list("a, b c").unwrap_err();
    assert_eq!(e.offset(), 5);
    assert_ne!(e.reason(), "");
    assert!(e.to_string().contains("byte 5"));
}

// --- §4.2.1 List ----------------------------------------------------------

#[test]
fn sf_rfc_4_2_1_list_separators_take_ows() {
    assert_eq!(
        parse_list("a,b \t,\t c").unwrap(),
        vec![m_item(token("a")), m_item(token("b")), m_item(token("c"))]
    );
}

#[test]
fn sf_fail_4_2_1_list_trailing_or_missing_comma() {
    assert!(parse_list("a,").is_err());
    assert!(parse_list("a, ").is_err());
    assert!(parse_list("a, , b").is_err());
    assert!(parse_list(", a").is_err());
    assert!(parse_list("a b").is_err());
    assert!(parse_list("a;b c").is_err());
}

// --- §4.2.1.2 Inner List --------------------------------------------------

#[test]
fn sf_rfc_4_2_1_2_inner_list_spaces() {
    assert_eq!(
        parse_list("(  a   b  )").unwrap(),
        vec![inner(vec![item(token("a")), item(token("b"))], &[])]
    );
}

#[test]
fn sf_fail_4_2_1_2_inner_list() {
    assert!(parse_list("(a b").is_err(), "no ')'");
    assert!(parse_list("(").is_err(), "no ')'");
    assert!(parse_list("(a,b)").is_err(), "comma inside");
    assert!(parse_list("(a\tb)").is_err(), "HTAB is not SP");
    assert!(parse_list("(\ta)").is_err(), "HTAB is not SP");
    assert!(parse_list("((a))").is_err(), "no nesting");
    assert!(parse_list("(a)b").is_err(), "text after ')'");
    assert!(parse_item("(a)").is_err(), "an Item is not an Inner List");
}

// --- §4.2.2 Dictionary ----------------------------------------------------

#[test]
fn sf_rfc_4_2_2_duplicate_key_keeps_last_value_at_first_position() {
    assert_eq!(
        dict_entries("a=1, b=2, a=3"),
        entries(vec![
            ("a", m_item(BareItem::Integer(3))),
            ("b", m_item(BareItem::Integer(2))),
        ])
    );
}

#[test]
fn sf_fail_4_2_2_dictionary() {
    assert!(parse_dictionary("a=1,").is_err(), "trailing comma");
    assert!(parse_dictionary("a=1 b=2").is_err(), "no comma");
    assert!(parse_dictionary("A=1").is_err(), "uppercase key");
    assert!(parse_dictionary("1a=1").is_err(), "digit first");
    assert!(parse_dictionary("a=").is_err(), "no value");
    assert!(parse_dictionary("a = 1").is_err(), "space before '='");
    assert!(parse_dictionary("a= 1").is_err(), "space after '='");
}

#[test]
fn sf_rfc_4_2_3_3_key_characters() {
    let d = parse_dictionary("*a0_-.*=1, z=2").unwrap();
    assert_eq!(d.get("*a0_-.*"), Some(&m_item(BareItem::Integer(1))));
    assert_eq!(d.get("z"), Some(&m_item(BareItem::Integer(2))));
    assert_eq!(d.get("y"), None);
    assert_eq!(d.len(), 2);
    assert!(!d.is_empty());
}

// --- §4.2.3.2 Parameters --------------------------------------------------

#[test]
fn sf_rfc_4_2_3_2_parameter_duplicate_key_keeps_last_value() {
    let i = parse_item("1;a=1;b;a=?0").unwrap();
    let got: Vec<(&str, &BareItem)> = i.params.iter().collect();
    assert_eq!(
        got,
        vec![
            ("a", &BareItem::Boolean(false)),
            ("b", &BareItem::Boolean(true))
        ]
    );
}

#[test]
fn sf_fail_4_2_3_2_parameters() {
    assert_eq!(
        parse_item("1;  a=2").unwrap(),
        item_p(BareItem::Integer(1), &[("a", BareItem::Integer(2))])
    );
    assert!(parse_item("1 ;a").is_err());
    assert!(parse_item("1;a =2").is_err());
    assert!(parse_item("1;a=(1)").is_err(), "a parameter value is bare");
    assert!(parse_item("1;").is_err(), "no key");
    assert!(parse_item("1;A").is_err(), "uppercase key");
}

// --- §4.2.4 Integer and Decimal -------------------------------------------

#[test]
fn sf_rfc_4_2_4_integer_limits() {
    assert_eq!(
        bare("999999999999999"),
        BareItem::Integer(999_999_999_999_999)
    );
    assert_eq!(
        bare("-999999999999999"),
        BareItem::Integer(-999_999_999_999_999)
    );
    assert!(parse_item("1000000000000000").is_err(), "16 digits");
    assert_eq!(bare("-0"), BareItem::Integer(0));
    assert_eq!(bare("007"), BareItem::Integer(7));
}

#[test]
fn sf_fail_4_2_4_integer() {
    assert!(parse_item("-").is_err());
    assert!(parse_item("-a").is_err());
    assert!(parse_item("--1").is_err());
    assert!(parse_item("+1").is_err());
    assert!(parse_item("1a").is_err());
}

#[test]
fn sf_fail_4_2_4_decimal_values_and_limits() {
    assert_eq!(bare("1.5"), decimal(1500));
    assert_eq!(bare("-1.25"), decimal(-1250));
    assert_eq!(bare("0.001"), decimal(1));
    assert_eq!(bare("1.000"), decimal(1000));
    assert_eq!(bare("123456789012.345"), decimal(123_456_789_012_345));
    assert_eq!(bare("-999999999999.999"), decimal(-999_999_999_999_999));
    assert!(parse_item("1234567890123.4").is_err(), "13 integer digits");
    assert!(parse_item("1.2345").is_err(), "4 fractional digits");
    assert!(parse_item("1.").is_err(), "ends with '.'");
    assert!(parse_item("1..2").is_err());
    assert!(parse_item("1.2.3").is_err());
    assert!(parse_item(".5").is_err());
}

#[test]
fn sf_rfc_3_3_2_decimal_range_constructor() {
    assert!(Decimal::from_thousandths(Decimal::MAX_THOUSANDTHS).is_some());
    assert!(Decimal::from_thousandths(-Decimal::MAX_THOUSANDTHS).is_some());
    assert!(Decimal::from_thousandths(Decimal::MAX_THOUSANDTHS + 1).is_none());
    assert!(Decimal::from_thousandths(-Decimal::MAX_THOUSANDTHS - 1).is_none());
    assert_eq!(Decimal::from_thousandths(42).unwrap().thousandths(), 42);
}

// --- §4.2.5 String --------------------------------------------------------

#[test]
fn sf_rfc_4_2_5_string_escapes() {
    assert_eq!(bare(r#""a\"b\\c""#), string(r#"a"b\c"#));
    assert_eq!(bare(r#""""#), string(""));
    assert_eq!(bare("\" ~\""), string(" ~"));
}

#[test]
fn sf_fail_4_2_5_string() {
    assert!(parse_item(r#""a\b""#).is_err(), "only \\\" and \\\\ escape");
    assert!(parse_item(r#""abc"#).is_err(), "no closing quote");
    assert!(parse_item("\"a\\").is_err(), "escape at the end");
    assert!(parse_item("\"a\tb\"").is_err(), "control character");
    assert!(parse_item("\"a\u{7f}b\"").is_err(), "DEL");
}

// --- §4.2.6 Token ---------------------------------------------------------

#[test]
fn sf_fail_4_2_6_token_characters() {
    assert_eq!(
        bare("*Foo!#$%&'*+-.^_`|~:/9"),
        token("*Foo!#$%&'*+-.^_`|~:/9")
    );
    assert!(parse_item("a(b").is_err());
    assert!(parse_item("a\"b").is_err());
    assert!(parse_item("a,b").is_err());
}

// --- §4.2.7 Byte Sequence (DL-089 item 7) ---------------------------------

#[test]
fn sf_byte_sequence_padding_absent_or_complete() {
    assert_eq!(bare("::"), BareItem::ByteSequence(vec![]));
    assert_eq!(bare(":AA==:"), BareItem::ByteSequence(vec![0]));
    assert_eq!(bare(":AA:"), BareItem::ByteSequence(vec![0]));
    assert_eq!(bare(":AAA=:"), BareItem::ByteSequence(vec![0, 0]));
    assert_eq!(bare(":AAA:"), BareItem::ByteSequence(vec![0, 0]));
    assert_eq!(bare(":AAAA:"), BareItem::ByteSequence(vec![0, 0, 0]));
    assert_eq!(
        bare(":+/+/:"),
        BareItem::ByteSequence(vec![0xfb, 0xff, 0xbf])
    );
}

#[test]
fn sf_byte_sequence_padding_non_zero_pad_bits_accepted() {
    assert_eq!(bare(":AB==:"), BareItem::ByteSequence(vec![0]));
    assert_eq!(bare(":AAB:"), BareItem::ByteSequence(vec![0, 0]));
}

#[test]
fn sf_byte_sequence_padding_partial_or_wrong_rejected() {
    for bad in [
        ":AA=:",
        ":A=:",
        ":A==:",
        ":A===:",
        ":AAA==:",
        ":AAAA==:",
        ":=:",
        ":==:",
        ":====:",
        ":AA=A:",
        ":=AAA:",
        ":AA==AA==:",
    ] {
        assert!(parse_item(bad).is_err(), "{bad}");
    }
}

#[test]
fn sf_fail_4_2_7_byte_sequence_content() {
    assert!(parse_item(":A:").is_err(), "length 4k + 1");
    assert!(parse_item(":AAAAA:").is_err(), "length 4k + 1");
    assert!(parse_item(":AA-_:").is_err(), "base64url alphabet");
    assert!(parse_item(":AA AA:").is_err(), "space");
    assert!(parse_item(":AAAA").is_err(), "no closing ':'");
    assert!(parse_item(":").is_err(), "no closing ':'");
}

// --- §4.2.8 Boolean, §4.2.9 Date, §4.2.10 Display String ------------------

#[test]
fn sf_fail_4_2_8_boolean() {
    assert_eq!(bare("?0"), BareItem::Boolean(false));
    assert!(parse_item("?").is_err());
    assert!(parse_item("?2").is_err());
    assert!(parse_item("?10").is_err());
    assert!(parse_item("?true").is_err());
}

#[test]
fn sf_fail_4_2_9_date() {
    assert_eq!(bare("@-1"), BareItem::Date(-1));
    assert_eq!(bare("@0"), BareItem::Date(0));
    assert!(parse_item("@1.5").is_err(), "a decimal");
    assert!(parse_item("@").is_err());
    assert!(parse_item("@a").is_err());
    assert!(parse_item("@1000000000000000").is_err(), "16 digits");
}

#[test]
fn sf_rfc_4_2_10_display_string_values() {
    assert_eq!(bare(r#"%"""#), BareItem::DisplayString(String::new()));
    assert_eq!(
        bare(r#"%"%25%22 x""#),
        BareItem::DisplayString("%\" x".to_owned())
    );
    assert_eq!(
        bare(r#"%"%e2%82%ac""#),
        BareItem::DisplayString("€".to_owned())
    );
}

#[test]
fn sf_fail_4_2_10_display_string() {
    assert!(parse_item(r#"%"%C3%BC""#).is_err(), "uppercase hex");
    assert!(parse_item(r#"%"%c""#).is_err(), "one hex digit");
    assert!(parse_item(r#"%"%g0""#).is_err(), "not hex");
    assert!(parse_item(r#"%"%0g""#).is_err(), "not hex");
    assert!(parse_item(r#"%"%c3""#).is_err(), "not UTF-8");
    assert!(parse_item(r#"%"%ed%a0%80""#).is_err(), "surrogate");
    assert!(parse_item(r#"%"abc"#).is_err(), "no closing quote");
    assert!(parse_item("%\"a\tb\"").is_err(), "control character");
    assert!(parse_item("%abc").is_err(), "no quote");
    assert!(parse_item("%").is_err());
}

#[test]
fn sf_fail_4_2_3_1_unknown_bare_item_start() {
    for bad in ["!a", "#", "&", "(", ")", "=", "/", "_a", "."] {
        assert!(parse_item(bad).is_err(), "{bad}");
    }
}

#[test]
fn sf_fail_4_2_10_display_string_del_and_non_ascii() {
    assert!(parse_item("%\"a\u{7f}b\"").is_err(), "DEL");
    assert!(parse_item("%\"\u{fc}\"").is_err(), "non-ASCII byte");
}

#[test]
fn sf_fail_errors_hold_no_field_content() {
    let marker = "SECRETMARKER";
    for input in [
        format!("\"{marker}"),
        format!("{marker} x"),
        format!(":{marker}=:"),
    ] {
        let e = parse_item(&input).unwrap_err();
        assert!(!e.to_string().contains(marker), "{e}");
        assert!(!format!("{e:?}").contains(marker), "{e:?}");
    }
}

// --- RFC 9651 §4.1 serialization -----------------------------------------

/// Parse, serialize, and check the canonical text.
fn canonical_list(input: &str, expected: &str) {
    assert_eq!(
        serialize_list(&parse_list(input).unwrap()).unwrap(),
        expected
    );
}

fn canonical_dict(input: &str, expected: &str) {
    assert_eq!(
        serialize_dictionary(&parse_dictionary(input).unwrap()).unwrap(),
        expected
    );
}

fn canonical_item(input: &str, expected: &str) {
    assert_eq!(
        serialize_item(&parse_item(input).unwrap()).unwrap(),
        expected
    );
}

#[test]
fn sf_serialize_rfc_3_examples() {
    canonical_list("sugar, tea, rum", "sugar, tea, rum");
    canonical_list(
        r#"("foo" "bar"), ("baz"), ("bat" "one"), ()"#,
        r#"("foo" "bar"), ("baz"), ("bat" "one"), ()"#,
    );
    canonical_list(
        r#"("foo"; a=1;b=2);lvl=5, ("bar" "baz");lvl=1"#,
        r#"("foo";a=1;b=2);lvl=5, ("bar" "baz");lvl=1"#,
    );
    canonical_list(
        r#"abc;a=1;b=2; cde_456, (ghi;jk=4 l);q="9";r=w"#,
        r#"abc;a=1;b=2;cde_456, (ghi;jk=4 l);q="9";r=w"#,
    );
    canonical_dict(
        r#"en="Applepie", da=:w4ZibGV0w6ZydGU=:"#,
        r#"en="Applepie", da=:w4ZibGV0w6ZydGU=:"#,
    );
    canonical_dict("a=?0, b, c; foo=bar", "a=?0, b, c;foo=bar");
    canonical_dict(
        "rating=1.5, feelings=(joy sadness)",
        "rating=1.5, feelings=(joy sadness)",
    );
    canonical_dict(
        "a=(1 2), b=3, c=4;aa=bb, d=(5 6);valid",
        "a=(1 2), b=3, c=4;aa=bb, d=(5 6);valid",
    );
    canonical_item("5; foo=bar", "5;foo=bar");
    canonical_item("1; a; b=?0", "1;a;b=?0");
    for bare in [
        "42",
        "4.5",
        r#""hello world""#,
        "foo123/456",
        ":cHJldGVuZCB0aGlzIGlzIGJpbmFyeSBjb250ZW50Lg==:",
        "?1",
        "?0",
        "@1659578233",
        r#"%"This is intended for display to %c3%bcsers.""#,
    ] {
        canonical_item(bare, bare);
    }
}

#[test]
fn sf_serialize_empty_list_and_dictionary_give_no_text() {
    assert_eq!(serialize_list(&List::new()).unwrap(), "");
    assert_eq!(serialize_dictionary(&Dictionary::new()).unwrap(), "");
}

#[test]
fn sf_serialize_integer_and_date_range() {
    assert_eq!(
        serialize_item(&item(BareItem::Integer(-MAX_INTEGER))).unwrap(),
        "-999999999999999"
    );
    assert_eq!(serialize_item(&item(BareItem::Date(-1))).unwrap(), "@-1");
    assert!(serialize_item(&item(BareItem::Integer(MAX_INTEGER + 1))).is_err());
    assert!(serialize_item(&item(BareItem::Integer(-MAX_INTEGER - 1))).is_err());
    assert!(serialize_item(&item(BareItem::Date(MAX_INTEGER + 1))).is_err());
}

#[test]
fn sf_serialize_decimal_keeps_one_to_three_fraction_digits() {
    for (thousandths, text) in [
        (1500, "1.5"),
        (-250, "-0.25"),
        (1000, "1.0"),
        (0, "0.0"),
        (1, "0.001"),
        (1050, "1.05"),
        (-999_999_999_999_999, "-999999999999.999"),
    ] {
        assert_eq!(
            serialize_item(&item(decimal(thousandths))).unwrap(),
            text,
            "{thousandths}"
        );
    }
}

#[test]
fn sf_serialize_string_escapes_quote_and_backslash() {
    assert_eq!(
        serialize_item(&item(string(r#"a"b\c"#))).unwrap(),
        r#""a\"b\\c""#
    );
    assert!(serialize_item(&item(string("é"))).is_err());
    assert!(serialize_item(&item(string("a\nb"))).is_err());
    assert!(serialize_item(&item(string("a\u{7f}"))).is_err());
}

#[test]
fn sf_serialize_token_grammar() {
    assert_eq!(serialize_item(&item(token("*a:/b"))).unwrap(), "*a:/b");
    for bad in ["", "1a", "a b", "a\"", "-a", "a,b"] {
        assert!(serialize_item(&item(token(bad))).is_err(), "{bad:?}");
    }
}

#[test]
fn sf_serialize_key_grammar() {
    for bad in ["", "A", "1a", "a b", "a="] {
        let i = item_p(BareItem::Integer(1), &[(bad, BareItem::Boolean(true))]);
        assert!(serialize_item(&i).is_err(), "{bad:?}");
        let d: Dictionary = [(bad.to_owned(), m_item(BareItem::Integer(1)))]
            .into_iter()
            .collect();
        assert!(serialize_dictionary(&d).is_err(), "{bad:?}");
    }
}

#[test]
fn sf_serialize_byte_sequence_with_padding() {
    for (bytes, text) in [
        (vec![], "::"),
        (vec![0], ":AA==:"),
        (vec![0, 0], ":AAA=:"),
        (vec![0, 0, 0], ":AAAA:"),
        (vec![0xfb, 0xff, 0xbf], ":+/+/:"),
        (
            b"pretend this is binary content.".to_vec(),
            ":cHJldGVuZCB0aGlzIGlzIGJpbmFyeSBjb250ZW50Lg==:",
        ),
    ] {
        assert_eq!(
            serialize_item(&item(BareItem::ByteSequence(bytes))).unwrap(),
            text
        );
    }
}

#[test]
fn sf_serialize_display_string_percent_encodes() {
    for (text, wire) in [
        ("%\" x", r#"%"%25%22 x""#),
        ("€", r#"%"%e2%82%ac""#),
        ("a\nb\u{7f}", r#"%"a%0ab%7f""#),
        ("", r#"%"""#),
    ] {
        assert_eq!(
            serialize_item(&item(BareItem::DisplayString(text.to_owned()))).unwrap(),
            wire
        );
    }
}

#[test]
fn sf_serialize_inner_list_and_parameter_values() {
    let list = vec![
        inner(
            vec![
                item_p(token("a"), &[("x", BareItem::Boolean(false))]),
                item(BareItem::Integer(-3)),
            ],
            &[("y", BareItem::Boolean(true)), ("z", string("q"))],
        ),
        m_item(BareItem::Boolean(true)),
    ];
    assert_eq!(serialize_list(&list).unwrap(), r#"(a;x=?0 -3);y;z="q", ?1"#);
    assert!(
        serialize_list(&vec![inner(vec![item(token("1"))], &[])]).is_err(),
        "an inner item fails"
    );
    assert!(
        serialize_list(&vec![inner(vec![], &[("A", BareItem::Boolean(true))])]).is_err(),
        "an inner list parameter fails"
    );
}

#[test]
fn sf_serialize_dictionary_true_member_keeps_parameters() {
    let d: Dictionary = [
        (
            "a".to_owned(),
            Member::Item(item_p(
                BareItem::Boolean(true),
                &[("p", BareItem::Integer(1))],
            )),
        ),
        ("b".to_owned(), m_item(BareItem::Boolean(false))),
        ("c".to_owned(), inner(vec![], &[])),
    ]
    .into_iter()
    .collect();
    assert_eq!(serialize_dictionary(&d).unwrap(), "a;p=1, b=?0, c=()");
    let bad: Dictionary = [("a".to_owned(), m_item(token("1")))].into_iter().collect();
    let e = serialize_dictionary(&bad).unwrap_err();
    assert_eq!(
        e.to_string(),
        format!("structured field serialize error: {}", e.reason())
    );
    assert_ne!(e.reason(), "");
}

#[test]
fn sf_parse_error_reason_text_is_fixed() {
    let e = parse_list("a, b c").unwrap_err();
    assert_eq!(e.reason(), "expected a comma after a member");
}
