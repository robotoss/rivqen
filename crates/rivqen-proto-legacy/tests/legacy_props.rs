// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the legacy wire codec: no function panics for any field
//! list or body, a head that passes the legacy test has the four fields with
//! one value each and follows the field rules of DL-226, page fields never
//! hold a line that is not a valid field line and never lose `Content-Type`
//! or a protected field, and the codec keeps the content and the order of
//! what it reads.
//!
//! Fixed seeds; `PROPTEST_CASES` raises the case count.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_integrity::legacy::sha1_hex;
use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::body::{BodyLimits, read_data_body};
use rivqen_proto_legacy::fields::{NOT_PAGE_FIELDS, merge_page_fields, page_fields};
use rivqen_proto_legacy::message::{MessageInput, ResultCode, message};
use rivqen_proto_legacy::request::{EntryValidators, LegacyRequest, request_fields};
use rivqen_proto_legacy::response::{
    LegacyHead, bare_etag, charset, etag_field_name, etag_value, is_legacy_response, is_safe_head,
    is_template_change, is_utf8_text,
};
use rivqen_template::legacy::DataBlock;
use serde_json::Value;

fn config(cases: u32, seed: u64) -> ProptestConfig {
    ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(cases),
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// Field names: the legacy and CL-21 names in several spellings, and others.
fn name() -> impl Strategy<Value = String> {
    let known = prop::sample::select(vec![
        "content-type",
        "Content-Type",
        "etag",
        "ETag",
        "sonic-etag-key",
        "Sonic-ETag-Key",
        "custom-etag",
        "template-tag",
        "TEMPLATE-TAG",
        "template-change",
        "cache-offline",
        "Cache-Offline",
        "connection",
        "set-cookie",
        "Set-Cookie2",
        "x-a",
        "X-A",
        "link",
        "Content-Security-Policy",
        "x-frame-options",
        "Cross-Origin-Opener-Policy",
        "referrer-policy",
        "isReload",
        // DL-226 (b): names that are not tokens.
        "Set-Cookie ",
        "set-cookie\0",
        "",
        "x a",
    ])
    .prop_map(str::to_owned);
    prop_oneof![4 => known, 1 => "[ -~]{0,12}", 1 => any::<String>()]
}

/// Field values: legacy values, media types, names (for `sonic-etag-key`
/// and `Connection`), white space, and any text.
fn value() -> impl Strategy<Value = String> {
    let known = prop::sample::select(vec![
        "",
        " ",
        "\t",
        "true",
        " true ",
        "false",
        "0",
        "store",
        "http",
        "e1",
        "W/\"e1\"",
        "t1",
        "text/html",
        "TEXT/HTML; charset=UTF-8",
        "text/html; charset=gbk",
        "text/plain",
        "custom-etag",
        "x-a, link",
        "etag",
        // DL-226 (a): names that `sonic-etag-key` and `Connection` may not
        // give.
        "Content-Security-Policy",
        "close, x-frame-options",
        "content-type",
        "cross-origin-resource-policy",
        "isReload",
        "template-tag",
        // DL-226 (b): values with a control byte.
        "a\r\nset-cookie: s=1",
        "\0",
        "a\tb",
    ])
    .prop_map(str::to_owned);
    prop_oneof![4 => known, 1 => "[ -~]{0,16}", 1 => any::<String>()]
}

/// Mostly valid field lines: an independent case reaches the accepted
/// heads often enough.
fn mostly_valid_field_list(max: usize) -> impl Strategy<Value = Vec<FieldLine>> {
    let line = prop_oneof![
        6 => ("[a-zA-Z][a-zA-Z0-9-]{0,10}", "[ -~]{0,12}"),
        6 => (name(), value()),
    ];
    prop::collection::vec(line, 0..max).prop_map(|pairs| {
        pairs
            .into_iter()
            .map(|(n, v)| FieldLine::new(n, v))
            .collect()
    })
}

fn field_list(max: usize) -> impl Strategy<Value = Vec<FieldLine>> {
    prop::collection::vec((name(), value()), 0..max).prop_map(|pairs| {
        pairs
            .into_iter()
            .map(|(n, v)| FieldLine::new(n, v))
            .collect()
    })
}

/// A canonical legacy head with extra lines mixed in: many pass the test
/// (`legacy_prop_near_legacy_heads_reach_both_answers` counts them).
fn near_legacy_head() -> impl Strategy<Value = Vec<FieldLine>> {
    (mostly_valid_field_list(6), any::<prop::sample::Index>()).prop_map(|(extra, at)| {
        let mut fields = vec![
            FieldLine::new("content-type", "text/html"),
            FieldLine::new("etag", "e1"),
            FieldLine::new("template-tag", "t1"),
            FieldLine::new("template-change", "false"),
            FieldLine::new("cache-offline", "true"),
        ];
        let at = at.index(fields.len() + 1);
        for (i, line) in extra.into_iter().enumerate() {
            fields.insert((at + i).min(fields.len()), line);
        }
        fields
    })
}

fn trimmed(value: &str) -> &str {
    value.trim_matches([' ', '\t'])
}

/// The CL-14 property for one passing head.
fn assert_one_value_each(fields: &[FieldLine], head: &LegacyHead<'_>) {
    let types: Vec<&FieldLine> = fields.iter().filter(|f| f.is("content-type")).collect();
    assert_eq!(types.len(), 1);
    let etag_field = etag_field_name(fields);
    let four = [
        (etag_field, head.etag()),
        ("template-tag", head.template_tag()),
        ("template-change", head.template_change()),
        ("cache-offline", head.cache_offline()),
    ];
    for (field, got) in four {
        let lines: Vec<&str> = fields
            .iter()
            .filter(|f| f.is(field))
            .map(|f| trimmed(&f.value))
            .collect();
        assert!(!lines.is_empty(), "{field}");
        assert!(!got.is_empty(), "{field}");
        assert!(lines.iter().all(|v| *v == got), "{field}");
    }
}

/// RFC 9110 `token`, written apart from the crate.
fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// A value without a control byte other than HTAB, written apart from the
/// crate.
fn has_no_control(value: &str) -> bool {
    !value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f)
}

/// DL-226 (a): `Content-Type` or a protected field, lower case.
fn is_kept_name(lower: &str) -> bool {
    lower == "content-type"
        || lower.starts_with("cross-origin-")
        || [
            "content-security-policy",
            "content-security-policy-report-only",
            "x-frame-options",
            "x-content-type-options",
            "permissions-policy",
            "referrer-policy",
            "strict-transport-security",
        ]
        .contains(&lower)
}

/// The field rules of DL-226 for a head that passed CL-14.
fn assert_safe_head(fields: &[FieldLine]) {
    for line in fields {
        assert!(is_token(&line.name), "{:?}", line.name);
        assert!(has_no_control(&line.value));
        if line.is("sonic-etag-key") {
            let key = trimmed(&line.value).to_ascii_lowercase();
            if !key.is_empty() {
                assert!(is_token(&key));
                assert!(!is_kept_name(&key), "{key}");
                assert!(!NOT_PAGE_FIELDS.contains(&key.as_str()), "{key}");
                assert_ne!(key, "isreload");
            }
        }
        if line.is("connection") {
            for token in line.value.split(',') {
                assert!(!is_kept_name(&trimmed(token).to_ascii_lowercase()));
            }
        }
    }
}

/// The page fields property: lowercase token names, values without a
/// control byte, sorted, no dropped name, each line from the input, and
/// every valid `Content-Type` or protected line of the input kept.
fn assert_page_fields(input: &[FieldLine], output: &[FieldLine]) {
    let etag_field = etag_field_name(input).to_ascii_lowercase();
    assert!(output.len() <= input.len());
    for line in input {
        let lower = line.name.to_ascii_lowercase();
        if is_kept_name(&lower) && is_token(&line.name) && has_no_control(&line.value) {
            assert!(
                output
                    .iter()
                    .any(|o| o.name == lower && o.value == line.value),
                "{lower} lost"
            );
        }
    }
    for line in output {
        assert!(is_token(&line.name), "{:?}", line.name);
        assert!(has_no_control(&line.value), "{}", line.name);
        assert_eq!(line.name, line.name.to_ascii_lowercase());
        assert!(
            !NOT_PAGE_FIELDS.contains(&line.name.as_str()),
            "{}",
            line.name
        );
        if !is_kept_name(&etag_field) {
            assert_ne!(line.name, etag_field);
        }
        assert!(
            input
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&line.name) && f.value == line.value)
        );
    }
    assert!(output.windows(2).all(|w| w[0].name <= w[1].name));
}

proptest! {
    #![proptest_config(config(512, 0x1e9a_c001))]

    #[test]
    fn legacy_prop_no_panic_on_any_fields(fields in field_list(12), body in any::<Vec<u8>>()) {
        let head = LegacyHead::read(&fields);
        prop_assert_eq!(head.is_some(), is_legacy_response(&fields));
        if let Some(head) = &head {
            assert_one_value_each(&fields, head);
            assert_safe_head(&fields);
            prop_assert_eq!(head.is_template_change(), is_template_change(head.template_change()));
            prop_assert_eq!(head.bare_etag(), bare_etag(head.etag()));
        }
        let etag_field = etag_field_name(&fields);
        let first_etag = fields.iter().find(|f| f.is(etag_field)).map(|f| trimmed(&f.value));
        prop_assert_eq!(etag_value(&fields), first_etag);
        if is_utf8_text(&fields, &body) {
            prop_assert!(std::str::from_utf8(&body).is_ok());
        }
        for line in &fields {
            prop_assert!(!charset(&line.value).bytes().any(|b| b.is_ascii_uppercase()));
        }
        let output = page_fields(&fields);
        assert_page_fields(&fields, &output);
        let merged = merge_page_fields(&output, &fields);
        prop_assert_eq!(&merged, &output);
        for code in [ResultCode::NoResult, ResultCode::DataUpdate, ResultCode::NotModified] {
            let diff = [DataBlock { key: "{a}".to_owned(), value: "v".to_owned() }];
            let input = MessageInput { src: code, final_code: code, diff: &diff, fields: &fields, reload: false };
            if let Some(text) = message(&input) {
                let object = text.starts_with("{\"") && text.ends_with("}}");
                prop_assert!(object);
            }
        }
    }

    #[test]
    fn legacy_prop_head_has_one_value_per_field(fields in near_legacy_head()) {
        prop_assert_eq!(LegacyHead::read(&fields).is_some(), is_legacy_response(&fields));
        if let Some(head) = LegacyHead::read(&fields) {
            assert_one_value_each(&fields, &head);
            assert_safe_head(&fields);
            prop_assert!(is_safe_head(&fields));
        }
        assert_page_fields(&fields, &page_fields(&fields));
    }

    #[test]
    fn legacy_prop_merge_never_passes_an_invalid_line(
        stored in field_list(8),
        response in field_list(8),
    ) {
        for line in merge_page_fields(&stored, &response) {
            prop_assert!(is_token(&line.name));
            prop_assert!(has_no_control(&line.value));
        }
    }

    #[test]
    fn legacy_prop_merge_keeps_stored_names_that_the_response_lacks(
        stored in field_list(8),
        response in field_list(8),
    ) {
        let stored = page_fields(&stored);
        let update = page_fields(&response);
        let merged = merge_page_fields(&stored, &response);
        for line in &merged {
            let from_update = update.iter().any(|u| u.name == line.name);
            if from_update {
                prop_assert!(update.contains(line));
            } else {
                prop_assert!(stored.contains(line));
            }
        }
        prop_assert_eq!(
            merged.len(),
            update.len() + stored.iter().filter(|s| !update.iter().any(|u| u.name == s.name)).count()
        );
        prop_assert!(merged.windows(2).all(|w| w[0].name <= w[1].name));
    }

    #[test]
    fn legacy_prop_no_panic_on_any_body(
        body in prop_oneof![
            any::<Vec<u8>>(),
            "[{}\\[\\]\":,a-z0-9 \\\\]{0,64}".prop_map(String::into_bytes),
        ],
        max_bytes in 0_usize..128,
        max_depth in 0_usize..6,
    ) {
        if let Ok(read) = read_data_body(&body, &BodyLimits::new(max_bytes, max_depth)) {
            prop_assert!(body.len() <= max_bytes);
            let decoded: usize = read.data().iter().map(|b| b.key.len() + b.value.len()).sum();
            prop_assert!(decoded <= body.len());
        }
    }

    #[test]
    fn legacy_prop_body_round_trip(
        entries in prop::collection::btree_map(any::<String>(), any::<String>(), 0..8),
        order in any::<u64>(),
        sha in any::<String>(),
    ) {
        // A server order that is not the sorted order of the keys.
        let mut data: Vec<(String, String)> = entries.into_iter().collect();
        if !data.is_empty() {
            let len = data.len();
            data.rotate_left(usize::try_from(order % len as u64).unwrap());
        }
        let members: Vec<String> = data
            .iter()
            .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), serde_json::to_string(v).unwrap()))
            .collect();
        let text = format!(
            "{{\"diff\":\"\",\"data\":{{{}}},\"html-sha1\":{}}}",
            members.join(","),
            serde_json::to_string(&sha).unwrap()
        );
        let read = read_data_body(text.as_bytes(), &BodyLimits::new(text.len(), 2)).unwrap();
        let got: Vec<(String, String)> = read
            .data()
            .iter()
            .map(|b| (b.key.clone(), b.value.clone()))
            .collect();
        prop_assert_eq!(got, data);
        prop_assert_eq!(read.html_sha1(), sha.as_str());
        prop_assert_eq!(read.diff(), Some(""));
    }

    #[test]
    fn legacy_prop_message_is_json(
        diff in prop::collection::btree_map(any::<String>(), any::<String>(), 0..4),
        fields in field_list(8),
        src in prop::sample::select(vec![-1_i64, 200, 304, 1000, 2000]),
        fin in prop::sample::select(vec![-1_i64, 200, 304, 1000, 2000]),
        reload in any::<bool>(),
    ) {
        let diff: Vec<DataBlock> = diff
            .into_iter()
            .map(|(key, value)| DataBlock { key, value })
            .collect();
        let input = MessageInput {
            src: ResultCode::from_value(src).unwrap(),
            final_code: ResultCode::from_value(fin).unwrap(),
            diff: &diff,
            fields: &fields,
            reload,
        };
        let Some(text) = message(&input) else {
            prop_assert!(fin == -1 || (fin == 200 && diff.is_empty()));
            return Ok(());
        };
        prop_assert!(!text.bytes().any(|b| b < 0x20), "a raw control byte");
        let json: Value = serde_json::from_str(&text).unwrap();
        prop_assert_eq!(json["code"].as_i64(), Some(fin));
        prop_assert_eq!(json["srcCode"].as_i64(), Some(src));
        let expected_reload = if reload { "true" } else { "false" };
        prop_assert_eq!(json["extra"]["isReload"].as_str(), Some(expected_reload));
        match json.get("result") {
            None => prop_assert!(fin != 200),
            Some(result) => {
                prop_assert_eq!(fin, 200);
                let inner: Value = serde_json::from_str(result.as_str().unwrap()).unwrap();
                let object = inner.as_object().unwrap();
                prop_assert_eq!(object.len(), diff.len());
                for block in &diff {
                    prop_assert_eq!(object[&block.key].as_str(), Some(block.value.as_str()));
                }
            }
        }
    }

    #[test]
    fn legacy_prop_bare_etag_removes_weak_prefix_and_quotes(inner in any::<String>()) {
        let weak = format!("W/\"{inner}\"");
        prop_assert_eq!(bare_etag(&weak), inner.as_str());
        let strong = format!("\"{inner}\"");
        prop_assert_eq!(bare_etag(&strong), inner.as_str());
        let bare = bare_etag(&inner);
        prop_assert!(inner.contains(bare));
        prop_assert!(bare.len() <= inner.len());
    }

    #[test]
    fn legacy_prop_request_never_adds_a_line(cookie in any::<String>(), with_entry in any::<bool>()) {
        let entry = with_entry.then(|| EntryValidators {
            etag: sha1_hex(b"p"),
            template_tag: sha1_hex(b"t"),
        });
        let request = LegacyRequest { accept_diff: true, entry, cookie: Some(&cookie) };
        let control = cookie.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f);
        match request_fields(&request) {
            Ok(fields) => {
                prop_assert!(!control);
                prop_assert_eq!(fields.len(), 1 + 2 * usize::from(with_entry) + usize::from(!cookie.is_empty()));
                for f in &fields {
                    prop_assert!(!f.value.contains(['\r', '\n', '\0']));
                }
            }
            Err(_) => prop_assert!(control),
        }
    }
}

/// The generator of `legacy_prop_head_has_one_value_per_field` reaches both
/// answers: with the fixed seed and 512 cases, 229 heads pass and 283 fail
/// (counted on 2026-11-23). The test asserts at least a quarter each way.
#[test]
fn legacy_prop_near_legacy_heads_reach_both_answers() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::new(config(512, 0x1e9a_c001));
    let strategy = near_legacy_head();
    let mut accepted = 0_u32;
    let mut rejected = 0_u32;
    for _ in 0..512 {
        let fields = strategy.new_tree(&mut runner).unwrap().current();
        if is_legacy_response(&fields) {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    assert!(accepted >= 128, "{accepted} accepted");
    assert!(rejected >= 128, "{rejected} rejected");
}
