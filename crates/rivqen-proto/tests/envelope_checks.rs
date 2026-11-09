// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Envelope checks 4 to 6 and the size part of check 10 of diff-engine
//! section 3, against the schema examples of `spec/rqp/examples`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::fs;
use std::path::{Path, PathBuf};

use rivqen_integrity::Binding;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::envelope::{EnvelopeError, check_content_bytes, envelope_limit, parse_envelope};
use rivqen_proto::json::{JsonRule, SchemaKeyword};
use rivqen_proto::types::{
    BlockId, Limits, MAX_BLOCK_BYTES, MAX_PATCH_BYTES, PatchEnvelope, PatchOp,
};

const BINDING: &str = "d1.orrx-_-Iflnnjs7DvvuOOdxW3FzbZpU_PTlR0jdWw54";

fn examples(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/rqp/examples")
        .join(kind)
        .join("patch")
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

fn parse(text: &str) -> Result<PatchEnvelope, EnvelopeError> {
    parse_envelope(text.as_bytes(), MAX_PATCH_BYTES, None)
}

fn envelope(operations: &str, members: &str) -> String {
    format!(r#"{{"binding":"{BINDING}","operations":[{operations}]{members}}}"#)
}

const PRICE: &str = r#"{"block_id":"price","content":"130.00"}"#;

fn schema_error(text: &str) -> (SchemaKeyword, String) {
    match parse(text) {
        Err(EnvelopeError::Schema(v)) => (v.keyword(), v.instance_path()),
        other => panic!("schema error expected, got {other:?}"),
    }
}

#[test]
fn envelope_examples() {
    let valid = json_files(&examples("valid"));
    assert_eq!(valid.len(), 5);
    for file in &valid {
        let bytes = fs::read(file).unwrap();
        parse_envelope(&bytes, MAX_PATCH_BYTES, None)
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    }
    let invalid = json_files(&examples("invalid"));
    assert_eq!(invalid.len(), 24);
    for file in &invalid {
        let name = file.file_name().unwrap().to_str().unwrap();
        let wrapper: serde_json::Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        // No instance has a duplicate name or a number that serde_json writes
        // in another form, so the defect survives the round trip.
        let text = serde_json::to_vec(&wrapper["instance"]).unwrap();
        let error = parse_envelope(&text, MAX_PATCH_BYTES, None).expect_err(name);
        assert_eq!(error.code(), ErrorCode::PatchInvalid, "{name}");
        let EnvelopeError::Schema(v) = error else {
            panic!("{name}: schema error expected, got {error:?}");
        };
        assert_eq!(
            v.keyword().as_str(),
            wrapper["expect"]["keyword"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            v.instance_path(),
            wrapper["expect"]["instancePath"].as_str().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn envelope_reads_binding_and_operations_in_order() {
    let bytes = fs::read(examples("valid").join("shop-all.json")).unwrap();
    let env = parse_envelope(&bytes, MAX_PATCH_BYTES, None).unwrap();
    assert_eq!(
        env.binding,
        Binding::parse("d1.EcTw23uAwgp3LYnFbLmTcuAAunRBFfRRIPy_yVPbSFM").unwrap()
    );
    let ops: Vec<(&str, &str)> = env
        .operations
        .iter()
        .map(|op| (op.block_id.as_str(), op.content.as_str()))
        .collect();
    assert_eq!(
        ops,
        [
            ("title", ""),
            ("price", "130.00"),
            ("cart", "{\"items\":4}")
        ]
    );
    // Content is the decoded string (manifest-patch section 2.3, rule 6).
    let bytes = fs::read(examples("valid").join("unicode.json")).unwrap();
    let env = parse_envelope(&bytes, MAX_PATCH_BYTES, None).unwrap();
    assert_eq!(env.operations[0].content, "café 😀 <b>&amp;</b>\r\n");
    assert_eq!(
        env.operations[1].content,
        r#"{"s":"~u00e9~ud83d~ude00","n":1e400}"#.replace('~', "\\")
    );
    // Members in any order; the escaped form of a string is the same string.
    let text = format!(
        r#"{{ "operations" : [ {{"content":"~u003cb>","block_id":"note"}} ] , "binding" : "{BINDING}" }}"#
    )
    .replace('~', "\\");
    assert!(text.contains('\\') && !text.contains('<'));
    let env = parse(&text).unwrap();
    assert_eq!(env.operations[0].content, "<b>");
}

#[test]
fn envelope_size_limit() {
    assert_eq!(envelope_limit(MAX_PATCH_BYTES, None), MAX_PATCH_BYTES);
    assert_eq!(
        envelope_limit(MAX_PATCH_BYTES, Some(Limits::default())),
        MAX_PATCH_BYTES
    );
    let hint = |n: u32| {
        Some(Limits {
            max_patch_bytes: Some(n),
        })
    };
    assert_eq!(envelope_limit(MAX_PATCH_BYTES, hint(1024)), 1024);
    assert_eq!(
        envelope_limit(MAX_PATCH_BYTES, hint(5_242_880)),
        MAX_PATCH_BYTES
    );
    assert_eq!(envelope_limit(2000, hint(3000)), 2000);

    let text = envelope(PRICE, "");
    // Exactly at the limit E passes; one byte over fails with PATCH_TOO_LARGE.
    let mut at_limit = text.clone().into_bytes();
    at_limit.resize(1024, b'\n');
    assert!(parse_envelope(&at_limit, MAX_PATCH_BYTES, hint(1024)).is_ok());
    assert!(parse_envelope(&at_limit, 1024, None).is_ok());
    at_limit.push(b'\n');
    for (max, limits) in [
        (MAX_PATCH_BYTES, hint(1024)),
        (1024, None),
        (1024, hint(4096)),
    ] {
        let e = parse_envelope(&at_limit, max, limits).unwrap_err();
        assert_eq!(e, EnvelopeError::TooLarge);
        assert_eq!(e.code(), ErrorCode::PatchTooLarge);
    }
    // The size check runs before the JSON parse.
    let garbage = vec![0xff_u8; 1025];
    assert_eq!(
        parse_envelope(&garbage, MAX_PATCH_BYTES, hint(1024)),
        Err(EnvelopeError::TooLarge)
    );
    let garbage = vec![b'{'; MAX_PATCH_BYTES + 1];
    assert_eq!(
        parse_envelope(&garbage, MAX_PATCH_BYTES, None),
        Err(EnvelopeError::TooLarge)
    );
    // A limit of 0 rejects every body that is not empty.
    assert_eq!(
        parse_envelope(b"{}", 0, None).unwrap_err(),
        EnvelopeError::TooLarge
    );
    assert!(matches!(
        parse_envelope(b"", 0, None),
        Err(EnvelopeError::Json(e)) if e.rule() == JsonRule::Syntax
    ));
}

#[test]
fn envelope_content_bytes_limit() {
    let id = BlockId::parse("a").unwrap();
    let op = |content: String| PatchOp {
        block_id: id.clone(),
        content,
    };
    let binding = Binding::parse(BINDING).unwrap();
    let mut env = PatchEnvelope {
        binding,
        operations: vec![op("x".into()), op("é".repeat(MAX_BLOCK_BYTES / 2))],
    };
    assert_eq!(check_content_bytes(&env), Ok(()));
    env.operations
        .push(op(format!("{}x", "é".repeat(MAX_BLOCK_BYTES / 2))));
    let e = check_content_bytes(&env).unwrap_err();
    assert_eq!(e, EnvelopeError::ContentTooLarge(2));
    assert_eq!(e.code(), ErrorCode::PatchTooLarge);
}

#[test]
fn envelope_content_code_point_limit() {
    // maxLength counts code points: 1 048 576 two-byte characters pass the
    // schema (and fail the byte check of check 10 later).
    let big = "é".repeat(1_048_576);
    let text = envelope(&format!(r#"{{"block_id":"a","content":"{big}"}}"#), "");
    let env = parse_envelope(text.as_bytes(), 4 * 1024 * 1024, None).unwrap();
    assert_eq!(
        check_content_bytes(&env),
        Err(EnvelopeError::ContentTooLarge(0))
    );
    let bigger = "a".repeat(1_048_577);
    let text = envelope(&format!(r#"{{"block_id":"a","content":"{bigger}"}}"#), "");
    match parse_envelope(text.as_bytes(), 4 * 1024 * 1024, None) {
        Err(EnvelopeError::Schema(v)) => {
            assert_eq!(v.keyword(), SchemaKeyword::MaxLength);
            assert_eq!(v.instance_path(), "/operations/0/content");
        }
        other => panic!("maxLength expected, got {other:?}"),
    }
    let exact = "a".repeat(1_048_576);
    let text = envelope(&format!(r#"{{"block_id":"a","content":"{exact}"}}"#), "");
    assert!(parse_envelope(text.as_bytes(), 4 * 1024 * 1024, None).is_ok());
}

#[test]
fn envelope_old_form() {
    for member in [
        r#","type":"rivqen.patch""#,
        r#","protocol":"rqp/1""#,
        r#","base_revision":"r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8""#,
        r#","template_revision":"t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM""#,
        r#","next_revision":"r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI""#,
        r#","page_id":"catalog""#,
        r#","sequence":1"#,
    ] {
        assert_eq!(
            schema_error(&envelope(PRICE, member)),
            (SchemaKeyword::AdditionalProperties, String::new()),
            "{member}"
        );
    }
    for member in [
        r#""op":"replace_block""#,
        r#""format":"html""#,
        r#""sha256":"bB9ScYcQQzKWqPCBW62MTsc9Ff_djve7Denf3YMoCUg""#,
    ] {
        let ops = format!(r#"{PRICE},{{"block_id":"cart","content":"x",{member}}}"#);
        assert_eq!(
            schema_error(&envelope(&ops, "")),
            (
                SchemaKeyword::AdditionalProperties,
                "/operations/1".to_owned()
            ),
            "{member}"
        );
    }
}

#[test]
fn envelope_operation_count_bounds() {
    let ops = |n: usize| {
        (0..n)
            .map(|i| format!(r#"{{"block_id":"b{i}","content":""}}"#))
            .collect::<Vec<_>>()
            .join(",")
    };
    assert_eq!(
        parse(&envelope(&ops(256), "")).unwrap().operations.len(),
        256
    );
    assert_eq!(
        schema_error(&envelope(&ops(257), "")),
        (SchemaKeyword::MaxItems, "/operations".to_owned())
    );
    assert_eq!(
        schema_error(&envelope("", "")),
        (SchemaKeyword::MinItems, "/operations".to_owned())
    );
    assert_eq!(parse(&envelope(&ops(1), "")).unwrap().operations.len(), 1);
}

#[test]
fn envelope_schema_failures() {
    for (text, keyword, at) in [
        ("[]".to_owned(), SchemaKeyword::Type, ""),
        (
            r#"{"operations":[{"block_id":"a","content":""}]}"#.to_owned(),
            SchemaKeyword::Required,
            "",
        ),
        (
            format!(r#"{{"binding":"{BINDING}"}}"#),
            SchemaKeyword::Required,
            "",
        ),
        (
            format!(r#"{{"binding":7,"operations":[{PRICE}]}}"#),
            SchemaKeyword::Type,
            "/binding",
        ),
        (
            format!(
                r#"{{"binding":"{}","operations":[{PRICE}]}}"#,
                BINDING.replace("d1.", "D1.")
            ),
            SchemaKeyword::Pattern,
            "/binding",
        ),
        (envelope(r#""x""#, ""), SchemaKeyword::Type, "/operations/0"),
        (
            envelope(&format!("{PRICE},[]"), ""),
            SchemaKeyword::Type,
            "/operations/1",
        ),
        (
            envelope(r#"{"block_id":null,"content":""}"#, ""),
            SchemaKeyword::Type,
            "/operations/0/block_id",
        ),
        (
            envelope(r#"{"block_id":"a_","content":7}"#, ""),
            SchemaKeyword::Type,
            "/operations/0/content",
        ),
        (
            envelope(r#"{"block_id":"a b","content":""}"#, ""),
            SchemaKeyword::Pattern,
            "/operations/0/block_id",
        ),
        (
            envelope(r#"{"content":""}"#, ""),
            SchemaKeyword::Required,
            "/operations/0",
        ),
        (
            envelope(r#"{"block_id":"a"}"#, ""),
            SchemaKeyword::Required,
            "/operations/0",
        ),
        (
            format!(r#"{{"binding":"{BINDING}","operations":"x"}}"#),
            SchemaKeyword::Type,
            "/operations",
        ),
    ] {
        assert_eq!(schema_error(&text), (keyword, at.to_owned()), "{text}");
    }
    // A 64-character id is the longest (M-15).
    let id = "a".repeat(64);
    assert!(
        parse(&envelope(
            &format!(r#"{{"block_id":"{id}","content":""}}"#),
            ""
        ))
        .is_ok()
    );
}

#[test]
fn envelope_json_rules_apply() {
    let rule = |text: &str| match parse(text) {
        Err(EnvelopeError::Json(e)) => e.rule(),
        other => panic!("JSON error expected, got {other:?}"),
    };
    assert_eq!(
        rule(&envelope(PRICE, &format!(r#","binding":"{BINDING}""#))),
        JsonRule::Duplicate
    );
    assert_eq!(
        rule(&envelope(
            r#"{"block_id":"a","content":"","content":""}"#,
            ""
        )),
        JsonRule::Duplicate
    );
    assert_eq!(
        rule(&envelope(r#"{"block_id":"a","content":"\udc00"}"#, "")),
        JsonRule::Surrogate
    );
    assert_eq!(
        rule(&format!("\u{feff}{}", envelope(PRICE, ""))),
        JsonRule::Bom
    );
    assert_eq!(
        rule(&format!("{} x", envelope(PRICE, ""))),
        JsonRule::Syntax
    );
    assert_eq!(
        rule(&envelope(PRICE, "").replace(']', ",]")),
        JsonRule::Syntax
    );
    let e = parse(&envelope(PRICE, &format!(r#","binding":"{BINDING}""#))).unwrap_err();
    assert_eq!(e.code(), ErrorCode::PatchInvalid);
    assert!(core::error::Error::source(&e).is_some());
}

#[test]
fn envelope_errors_hold_no_content() {
    let marker = "SECRETMARKER";
    let texts = [
        envelope(
            &format!(r#"{{"block_id":"a","content":"{marker}","{marker}":1}}"#),
            "",
        ),
        envelope(&format!(r#"{{"block_id":"{marker}","content":""}}"#), ""),
        envelope(
            &format!(r#"{{"block_id":"a","content":"{marker}\ud800"}}"#),
            "",
        ),
    ];
    for text in &texts {
        let e = parse(text).unwrap_err();
        assert!(!format!("{e} {e:?}").contains(marker), "{e}");
    }
    let env = parse(&envelope(
        &format!(r#"{{"block_id":"a","content":"{marker}"}}"#),
        "",
    ))
    .unwrap();
    assert!(!format!("{env:?}").contains(marker));
    assert_eq!(
        EnvelopeError::ContentTooLarge(3).to_string(),
        "PATCH_TOO_LARGE: operation content larger than maxBlockBytes (operation 3)"
    );
    assert_eq!(
        EnvelopeError::TooLarge.to_string(),
        "PATCH_TOO_LARGE: envelope larger than min(maxPatchBytes, max_patch_bytes)"
    );
    assert_eq!(
        schema_error(&envelope(r#"{"block_id":"a","content":"","x":1}"#, "")).1,
        "/operations/0"
    );
    assert_eq!(
        parse(&envelope(r#"{"block_id":"a","content":"","x":1}"#, ""))
            .unwrap_err()
            .to_string(),
        "PATCH_INVALID: envelope fails the schema: schema keyword additionalProperties fails at '/operations/0'"
    );
    assert!(core::error::Error::source(&EnvelopeError::TooLarge).is_none());
}
