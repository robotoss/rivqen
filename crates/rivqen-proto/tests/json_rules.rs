// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The JSON rules of manifest-patch section 3.2 on the strict reader.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use rivqen_proto::json::{Event, JsonRule, Member, Reader, Value, parse, parse_with_max_depth};

fn rule(text: &str) -> JsonRule {
    parse(text.as_bytes())
        .expect_err("the text must be rejected")
        .rule()
}

fn ok(text: &str) -> Value {
    parse(text.as_bytes()).expect("the text must be accepted")
}

fn obj(members: &[(&str, Value)]) -> Value {
    Value::Object(
        members
            .iter()
            .map(|(n, v)| Member {
                name: (*n).to_owned(),
                value: v.clone(),
            })
            .collect(),
    )
}

#[test]
fn json_accepts_every_value_kind() {
    let v = ok(" {\"a\" : [1, true, false, null, \"x\", {}, []],\n\t\"b\":{\"c\":0}}\r\n");
    assert_eq!(
        v,
        obj(&[
            (
                "a",
                Value::Array(vec![
                    Value::Integer(1),
                    Value::Bool(true),
                    Value::Bool(false),
                    Value::Null,
                    Value::String("x".into()),
                    Value::Object(vec![]),
                    Value::Array(vec![]),
                ])
            ),
            ("b", obj(&[("c", Value::Integer(0))])),
        ])
    );
    assert_eq!(ok("\"top\""), Value::String("top".into()));
    assert_eq!(ok("7"), Value::Integer(7));
}

#[test]
fn json_keeps_member_order() {
    let Value::Object(members) = ok(r#"{"z":1,"a":2,"m":3}"#) else {
        panic!("object expected");
    };
    let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["z", "a", "m"]);
}

#[test]
fn json_rejects_duplicate_member_names_at_any_depth() {
    assert_eq!(rule(r#"{"a":1,"a":1}"#), JsonRule::Duplicate);
    assert_eq!(rule(r#"{"a":{"b":1,"c":2,"b":3}}"#), JsonRule::Duplicate);
    assert_eq!(rule(r#"[{"x":[{"y":0,"y":0}]}]"#), JsonRule::Duplicate);
    // Names are compared after the escapes are decoded.
    assert_eq!(rule(&tilde(r#"{"a":1,"~u0061":2}"#)), JsonRule::Duplicate);
    assert_eq!(rule(r#"{"\/":1,"/":2}"#), JsonRule::Duplicate);
    // Equal names in different objects are not duplicates.
    ok(r#"{"a":{"a":1},"b":{"a":2},"c":[{"a":3},{"a":4}]}"#);
}

#[test]
fn json_duplicate_error_points_at_the_second_name() {
    let e = parse(br#"{"a":1, "a":2}"#).unwrap_err();
    assert_eq!(e.rule(), JsonRule::Duplicate);
    assert_eq!(e.offset(), 8);
}

#[test]
fn json_rejects_a_byte_order_mark() {
    assert_eq!(rule("\u{feff}{}"), JsonRule::Bom);
    assert_eq!(rule("\u{feff}"), JsonRule::Bom);
    // A BOM after the start is not white space.
    assert_eq!(rule(" \u{feff}{}"), JsonRule::Syntax);
}

#[test]
fn json_rejects_text_that_is_not_utf8() {
    let e = parse(b"{\"a\":\"\xff\"}").unwrap_err();
    assert_eq!(e.rule(), JsonRule::Encoding);
    assert_eq!(e.offset(), 6);
    // An encoded surrogate (CESU-8) is not UTF-8.
    assert_eq!(
        parse(b"\"\xed\xa0\x80\"").unwrap_err().rule(),
        JsonRule::Encoding
    );
}

#[test]
fn json_rejects_a_lone_surrogate_escape() {
    assert_eq!(rule(r#""\ud800""#), JsonRule::Surrogate);
    assert_eq!(rule(r#""\udbff x""#), JsonRule::Surrogate);
    assert_eq!(rule(r#""\udc00""#), JsonRule::Surrogate);
    assert_eq!(rule(r#""\ude00\ud83d""#), JsonRule::Surrogate);
    assert_eq!(rule(&tilde(r#""~ud83d~u0041""#)), JsonRule::Surrogate);
    assert_eq!(rule(&tilde(r#""~ud83dA""#)), JsonRule::Surrogate);
    assert_eq!(rule(r#""\ud83d\ud83d""#), JsonRule::Surrogate);
    assert_eq!(rule(r#""\ud83d\n""#), JsonRule::Surrogate);
    assert_eq!(rule(r#"{"\ud800":1}"#), JsonRule::Surrogate);
    // A pair decodes to one scalar value.
    assert_eq!(
        ok(&tilde(r#""~ud83d~ude00""#)),
        Value::String("\u{1f600}".into())
    );
    assert_eq!(
        ok(&tilde(r#""~uD83D~uDE00""#)),
        Value::String("\u{1f600}".into())
    );
    assert_eq!(
        ok(&tilde(r#""~udbff~udfff""#)),
        Value::String("\u{10ffff}".into())
    );
    assert_eq!(
        ok(&tilde(r#""~ud800~udc00""#)),
        Value::String("\u{10000}".into())
    );
}

/// `~` stands for a backslash, so that the JSON escapes stay visible.
fn tilde(text: &str) -> String {
    text.replace('~', "\\")
}

#[test]
fn json_rejects_numbers_that_are_not_digits_only() {
    for text in [
        "60.0",
        "6e1",
        "6E1",
        "6e+1",
        "60e-0",
        "-1",
        "-0",
        "0.5",
        "1.5e3",
        "[60.0]",
        r#"{"ttl_seconds":6e1}"#,
    ] {
        assert_eq!(rule(text), JsonRule::Integer, "{text}");
    }
    // Above u64::MAX.
    assert_eq!(rule("18446744073709551616"), JsonRule::Integer);
    assert_eq!(ok("18446744073709551615"), Value::Integer(u64::MAX));
    assert_eq!(ok("60"), Value::Integer(60));
    assert_eq!(ok("0"), Value::Integer(0));
}

#[test]
fn json_rejects_broken_number_grammar_as_syntax() {
    for text in [
        "01", "00", "1.", "1.e5", "1e", "1e+", "-", "-a", "+1", ".5", "0x10", "1 2",
    ] {
        assert_eq!(rule(text), JsonRule::Syntax, "{text}");
    }
}

fn nested(depth: usize, open: &str, close: &str, inner: &str) -> String {
    let mut s = open.repeat(depth);
    s.push_str(inner);
    s.push_str(&close.repeat(depth));
    s
}

#[test]
fn json_depth_16_is_accepted_and_17_is_rejected() {
    ok(&nested(16, "[", "]", ""));
    ok(&nested(16, "[", "]", "1"));
    ok(&nested(15, r#"{"a":"#, "}", "{}"));
    assert_eq!(rule(&nested(17, "[", "]", "")), JsonRule::Depth);
    assert_eq!(rule(&nested(16, r#"{"a":"#, "}", "[]")), JsonRule::Depth);
    assert_eq!(rule(&nested(17, r#"{"a":"#, "}", "0")), JsonRule::Depth);
    // A depth of 10 000 fails without a deep call stack.
    assert_eq!(rule(&nested(10_000, "[", "]", "")), JsonRule::Depth);
}

#[test]
fn json_depth_limit_is_a_parameter() {
    assert!(parse_with_max_depth(b"[[]]", 2).is_ok());
    assert_eq!(
        parse_with_max_depth(b"[[[]]]", 2).unwrap_err().rule(),
        JsonRule::Depth
    );
    assert_eq!(
        parse_with_max_depth(b"[]", 0).unwrap_err().rule(),
        JsonRule::Depth
    );
    assert_eq!(parse_with_max_depth(b"0", 0).unwrap(), Value::Integer(0));
}

#[test]
fn json_tree_depth_is_capped() {
    // A tree deeper than MAX_JSON_DEPTH would make drop, clone, comparison and
    // Debug recurse once per level: the builder caps any larger limit.
    let at_cap = nested(16, "[", "]", "");
    let tree = parse_with_max_depth(at_cap.as_bytes(), usize::MAX).unwrap();
    assert_eq!(tree.clone(), tree);
    assert_eq!(
        parse_with_max_depth(nested(17, "[", "]", "").as_bytes(), usize::MAX)
            .unwrap_err()
            .rule(),
        JsonRule::Depth
    );
    assert_eq!(
        parse_with_max_depth(nested(17, r#"{"a":"#, "}", "0").as_bytes(), 17)
            .unwrap_err()
            .rule(),
        JsonRule::Depth
    );
    // 100 000 levels fail at level 17, without a crash.
    let deep = nested(100_000, "[", "]", "");
    let e = parse_with_max_depth(deep.as_bytes(), usize::MAX).unwrap_err();
    assert_eq!((e.rule(), e.offset()), (JsonRule::Depth, 16));
    // The streaming reader builds no tree and keeps a larger limit.
    let mut reader = Reader::new(deep.as_bytes(), usize::MAX).unwrap();
    while reader.next_event().unwrap().is_some() {}
}

#[test]
fn json_rejects_broken_syntax() {
    for text in [
        "",
        " ",
        "[1,]",
        r#"{"a":1,}"#,
        "[,1]",
        "{,}",
        r#"{"a"}"#,
        r#"{"a" 1}"#,
        "{a:1}",
        "{'a':1}",
        "[1 2]",
        r#"{"a":1 "b":2}"#,
        "[1}",
        r#"{"a":1]"#,
        "[",
        "]",
        "{",
        r#"""#,
        r#""abc"#,
        "\"a\u{1}b\"",
        "\"a\nb\"",
        r#""\x""#,
        r#""\u12""#,
        r#""\u+123""#,
        r#""\u12G4""#,
        r#""\"#,
        "tru",
        "truex",
        "nul",
        "NaN",
        "Infinity",
        "[1] [2]",
        "{} x",
        "/* c */ 1",
        "1 // c",
        "\u{a0}1",
    ] {
        assert_eq!(rule(text), JsonRule::Syntax, "{text:?}");
    }
    // A `\u` escape with a non-hex character, or one that ends inside a
    // multi-byte character, or at the end of the text.
    for text in ["\"~u00\u{e9}\"", "\"~u000\u{e9}\"", "\"~u0", "\"~ud83d~u00"] {
        assert_eq!(rule(&tilde(text)), JsonRule::Syntax, "{text:?}");
    }
}

#[test]
fn json_decodes_every_escape() {
    assert_eq!(
        ok(&tilde(r#""~"~~~/~b~f~n~r~t~u0000~u00e9~u20AC~u007F""#)),
        Value::String("\"\\/\u{8}\u{c}\n\r\t\u{0}\u{e9}\u{20ac}\u{7f}".into())
    );
    // Escapes, raw non-ASCII text and plain runs mix.
    assert_eq!(
        ok(&tilde("\"caf~u00e9 \u{2013} \u{e9}~n<b>\"")),
        Value::String("caf\u{e9} \u{2013} \u{e9}\n<b>".into())
    );
    // DEL and other non-control characters are allowed raw.
    assert_eq!(ok("\"\u{7f}\u{80}\""), Value::String("\u{7f}\u{80}".into()));
}

#[test]
fn json_reader_streams_events_and_borrows_plain_strings() {
    let text = tilde(r#"{"a":["x","~u0079"]}"#);
    let mut reader = Reader::new(text.as_bytes(), 16).unwrap();
    let mut events = Vec::new();
    while let Some(e) = reader.next_event().unwrap() {
        events.push(e);
    }
    assert_eq!(events.len(), 7);
    assert_eq!(events[6], Event::EndObject);
    assert_eq!(events[0], Event::StartObject);
    assert!(matches!(
        &events[1],
        Event::Name(std::borrow::Cow::Borrowed("a"))
    ));
    assert_eq!(events[2], Event::StartArray);
    assert!(matches!(
        &events[3],
        Event::String(std::borrow::Cow::Borrowed("x"))
    ));
    assert!(matches!(&events[4], Event::String(std::borrow::Cow::Owned(s)) if s == "y"));
    assert_eq!(events[5], Event::EndArray);
    // After the end the reader stays at the end.
    assert_eq!(reader.next_event().unwrap(), None);
    assert!(reader.finish().is_ok());
}

#[test]
fn json_reader_repeats_its_first_error() {
    let mut reader = Reader::new(br#"{"a":1,"a":2}"#, 16).unwrap();
    let first = loop {
        match reader.next_event() {
            Ok(Some(_)) => {}
            Ok(None) => panic!("the text must be rejected"),
            Err(e) => break e,
        }
    };
    assert_eq!(first.rule(), JsonRule::Duplicate);
    assert_eq!(reader.next_event().unwrap_err(), first);
    assert_eq!(reader.finish().unwrap_err(), first);
}

#[test]
fn json_reader_finish_rejects_an_incomplete_value() {
    let mut reader = Reader::new(b"[1]", 16).unwrap();
    assert_eq!(reader.next_event().unwrap(), Some(Event::StartArray));
    assert_eq!(reader.finish().unwrap_err().rule(), JsonRule::Syntax);
}

#[test]
fn json_errors_and_debug_hold_no_content() {
    let marker = "SECRET-MARKER-7f3a";
    let texts = [
        format!(r#"{{"{marker}":1,"{marker}":2}}"#),
        format!(r#"{{"a":"{marker}\ud800"}}"#),
        format!(r#"["{marker}", 1.5]"#),
        format!(r#"["{marker}" x]"#),
    ];
    for text in &texts {
        let e = parse(text.as_bytes()).unwrap_err();
        assert!(!format!("{e} {e:?}").contains(marker));
    }
    let v = ok(&format!(r#"{{"{marker}":["{marker}"]}}"#));
    assert!(!format!("{v:?}").contains(marker));
    let text = format!(r#"{{"{marker}":"{marker}"}}"#);
    let mut reader = Reader::new(text.as_bytes(), 16).unwrap();
    while let Some(e) = reader.next_event().unwrap() {
        assert!(!format!("{e:?} {reader:?}").contains(marker));
    }
    // Numbers and booleans can be block content too: Debug gives the kind only.
    let number = "987654321";
    let text = format!(r#"{{"a":[{number},true,false,null]}}"#);
    let v = ok(&text);
    let shown = format!("{v:?}");
    assert!(!shown.contains(number), "{shown}");
    assert!(
        !shown.contains("true") && !shown.contains("false"),
        "{shown}"
    );
    assert_eq!(
        shown,
        "[Member(name len=1, [Integer(..), Bool(..), Bool(..), Null])]"
    );
    let mut reader = Reader::new(text.as_bytes(), 16).unwrap();
    let mut events = Vec::new();
    while let Some(e) = reader.next_event().unwrap() {
        events.push(format!("{e:?}"));
    }
    let shown = events.join(" ");
    assert!(!shown.contains(number), "{shown}");
    assert!(
        !shown.contains("true") && !shown.contains("false"),
        "{shown}"
    );
    assert_eq!(
        shown,
        "StartObject Name(len=1) StartArray Integer(..) Bool(..) Bool(..) Null EndArray EndObject"
    );
}

#[test]
fn json_error_display_names_the_rule_and_offset() {
    let e = parse(b"[1.5]").unwrap_err();
    assert_eq!(e.offset(), 1);
    assert_eq!(e.rule().as_str(), "integer");
    assert_eq!(
        e.to_string(),
        "number is not an integer of digits only (manifest-patch 3.2 rule 4) at byte 1"
    );
    for (rule, name) in [
        (JsonRule::Encoding, "encoding"),
        (JsonRule::Bom, "bom"),
        (JsonRule::Syntax, "syntax"),
        (JsonRule::Duplicate, "duplicate"),
        (JsonRule::Surrogate, "surrogate"),
        (JsonRule::Integer, "integer"),
        (JsonRule::Depth, "depth"),
    ] {
        assert_eq!(rule.to_string(), name);
    }
}

#[test]
fn json_reader_debug_shows_counts_not_input() {
    let mut reader = Reader::new(b"[\"SECRET\"]", 16).unwrap();
    assert_eq!(format!("{reader:?}"), "Reader(len=10, pos=0, depth=0)");
    reader.next_event().unwrap();
    let text = format!("{reader:?}");
    assert_eq!(text, "Reader(len=10, pos=1, depth=1)");
    assert!(!text.contains("SECRET"));
}

#[test]
fn json_string_refuses_raw_control_character_before_and_after_an_escape() {
    // Fast path: no escape before the control character.
    assert_eq!(rule("\"a\u{1}\""), JsonRule::Syntax);
    // Slow path: a two-character escape first, then a raw U+0001.
    assert_eq!(rule("\"a\\n\u{1}\""), JsonRule::Syntax);
    assert_eq!(rule("\"a\\nb\u{1}c\""), JsonRule::Syntax);
    // The same text without the control character is accepted.
    assert_eq!(ok("\"a\\nb\""), Value::String("a\nb".into()));
}
