// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The data body of a legacy data update (legacy client CL-29, legacy wire
//! §5.4 and C-11).

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::json::JsonRule;
use rivqen_proto_legacy::body::{
    BodyError, BodyLimits, BodyMember, DATA_BODY_DEPTH, read_data_body,
};

const LIMITS: BodyLimits = BodyLimits::new(1 << 20, 8);

/// The body of the example of legacy wire §5.4.
const EXAMPLE: &str = r#"{
  "data": {
    "{title}": "<title>Shop</title>",
    "{price}": "<!--sonicdiff-price--><p class=\"price\">120.00</p><!--sonicdiff-price-end-->"
  },
  "template-tag": "3e3184cbd12e5294959effe6f2d9985f50f4ea5d",
  "html-sha1": "0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33",
  "diff": ""
}"#;

fn read(text: &str) -> Result<rivqen_proto_legacy::body::DataBody, BodyError> {
    read_data_body(text.as_bytes(), &LIMITS)
}

fn json_rule(result: Result<rivqen_proto_legacy::body::DataBody, BodyError>) -> JsonRule {
    match result {
        Err(BodyError::Json(error)) => error.rule(),
        other => panic!("not a JSON error: {other:?}"),
    }
}

#[test]
fn legacy_body_example_gives_the_members_in_text_order() {
    let body = read(EXAMPLE).unwrap();
    let keys: Vec<&str> = body.data().iter().map(|b| b.key.as_str()).collect();
    assert_eq!(keys, ["{title}", "{price}"]);
    assert_eq!(body.data()[0].value, "<title>Shop</title>");
    assert_eq!(
        body.data()[1].value,
        "<!--sonicdiff-price--><p class=\"price\">120.00</p><!--sonicdiff-price-end-->"
    );
    assert_eq!(body.html_sha1(), "0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33");
    assert_eq!(
        body.template_tag(),
        Some("3e3184cbd12e5294959effe6f2d9985f50f4ea5d")
    );
    assert_eq!(body.diff(), Some(""));
    assert_eq!(body.into_data().len(), 2);
}

#[test]
fn legacy_body_keeps_the_server_order_not_a_sorted_order() {
    let body = read(r#"{"data":{"{z}":"1","{a}":"2","{m}":"3"},"html-sha1":"h"}"#).unwrap();
    let keys: Vec<&str> = body.data().iter().map(|b| b.key.as_str()).collect();
    assert_eq!(keys, ["{z}", "{a}", "{m}"]);
}

#[test]
fn legacy_body_decodes_escapes() {
    let body = read(r#"{"html-sha1":"h","data":{"{ab}":"é\n\/\"😀"}}"#).unwrap();
    assert_eq!(body.data()[0].key, "{ab}");
    assert_eq!(body.data()[0].value, "é\n/\"\u{1f600}");
}

#[test]
fn legacy_body_template_tag_and_diff_are_optional() {
    let body = read(r#"{"data":{},"html-sha1":"h"}"#).unwrap();
    assert_eq!(body.data(), &[]);
    assert_eq!(body.template_tag(), None);
    assert_eq!(body.diff(), None);
}

#[test]
fn legacy_body_other_members_are_ignored() {
    let body = read(
        r#"{"x":{"a":[1,{"b":null}],"c":true},"data":{"{a}":"v"},"y":[],"z":"s","html-sha1":"h","n":0}"#,
    )
    .unwrap();
    assert_eq!(body.data().len(), 1);
    assert_eq!(body.html_sha1(), "h");
}

#[test]
fn legacy_body_without_data_or_html_sha1_is_an_error() {
    assert_eq!(read(r#"{"html-sha1":"h"}"#), Err(BodyError::NoData));
    assert_eq!(read(r#"{"data":{}}"#), Err(BodyError::NoHtmlSha1));
    assert_eq!(read("{}"), Err(BodyError::NoData));
}

#[test]
fn legacy_body_value_that_is_not_a_string_is_an_error() {
    for value in ["1", "true", "null", "[]", "{}", r#"["a"]"#] {
        let text = format!(r#"{{"data":{{"{{a}}":"x","{{b}}":{value}}},"html-sha1":"h"}}"#);
        assert_eq!(read(&text), Err(BodyError::ValueNotString), "{value}");
    }
}

#[test]
fn legacy_body_data_that_is_not_an_object_is_an_error() {
    for value in [r#""x""#, "[]", "null", "1", "false"] {
        let text = format!(r#"{{"data":{value},"html-sha1":"h"}}"#);
        assert_eq!(read(&text), Err(BodyError::DataNotObject), "{value}");
    }
}

#[test]
fn legacy_body_known_member_that_is_not_a_string_is_an_error() {
    let cases = [
        (
            r#"{"data":{},"html-sha1x":1,"html-sha1":[]}"#,
            "html-sha1",
            BodyMember::HtmlSha1,
        ),
        (
            r#"{"data":{},"html-sha1":"h","template-tag":0}"#,
            "template-tag",
            BodyMember::TemplateTag,
        ),
        (
            r#"{"data":{},"html-sha1":"h","diff":{}}"#,
            "diff",
            BodyMember::Diff,
        ),
    ];
    for (text, name, member) in cases {
        assert_eq!(read(text), Err(BodyError::NotString(member)), "{name}");
        assert_eq!(member.name(), name);
    }
}

#[test]
fn legacy_body_top_level_that_is_not_an_object_is_an_error() {
    for text in ["[]", r#""data""#, "1", "null", "[{}]"] {
        assert_eq!(read(text), Err(BodyError::NotObject), "{text}");
    }
}

#[test]
fn legacy_body_duplicate_member_is_an_error() {
    let top = r#"{"data":{},"data":{},"html-sha1":"h"}"#;
    assert_eq!(json_rule(read(top)), JsonRule::Duplicate);
    let in_data = r#"{"data":{"{a}":"1","{a}":"2"},"html-sha1":"h"}"#;
    assert_eq!(json_rule(read(in_data)), JsonRule::Duplicate);
    let escaped = r#"{"data":{"{a}":"1","{a}":"2"},"html-sha1":"h"}"#;
    assert_eq!(json_rule(read(escaped)), JsonRule::Duplicate);
    let sha = r#"{"data":{},"html-sha1":"h","html-sha1":"h"}"#;
    assert_eq!(json_rule(read(sha)), JsonRule::Duplicate);
}

#[test]
fn legacy_body_invalid_utf8_is_an_error() {
    let mut bytes = br#"{"data":{"{a}":""#.to_vec();
    bytes.extend_from_slice(b"\xff\"},\"html-sha1\":\"h\"}");
    match read_data_body(&bytes, &LIMITS) {
        Err(BodyError::Json(error)) => assert_eq!(error.rule(), JsonRule::Encoding),
        other => panic!("{other:?}"),
    }
}

#[test]
fn legacy_body_byte_order_mark_is_an_error() {
    let text = format!("\u{feff}{EXAMPLE}");
    assert_eq!(json_rule(read(&text)), JsonRule::Bom);
}

#[test]
fn legacy_body_broken_json_is_an_error() {
    let cases = [
        ("", JsonRule::Syntax),
        (r#"{"data":{},"html-sha1":"h""#, JsonRule::Syntax),
        (r#"{"data":{},"html-sha1":"h"} {}"#, JsonRule::Syntax),
        (r#"{"data":{},"html-sha1":"h",}"#, JsonRule::Syntax),
        (
            r#"{"data":{"{a}":"\ud800"},"html-sha1":"h"}"#,
            JsonRule::Surrogate,
        ),
        (r#"{"n":1.5,"data":{},"html-sha1":"h"}"#, JsonRule::Integer),
        (r#"{"n":-1,"data":{},"html-sha1":"h"}"#, JsonRule::Integer),
    ];
    for (text, rule) in cases {
        assert_eq!(json_rule(read(text)), rule, "{text}");
    }
}

#[test]
fn legacy_body_trailing_white_space_passes() {
    assert!(read(&format!(" \n{EXAMPLE}\r\n\t ")).is_ok());
}

#[test]
fn legacy_body_size_limit_is_checked_first() {
    let len = EXAMPLE.len();
    assert!(read_data_body(EXAMPLE.as_bytes(), &BodyLimits::new(len, 8)).is_ok());
    assert_eq!(
        read_data_body(EXAMPLE.as_bytes(), &BodyLimits::new(len - 1, 8)),
        Err(BodyError::TooLarge)
    );
    // Over the limit and not JSON: the size test comes first.
    assert_eq!(
        read_data_body(b"\xff\xff\xff", &BodyLimits::new(2, 8)),
        Err(BodyError::TooLarge)
    );
}

#[test]
fn legacy_body_depth_limit_is_checked() {
    let limits = BodyLimits::new(1 << 20, DATA_BODY_DEPTH);
    assert!(read_data_body(EXAMPLE.as_bytes(), &limits).is_ok());
    let shallow = BodyLimits::new(1 << 20, DATA_BODY_DEPTH - 1);
    assert_eq!(
        json_rule(read_data_body(EXAMPLE.as_bytes(), &shallow)),
        JsonRule::Depth
    );
    let deep = r#"{"x":[[[]]],"data":{},"html-sha1":"h"}"#;
    assert_eq!(
        json_rule(read_data_body(
            deep.as_bytes(),
            &BodyLimits::new(1 << 20, 3)
        )),
        JsonRule::Depth
    );
    assert!(read_data_body(deep.as_bytes(), &BodyLimits::new(1 << 20, 4)).is_ok());
}

#[test]
fn legacy_body_limits_report_their_values() {
    let limits = BodyLimits::new(7, 3);
    assert_eq!((limits.max_bytes(), limits.max_depth()), (7, 3));
}

#[test]
fn legacy_body_error_codes() {
    assert_eq!(BodyError::TooLarge.code(), ErrorCode::DocumentTooLarge);
    for error in [
        BodyError::NotObject,
        BodyError::NoData,
        BodyError::DataNotObject,
        BodyError::ValueNotString,
        BodyError::NoHtmlSha1,
        BodyError::NotString(BodyMember::Diff),
    ] {
        assert_eq!(error.code(), ErrorCode::IntegrityError, "{error:?}");
    }
    let json = read("{,").unwrap_err();
    assert_eq!(json.code(), ErrorCode::IntegrityError);
    assert!(std::error::Error::source(&json).is_some());
    assert!(std::error::Error::source(&BodyError::NoData).is_none());
}

#[test]
fn legacy_body_debug_and_errors_hold_no_body_bytes() {
    let secret = r#"{"data":{"{secret-key}":"secret-value"},"html-sha1":"secret-sha","diff":"secret-diff","template-tag":"secret-tag"}"#;
    let body = read(secret).unwrap();
    let text = format!("{body:?}");
    assert!(!text.contains("secret"), "{text}");
    assert!(text.contains("data_len: 1"), "{text}");
    let failures = [
        r#"{"data":{"{secret}":1},"html-sha1":"h"}"#,
        r#"{"data":{},"html-sha1":"h","diff":["secret"]}"#,
        r#"{"data":{"secret":"x","secret":"y"},"html-sha1":"h"}"#,
        r#"{"secret":"x""#,
    ];
    for failure in failures {
        let error = read(failure).unwrap_err();
        let text = format!("{error:?} {error}");
        assert!(!text.contains("secret"), "{text}");
    }
    assert_eq!(
        BodyError::NotString(BodyMember::Diff).to_string(),
        "a member of the legacy data body is not a string: diff"
    );
}

/// The text of a JSON error has the reason of the body error and then the
/// text of the reader error (its source).
#[test]
fn legacy_body_json_error_text_has_the_reader_error() {
    let error = read_data_body(b"{", &LIMITS).unwrap_err();
    let BodyError::Json(json) = error else {
        panic!("{error:?}");
    };
    assert_eq!(error.to_string(), format!("{}: {json}", error.reason()));
}
