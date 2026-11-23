// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `rqp::validate`: checks 4 to 13 of diff engine section 3 in their order.
//! Each test gives a body that fails one check, and where possible also a
//! later check, so the first failure must win. The errors have the check
//! number and the catalog code and no input bytes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod rqp_common;

use std::error::Error as _;

use rivqen_diff::rqp::{DeltaRequest, PatchLimits, ValidateError, validate};
use rivqen_integrity::b64u_encode;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::delta::DeltaError;
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::patch::PatchError;

use rqp_common::{
    NONCE, OTHER_NONCE, OTHER_URL, PAGE, Snap, UNBOUND, URL, bound_delta, delta, envelope, limits,
    request, run, server_delta,
};

/// The check number and the code of a failed `validate`.
fn failure(result: Result<rivqen_diff::rqp::ValidatedPatch, ValidateError>) -> (u8, ErrorCode) {
    let error = result.expect_err("expected a failed check");
    (error.check(), error.code())
}

fn base() -> Snap {
    Snap::new(PAGE)
}

/// The base page with the content of the `price` block replaced.
fn with_price(price: &str) -> Snap {
    Snap::new(&PAGE.replace(">120.00<", &format!(">{price}<")))
}

/// A body with the right structure for the base page and a binding that
/// fits no request.
fn unbound(content: &str) -> Vec<u8> {
    envelope(UNBOUND, &[("price".to_owned(), content.to_owned())])
}

#[test]
fn rqp_check_order_4_size_before_json() {
    // 1 025 bytes that are not JSON, over a limit of 1 024: check 4 runs
    // before the parse.
    let limits = PatchLimits::new(1024, TemplateLimits::default());
    let body = vec![b'{'; 1025];
    let result = validate(&body, &base().stored(), &request(URL, &NONCE), &limits);
    assert_eq!(failure(result), (4, ErrorCode::PatchTooLarge));
}

#[test]
fn rqp_check_order_4_limit_is_inclusive() {
    // The body of a valid delta at exactly the limit passes check 4; one byte
    // less as the limit fails it.
    let (base, next) = (base(), with_price("130.00"));
    let body = delta(&base, &next);
    let at = PatchLimits::new(body.len(), TemplateLimits::default());
    let below = PatchLimits::new(body.len() - 1, TemplateLimits::default());
    let stored = base.stored();
    let req = request(URL, &NONCE);
    assert!(validate(&body, &stored, &req, &at).is_ok());
    assert_eq!(
        failure(validate(&body, &stored, &req, &below)),
        (4, ErrorCode::PatchTooLarge)
    );
}

#[test]
fn rqp_check_order_4_patch_limit_is_capped() {
    // A configured limit over 5 MiB gives 5 MiB: a body of 5 MiB + 1 byte
    // fails check 4 before any parse.
    let limits = PatchLimits::new(usize::MAX, TemplateLimits::default());
    assert_eq!(limits.max_patch_bytes(), 5 * 1024 * 1024);
    let body = vec![b' '; 5 * 1024 * 1024 + 1];
    let result = validate(&body, &base().stored(), &request(URL, &NONCE), &limits);
    assert_eq!(failure(result), (4, ErrorCode::PatchTooLarge));
}

#[test]
fn rqp_check_order_5_json_rules() {
    let snap = base();
    // A duplicate member name.
    let body = format!(r#"{{"binding":"{UNBOUND}","binding":"{UNBOUND}","operations":[]}}"#);
    assert_eq!(
        failure(run(body.as_bytes(), &snap)),
        (5, ErrorCode::PatchInvalid)
    );
    // Bytes that are not UTF-8.
    let mut body = unbound("130.00");
    let at = body.iter().position(|&b| b == b'1').unwrap();
    body[at] = 0xFF;
    assert_eq!(failure(run(&body, &snap)), (5, ErrorCode::PatchInvalid));
}

#[test]
fn rqp_check_order_6_schema() {
    let snap = base();
    let body = br#"{"type":"patch","binding":"d1.x","operations":[]}"#;
    assert_eq!(failure(run(body, &snap)), (6, ErrorCode::PatchInvalid));
}

#[test]
fn rqp_check_order_9_operations_before_binding() {
    // An unknown block and a binding that fits nothing: check 9 wins.
    let body = envelope(UNBOUND, &[("nope".to_owned(), "x".to_owned())]);
    assert_eq!(failure(run(&body, &base())), (9, ErrorCode::PatchInvalid));
}

#[test]
fn rqp_check_order_10_block_size_from_template_limits() {
    // `maxBlockBytes` of the template limits is the limit of check 10; the
    // binding fits nothing, so check 10 runs before check 11.
    let template = TemplateLimits::new(5 * 1024 * 1024, 256, 8, 128, 256);
    let limits = PatchLimits::new(256 * 1024, template);
    let req = request(URL, &NONCE);
    let snap = base();
    let fits = validate(&unbound("12345678"), &snap.stored(), &req, &limits);
    assert_eq!(failure(fits), (11, ErrorCode::IntegrityError));
    let over = validate(&unbound("123456789"), &snap.stored(), &req, &limits);
    assert_eq!(failure(over), (10, ErrorCode::PatchTooLarge));
}

#[test]
fn rqp_check_order_11_binding_before_content() {
    // Content that check 13 rejects, with a binding of another base: the
    // binding fails first.
    let body = unbound("<script>alert(1)</script>");
    assert_eq!(
        failure(run(&body, &base())),
        (11, ErrorCode::IntegrityError)
    );
}

#[test]
fn rqp_check_order_11_request_inputs() {
    let (base, next) = (base(), with_price("130.00"));
    let body = delta(&base, &next);
    let stored = base.stored();
    let limits = limits();
    let cases = [
        ("no URL", None, Some(&NONCE)),
        ("empty URL", Some(""), Some(&NONCE)),
        ("other URL", Some(OTHER_URL), Some(&NONCE)),
        ("no nonce", Some(URL), None),
        ("other nonce", Some(URL), Some(&OTHER_NONCE)),
    ];
    for (name, final_url, nonce) in cases {
        let req = DeltaRequest { final_url, nonce };
        let result = validate(&body, &stored, &req, &limits);
        assert_eq!(failure(result), (11, ErrorCode::IntegrityError), "{name}");
    }
    // The control: the same body with the inputs of its request passes.
    let req = request(URL, &NONCE);
    assert!(validate(&body, &stored, &req, &limits).is_ok());
}

#[test]
fn rqp_check_order_11_other_base() {
    // A delta to the same next page, built for another base.
    let (base, other, next) = (base(), with_price("99.00"), with_price("130.00"));
    let body = server_delta(&other, &next, &[1], URL, &NONCE);
    assert_eq!(failure(run(&body, &base)), (11, ErrorCode::IntegrityError));
}

#[test]
fn rqp_check_order_12_markup_before_content() {
    // The content closes the block element early (M-23) and has a script:
    // the markup analysis of check 12 rejects first.
    let base = base();
    let ops = [(
        "price".to_owned(),
        "<script>alert(1)</script></p><p>".to_owned(),
    )];
    let error = run(&bound_delta(&base, &ops), &base).unwrap_err();
    assert_eq!(
        (error.check(), error.code()),
        (12, ErrorCode::PatchContentRejected)
    );
    // The early close moves the rest of the page out of the block: the
    // analysis sees another template.
    assert!(
        matches!(
            error,
            ValidateError::Patch(
                PatchError::Markup(_) | PatchError::TemplateChanged | PatchError::BlocksChanged
            )
        ),
        "{error:?}"
    );
}

#[test]
fn rqp_check_order_12_document_size_before_content() {
    // The patched document is over `maxDocumentBytes` and its content has a
    // script: the size of check 12 rejects first.
    let base = base();
    let next = with_price("<script>alert(1)</script>");
    let body = delta(&base, &next);
    let template = TemplateLimits::new(PAGE.len() + 4, 256, 1 << 20, 128, 256);
    let limits = PatchLimits::new(256 * 1024, template);
    let result = validate(&body, &base.stored(), &request(URL, &NONCE), &limits);
    assert_eq!(failure(result), (12, ErrorCode::PatchTooLarge));
}

#[test]
fn rqp_check_order_12_json_content() {
    // Empty content of a json block is not a JSON text (M-25).
    let base = base();
    let body = bound_delta(&base, &[("cart".to_owned(), String::new())]);
    let error = run(&body, &base).unwrap_err();
    assert_eq!(
        (error.check(), error.code()),
        (12, ErrorCode::PatchContentRejected)
    );
    assert!(
        matches!(error, ValidateError::Patch(PatchError::Markup(_))),
        "{error:?}"
    );
}

#[test]
fn rqp_check_order_12_stored_analysis_of_another_document() {
    // The analysis does not belong to the snapshot values: fail closed with
    // INTEGRITY_ERROR, never another page.
    let (base, next) = (base(), with_price("130.00"));
    let body = delta(&base, &next);
    let other = with_price("1.00");
    let mut stored = base.stored();
    stored.analysis = &other.analysis;
    let result = validate(&body, &stored, &request(URL, &NONCE), &limits());
    assert_eq!(failure(result), (12, ErrorCode::IntegrityError));
}

#[test]
fn rqp_check_order_13_content_rule() {
    let base = base();
    for content in [
        "<script>alert(1)</script>",
        "<b onclick=\"x()\">1</b>",
        "<a href=\"javascript:x()\">1</a>",
        "<svg><a href=\"/x\">1</a></svg>",
    ] {
        let next = with_price(content);
        let error = run(&delta(&base, &next), &base).unwrap_err();
        assert_eq!(
            (error.check(), error.code()),
            (13, ErrorCode::PatchContentRejected),
            "{content}"
        );
        assert!(matches!(
            error,
            ValidateError::Patch(PatchError::Content(_))
        ));
    }
}

#[test]
fn rqp_check_order_error_has_no_input() {
    // A page whose block ids, contents and request values are unique words;
    // no error text, Debug text or source text may contain one of them.
    let page = PAGE
        .replace("\"price\"", "\"zqblockid\"")
        .replace(">120.00<", ">zqcontent<");
    let url = "https://zqhost.example/zqpath?zqquery=1";
    let nonce = rivqen_proto::types::Nonce([0xA5; 16]);
    let nonce_text = b64u_encode(&nonce.0);
    let base = Snap::new(&page);
    let mut errors: Vec<ValidateError> = Vec::new();
    let req = request(url, &nonce);
    let limits = limits();
    let mut check = |body: &[u8], req: &DeltaRequest<'_>, limits: &PatchLimits| {
        errors.push(validate(body, &base.stored(), req, limits).unwrap_err());
    };
    // Checks 5, 6, 9, 11 (binding), 11 (no URL), 12, 13.
    let duplicate = format!(r#"{{"binding":"{UNBOUND}","binding":"zqcontent"}}"#);
    check(duplicate.as_bytes(), &req, &limits);
    check(br#"{"zqblockid":1}"#, &req, &limits);
    let unknown = envelope(UNBOUND, &[("zqunknown".to_owned(), "zqcontent".to_owned())]);
    check(&unknown, &req, &limits);
    let wrong = envelope(UNBOUND, &[("zqblockid".to_owned(), "zqcontent".to_owned())]);
    check(&wrong, &req, &limits);
    let no_url = DeltaRequest {
        final_url: None,
        nonce: Some(&nonce),
    };
    check(&wrong, &no_url, &limits);
    let next = Snap::new(&page.replace(">zqcontent<", "><script>zqcontent</script><"));
    let body = server_delta(&base, &next, &[1], url, &nonce);
    check(&body, &req, &limits);
    let tiny = PatchLimits::new(
        256 * 1024,
        TemplateLimits::new(page.len(), 256, 1 << 20, 128, 256),
    );
    check(&body, &req, &tiny);
    let checks: Vec<u8> = errors.iter().map(ValidateError::check).collect();
    assert_eq!(checks, [5, 6, 9, 11, 11, 13, 12]);
    for error in &errors {
        let mut texts = vec![error.to_string(), format!("{error:?}")];
        let mut source = error.source();
        while let Some(inner) = source {
            texts.push(inner.to_string());
            texts.push(format!("{inner:?}"));
            source = inner.source();
        }
        assert!(texts.len() > 2, "{error:?} has no source");
        for text in texts {
            for secret in ["zq", nonce_text.as_str(), "example"] {
                assert!(!text.contains(secret), "{text} has {secret}");
            }
        }
    }
}

#[test]
fn rqp_check_order_display_names_check_and_code() {
    let (base, next) = (base(), with_price("<script>alert(1)</script>"));
    let content = run(&delta(&base, &next), &base).unwrap_err();
    assert!(
        content
            .to_string()
            .starts_with("patch check 13: PATCH_CONTENT_REJECTED: "),
        "{content}"
    );
    let binding = run(&unbound("1"), &base).unwrap_err();
    assert!(
        binding
            .to_string()
            .starts_with("delta check 11: INTEGRITY_ERROR"),
        "{binding}"
    );
    assert!(matches!(
        binding,
        ValidateError::Delta(DeltaError::Integrity(_))
    ));
}
