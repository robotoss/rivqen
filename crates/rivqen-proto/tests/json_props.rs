// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Property tests of the strict JSON reader, the manifest and the envelope:
//! no panic on any bytes, a round trip of random values, and the depth and
//! size bounds. Fixed seed, no failure files.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::fmt::Write as _;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_integrity::{Binding, PageRevision, sha256};
use rivqen_proto::envelope::{EnvelopeError, parse_envelope};
use rivqen_proto::json::{JsonRule, Member, Value, parse};
use rivqen_proto::manifest::{ManifestError, parse_manifest};
use rivqen_proto::types::{
    BlockId, Limits, MAX_JSON_DEPTH, MAX_MANIFEST_BYTES, MAX_PATCH_BYTES, Mode, PageManifest,
    Policy,
};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        rng_seed: RngSeed::Fixed(0x0005_ee0d_2026_1109),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// How the test serializer writes a text.
#[derive(Debug, Clone, Copy)]
struct Style {
    /// White space around every token.
    spaces: bool,
    /// Every character of a string as `\u` escapes (UTF-16 units).
    escape_all: bool,
}

fn write_string(out: &mut String, s: &str, style: Style) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' if !style.escape_all => out.push_str("\\n"),
            '\t' if !style.escape_all => out.push_str("\\t"),
            c if style.escape_all || u32::from(c) < 0x20 => {
                let mut units = [0_u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push('\\');
                    write!(out, "u{unit:04x}").unwrap();
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn ws(out: &mut String, style: Style) {
    if style.spaces {
        out.push_str(" \n\t\r ");
    }
}

fn write_value(out: &mut String, v: &Value, style: Style) {
    ws(out, style);
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => write!(out, "{b}").unwrap(),
        Value::Integer(n) => write!(out, "{n}").unwrap(),
        Value::String(s) => write_string(out, s, style),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, item, style);
            }
            ws(out, style);
            out.push(']');
        }
        Value::Object(members) => {
            out.push('{');
            for (i, m) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                ws(out, style);
                write_string(out, &m.name, style);
                ws(out, style);
                out.push(':');
                write_value(out, &m.value, style);
            }
            ws(out, style);
            out.push('}');
        }
    }
    ws(out, style);
}

fn serialize(v: &Value, style: Style) -> String {
    let mut out = String::new();
    write_value(&mut out, v, style);
    out
}

fn depth(v: &Value) -> usize {
    match v {
        Value::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Value::Object(members) => 1 + members.iter().map(|m| depth(&m.value)).max().unwrap_or(0),
        _ => 0,
    }
}

fn arb_style() -> impl Strategy<Value = Style> {
    (any::<bool>(), any::<bool>()).prop_map(|(spaces, escape_all)| Style { spaces, escape_all })
}

fn arb_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<u64>().prop_map(Value::Integer),
        any::<String>().prop_map(Value::String),
    ];
    leaf.prop_recursive(18, 96, 5, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..5).prop_map(Value::Array),
            // Unique names (rule 2); the map order is the member order.
            prop::collection::btree_map(any::<String>(), inner, 0..5).prop_map(|m| {
                Value::Object(
                    m.into_iter()
                        .map(|(name, value)| Member { name, value })
                        .collect(),
                )
            }),
        ]
    })
}

/// Pieces of JSON-like text, so that random input reaches deep states.
fn arb_token_soup() -> impl Strategy<Value = Vec<u8>> {
    let tokens: Vec<&'static [u8]> = vec![
        b"{",
        b"}",
        b"[",
        b"]",
        b"\"",
        b":",
        b",",
        b" ",
        b"\\",
        b"u",
        b"d800",
        b"dc00",
        b"0041",
        b"0",
        b"1",
        b"-",
        b".",
        b"e",
        b"true",
        b"null",
        b"\"a\"",
        b"\"binding\"",
        b"\"operations\"",
        b"\"block_id\"",
        b"\"content\"",
        b"\"protocol\"",
        b"\"rqp/1\"",
        b"\"origin\"",
        b"\"path\"",
        b"\"policy\"",
        b"\"limits\"",
        b"\"ttl_seconds\"",
        b"\"max_patch_bytes\"",
        b"\"mode\"",
        b"\"page_revision\"",
        b"\xc3\xa9",
        b"\xff",
        b"\xef\xbb\xbf",
        b"\n",
    ];
    prop::collection::vec(prop::sample::select(tokens), 0..600).prop_map(|parts| parts.concat())
}

/// Whatever the reader says about `bytes`, it says it without a panic and
/// with an offset inside the text.
fn check_any_bytes(bytes: &[u8]) -> Result<(), TestCaseError> {
    if let Err(e) = parse(bytes) {
        prop_assert!(e.offset() <= bytes.len());
    }
    match parse_manifest(bytes) {
        Err(ManifestError::Json(e)) => prop_assert!(e.offset() <= bytes.len()),
        Err(ManifestError::TooLarge) => prop_assert!(bytes.len() > MAX_MANIFEST_BYTES),
        _ => {}
    }
    match parse_envelope(bytes, MAX_PATCH_BYTES, None) {
        Err(EnvelopeError::Json(e)) => prop_assert!(e.offset() <= bytes.len()),
        Ok(env) => prop_assert!((1..=256).contains(&env.operations.len())),
        _ => {}
    }
    Ok(())
}

fn nested(kinds: &[bool]) -> String {
    let mut s = String::new();
    for &object in kinds {
        s.push_str(if object { "{\"k\":" } else { "[" });
    }
    s.push('0');
    for &object in kinds.iter().rev() {
        s.push(if object { '}' } else { ']' });
    }
    s
}

fn arb_policy() -> impl Strategy<Value = Option<Policy>> {
    proptest::option::of(
        (
            proptest::option::of(prop_oneof![Just(Mode::Private), Just(Mode::Public)]),
            proptest::option::of(0_u32..=31_536_000),
            proptest::option::of(any::<bool>()),
        )
            .prop_map(|(mode, ttl_seconds, offline)| Policy {
                mode,
                ttl_seconds,
                offline,
            }),
    )
}

fn arb_limits() -> impl Strategy<Value = Option<Limits>> {
    proptest::option::of(
        proptest::option::of(1024_u32..=5_242_880)
            .prop_map(|max_patch_bytes| Limits { max_patch_bytes }),
    )
}

fn arb_manifest() -> impl Strategy<Value = PageManifest> {
    (
        "https?://([a-z0-9._-]{1,40}|\\[[0-9a-f:.]{1,39}\\])(:[1-9][0-9]{0,4})?",
        "/[!$-;=@-~]{0,60}",
        any::<[u8; 8]>(),
        arb_policy(),
        arb_limits(),
    )
        .prop_map(|(origin, path, seed, policy, limits)| PageManifest {
            origin,
            path,
            page_revision: PageRevision::from_digest(sha256(&seed)),
            policy,
            limits,
        })
}

fn manifest_value(m: &PageManifest) -> Value {
    let member = |name: &str, value: Value| Member {
        name: name.to_owned(),
        value,
    };
    let mut members = vec![
        member("protocol", Value::String("rqp/1".into())),
        member("origin", Value::String(m.origin.clone())),
        member("path", Value::String(m.path.clone())),
        member("page_revision", Value::String(m.page_revision.to_string())),
    ];
    if let Some(p) = m.policy {
        let mut inner = Vec::new();
        if let Some(mode) = p.mode {
            let text = if mode == Mode::Public {
                "public"
            } else {
                "private"
            };
            inner.push(member("mode", Value::String(text.into())));
        }
        if let Some(ttl) = p.ttl_seconds {
            inner.push(member("ttl_seconds", Value::Integer(u64::from(ttl))));
        }
        if let Some(offline) = p.offline {
            inner.push(member("offline", Value::Bool(offline)));
        }
        members.push(member("policy", Value::Object(inner)));
    }
    if let Some(l) = m.limits {
        let inner = l
            .max_patch_bytes
            .map(|n| member("max_patch_bytes", Value::Integer(u64::from(n))))
            .into_iter()
            .collect();
        members.push(member("limits", Value::Object(inner)));
    }
    members.reverse();
    Value::Object(members)
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn json_no_panic_on_any_bytes(bytes in prop::collection::vec(any::<u8>(), 0..=65_536)) {
        check_any_bytes(&bytes)?;
    }

    #[test]
    fn json_no_panic_on_token_soup(bytes in arb_token_soup()) {
        check_any_bytes(&bytes)?;
    }

    #[test]
    fn json_reader_accepts_serialized_values(value in arb_value(), style in arb_style()) {
        let text = serialize(&value, style);
        let parsed = parse(text.as_bytes());
        if depth(&value) <= MAX_JSON_DEPTH {
            prop_assert_eq!(parsed.unwrap(), value);
        } else {
            prop_assert_eq!(parsed.unwrap_err().rule(), JsonRule::Depth);
        }
    }

    #[test]
    fn json_depth_bound_holds(kinds in prop::collection::vec(any::<bool>(), 0..=40)) {
        let result = parse(nested(&kinds).as_bytes());
        if kinds.len() <= MAX_JSON_DEPTH {
            prop_assert!(result.is_ok());
        } else {
            prop_assert_eq!(result.unwrap_err().rule(), JsonRule::Depth);
        }
    }

    #[test]
    fn json_numbers_other_than_digits_break_rule_4(
        a in any::<u64>(),
        b in any::<u32>(),
        e in any::<i16>(),
        form in 0_u8..4,
    ) {
        let text = match form {
            0 => format!("-{a}"),
            1 => format!("{a}.{b}"),
            2 => format!("{a}e{e}"),
            _ => format!("{a}.{b}E+{}", e.unsigned_abs()),
        };
        prop_assert_eq!(parse(text.as_bytes()).unwrap_err().rule(), JsonRule::Integer);
    }

    #[test]
    fn manifest_accepts_generated_valid_manifests(m in arb_manifest(), style in arb_style()) {
        let text = serialize(&manifest_value(&m), style);
        prop_assert_eq!(parse_manifest(text.as_bytes()).unwrap(), m);
    }

    #[test]
    fn manifest_size_bound_holds(m in arb_manifest(), extra in 0_usize..4096, fill in any::<u8>()) {
        let mut text = serialize(&manifest_value(&m), Style { spaces: false, escape_all: false })
            .into_bytes();
        let padded = MAX_MANIFEST_BYTES - extra;
        if text.len() <= padded {
            text.resize(padded, b' ');
            prop_assert!(parse_manifest(&text).is_ok());
        }
        let over = vec![fill; MAX_MANIFEST_BYTES + 1 + extra];
        prop_assert_eq!(parse_manifest(&over), Err(ManifestError::TooLarge));
    }

    #[test]
    fn envelope_accepts_generated_valid_envelopes(
        ops in prop::collection::vec(("[a-z0-9][a-z0-9_-]{0,63}", any::<String>()), 1..=20),
        seed in any::<[u8; 8]>(),
        style in arb_style(),
    ) {
        let binding = Binding::from_digest(sha256(&seed));
        let operations = ops
            .iter()
            .map(|(id, content)| {
                Value::Object(vec![
                    Member { name: "content".into(), value: Value::String(content.clone()) },
                    Member { name: "block_id".into(), value: Value::String(id.clone()) },
                ])
            })
            .collect();
        let value = Value::Object(vec![
            Member { name: "operations".into(), value: Value::Array(operations) },
            Member { name: "binding".into(), value: Value::String(binding.to_string()) },
        ]);
        let text = serialize(&value, style);
        let env = parse_envelope(text.as_bytes(), usize::MAX, None).unwrap();
        prop_assert_eq!(env.binding, binding);
        let got: Vec<(&str, &str)> =
            env.operations.iter().map(|op| (op.block_id.as_str(), op.content.as_str())).collect();
        let want: Vec<(&str, &str)> = ops.iter().map(|(i, c)| (i.as_str(), c.as_str())).collect();
        prop_assert_eq!(got, want);
        prop_assert!(env.operations.iter().all(|op| BlockId::parse(op.block_id.as_str()).is_ok()));
    }

    #[test]
    fn envelope_size_bound_holds(
        limit in 0_usize..8192,
        hint in proptest::option::of(1024_u32..=8192),
        extra in 0_usize..64,
    ) {
        let limits = Some(Limits { max_patch_bytes: hint });
        let e = hint.map_or(limit, |h| limit.min(usize::try_from(h).unwrap()));
        let over = vec![b'{'; e + 1 + extra];
        prop_assert_eq!(parse_envelope(&over, limit, limits), Err(EnvelopeError::TooLarge));
        let base = format!(
            r#"{{"binding":"{}","operations":[{{"block_id":"a","content":""}}]}}"#,
            Binding::from_digest(sha256(b"x"))
        );
        if base.len() <= e {
            let mut body = base.into_bytes();
            body.resize(e, b' ');
            prop_assert!(parse_envelope(&body, limit, limits).is_ok());
        }
    }
}
