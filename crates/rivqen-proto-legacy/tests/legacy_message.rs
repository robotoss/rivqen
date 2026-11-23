// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The page message (legacy client CL-23 item 1, CL-24, CL-28, CL-35): the
//! cases that the function fixtures do not reach (a refresh, a diff with
//! more than one member, escapes in `extra`).

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use rivqen_model::http::FieldLine;
use rivqen_proto_legacy::message::{MessageInput, ResultCode, message};
use rivqen_template::legacy::DataBlock;

fn lines(pairs: &[(&str, &str)]) -> Vec<FieldLine> {
    pairs.iter().map(|(n, v)| FieldLine::new(*n, *v)).collect()
}

fn block(key: &str, value: &str) -> DataBlock {
    DataBlock {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

fn text(
    src: ResultCode,
    final_code: ResultCode,
    diff: &[DataBlock],
    fields: &[FieldLine],
    reload: bool,
) -> Option<String> {
    message(&MessageInput {
        src,
        final_code,
        diff,
        fields,
        reload,
    })
}

#[test]
fn legacy_message_result_keeps_the_diff_order() {
    let diff = [block("{z}", "1"), block("{a}", "2")];
    let got = text(
        ResultCode::DataUpdate,
        ResultCode::DataUpdate,
        &diff,
        &[],
        false,
    )
    .unwrap();
    assert_eq!(
        got,
        r#"{"result":"{\"{z}\":\"1\",\"{a}\":\"2\"}","code":200,"srcCode":200,"extra":{"isReload":"false"}}"#
    );
}

#[test]
fn legacy_message_refresh_sets_is_reload() {
    let got = text(
        ResultCode::NotModified,
        ResultCode::NotModified,
        &[],
        &[],
        true,
    )
    .unwrap();
    assert_eq!(
        got,
        r#"{"code":304,"srcCode":304,"extra":{"isReload":"true"}}"#
    );
}

#[test]
fn legacy_message_no_text_for_no_result_or_empty_update() {
    assert_eq!(
        text(ResultCode::NoResult, ResultCode::NoResult, &[], &[], false),
        None
    );
    let diff = [block("{a}", "1")];
    assert_eq!(
        text(
            ResultCode::DataUpdate,
            ResultCode::NoResult,
            &diff,
            &[],
            false
        ),
        None
    );
    assert_eq!(
        text(
            ResultCode::DataUpdate,
            ResultCode::DataUpdate,
            &[],
            &[],
            false
        ),
        None
    );
}

#[test]
fn legacy_message_has_no_result_unless_final_is_200() {
    let diff = [block("{a}", "1")];
    for final_code in [
        ResultCode::NotModified,
        ResultCode::FirstLoad,
        ResultCode::TemplateChange,
    ] {
        let got = text(ResultCode::DataUpdate, final_code, &diff, &[], false).unwrap();
        assert!(!got.contains("result"), "{got}");
        assert!(
            got.starts_with(&format!("{{\"code\":{},", final_code.value())),
            "{got}"
        );
    }
}

#[test]
fn legacy_message_extra_takes_the_first_line_and_escapes() {
    let fields = lines(&[
        ("ETag", " \"e\\1\" "),
        ("etag", "second"),
        ("Template-Tag", "t\u{1}"),
        ("CACHE-OFFLINE", "Store"),
    ]);
    let got = text(
        ResultCode::FirstLoad,
        ResultCode::NotModified,
        &[],
        &fields,
        false,
    )
    .unwrap();
    assert_eq!(
        got,
        r#"{"code":304,"srcCode":1000,"extra":{"eTag":"\"e\\1\"","template-tag":"t\u0001","cache-offline":"Store","isReload":"false"}}"#
    );
}

#[test]
fn legacy_message_extra_has_a_present_empty_field() {
    let fields = lines(&[("etag", "")]);
    let got = text(
        ResultCode::FirstLoad,
        ResultCode::FirstLoad,
        &[],
        &fields,
        false,
    )
    .unwrap();
    assert_eq!(
        got,
        r#"{"code":1000,"srcCode":1000,"extra":{"eTag":"","isReload":"false"}}"#
    );
}

#[test]
fn legacy_message_sonic_etag_key_gives_the_key_as_written() {
    let fields = lines(&[
        ("sonic-etag-key", "Custom-ETag"),
        ("custom-etag", "c1"),
        ("etag", "e1"),
    ]);
    let got = text(
        ResultCode::FirstLoad,
        ResultCode::FirstLoad,
        &[],
        &fields,
        false,
    )
    .unwrap();
    assert_eq!(
        got,
        r#"{"code":1000,"srcCode":1000,"extra":{"Custom-ETag":"c1","isReload":"false"}}"#
    );
}

#[test]
fn legacy_message_codes_map_both_ways() {
    let all = [
        (ResultCode::NoResult, -1),
        (ResultCode::DataUpdate, 200),
        (ResultCode::NotModified, 304),
        (ResultCode::FirstLoad, 1000),
        (ResultCode::TemplateChange, 2000),
    ];
    for (code, value) in all {
        assert_eq!(code.value(), value);
        assert_eq!(ResultCode::from_value(i64::from(value)), Some(code));
    }
    for other in [0, 1, 199, 201, 404, 999, 1001, 3000, -2] {
        assert_eq!(ResultCode::from_value(other), None, "{other}");
    }
}

#[test]
fn legacy_message_input_debug_has_no_content() {
    let diff = [block("{secret-key}", "secret-value")];
    let fields = lines(&[("etag", "secret-etag")]);
    let input = MessageInput {
        src: ResultCode::DataUpdate,
        final_code: ResultCode::DataUpdate,
        diff: &diff,
        fields: &fields,
        reload: false,
    };
    let debug = format!("{input:?}");
    assert!(!debug.contains("secret"), "{debug}");
    assert!(debug.contains("diff_len: 1"), "{debug}");
}
