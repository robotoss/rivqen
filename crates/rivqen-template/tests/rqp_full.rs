// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Tests of the client checks of a full document (WP-06 S2 T-01).
//!
//! Normative text: docs/engineering/protocol/manifest-patch.md section 1.7
//! (client items 1 to 5) and section 1.8 (items 1, 2, 4, 5). The expected
//! values come from that text and from the known-answer set of section 1.3
//! (`spec/rqp/examples/kat`), not from another implementation (DL-173).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::manifest::ManifestError;
use rivqen_proto::types::{Limits, Mode, Policy};
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::full::{FullDocument, FullDocumentError, read_full_document};
use rivqen_template::rqp::locate::{LocateError, insertion_point};
use rivqen_template::rqp::scan::scan;
use rivqen_template::rqp::{MarkupCode, split_rqp};

const ORIGIN: &str = "https://shop.example.com";
const PATH: &str = "/catalog";
const START: &str = r#"<script type="application/rivqen-manifest+json">"#;
const END: &str = "</script>";

/// The example page of section 1.1 (the known-answer page).
const PAGE: &str = "<!DOCTYPE html>\n<html>\n<head>\n  <title data-rq-block=\"title\">Shop</title>\n</head>\n<body>\n  <header>Static header</header>\n  <p class=\"price\" data-rq-block=\"price\">120.00</p>\n  <script type=\"application/json\" data-rq-block=\"cart\">{\"items\":3}</script>\n</body>\n</html>\n";

fn read(full: &[u8]) -> Result<FullDocument, FullDocumentError> {
    read_full_document(full, ORIGIN, PATH, &TemplateLimits::default())
}

fn revision(original: &str, policy: Option<&Policy>, limits: Option<&Limits>) -> String {
    split_rqp(original.as_bytes(), &TemplateLimits::default())
        .unwrap()
        .page_revision(policy, limits)
        .to_string()
}

/// A manifest text with `page_revision` and the members in `extra`.
fn manifest(page_revision: &str, extra: &str) -> String {
    format!(
        r#"{{"protocol":"rqp/1","origin":"{ORIGIN}","path":"{PATH}","page_revision":"{page_revision}"{extra}}}"#
    )
}

/// F: the element with the text `text` at `offset` of `original`.
fn insert_at(original: &str, offset: usize, text: &str) -> String {
    format!(
        "{}{START}{text}{END}{}",
        &original[..offset],
        &original[offset..]
    )
}

/// F: the element with the text `text` at the insertion point of `original`.
fn full_of(original: &str, text: &str) -> String {
    let point = insertion_point(original.as_bytes(), &TemplateLimits::default())
        .unwrap()
        .unwrap();
    insert_at(original, point, text)
}

fn valid_full() -> String {
    full_of(PAGE, &manifest(&revision(PAGE, None, None), ""))
}

fn kat(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../../spec/rqp/examples/kat/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn fixture_body(id: &str) -> Vec<u8> {
    let path = format!(
        "{}/../../fixtures/rqp/protocol/{id}/response.body",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

// ---------------------------------------------------------------------------
// Accept path.
// ---------------------------------------------------------------------------

#[test]
fn full_accepts_known_answer_full_document() {
    let full = kat("full.html");
    let page = kat("page.html");
    let document = read(&full).unwrap();
    assert_eq!(document.original(), page.as_slice());
    assert_eq!(
        document.page_revision().to_string(),
        "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8"
    );
    assert_eq!(
        document.document().template_revision.to_string(),
        "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM"
    );
    assert_eq!(
        document.document(),
        &split_rqp(&page, &TemplateLimits::default()).unwrap()
    );
    assert_eq!(document.manifest().origin, ORIGIN);
    assert_eq!(document.manifest().path, PATH);
    assert_eq!(document.manifest().policy, None);
    let (original, analysis, manifest) = document.clone().into_parts();
    assert_eq!(original, page);
    assert_eq!(&analysis, document.document());
    assert_eq!(&manifest, document.manifest());
}

#[test]
fn full_accepts_known_answer_full_document_with_policy() {
    let full = kat("full-policy.html");
    let document = read(&full).unwrap();
    assert_eq!(document.original(), kat("page.html").as_slice());
    assert_eq!(
        document.page_revision().to_string(),
        "r1.pONNMoySlMlYw50BpgdvbUlGESnMzZBCOmQ4SlebDxs"
    );
    let policy = document.manifest().policy.unwrap();
    assert_eq!(policy.ttl_seconds, Some(60));
    assert_eq!(policy.offline, Some(true));
    assert_eq!(
        document.manifest().limits.unwrap().max_patch_bytes,
        Some(65536)
    );
}

#[test]
fn full_accepts_implied_head() {
    let original = "<!DOCTYPE html>\n<html>\n<meta charset=\"utf-8\">\n<title data-rq-block=\"title\">Shop</title>\n<body><p data-rq-block=\"p\">1</p></body>";
    let full = full_of(original, &manifest(&revision(original, None, None), ""));
    assert!(full.contains(&format!("<html>\n{START}")));
    let document = read(full.as_bytes()).unwrap();
    assert_eq!(document.original(), original.as_bytes());
}

#[test]
fn full_accepts_fixture_bodies() {
    for id in [
        "FX-RQ-C21-COMMENT-DOCTYPE-BEFORE-HEAD",
        "FX-RQ-C21-HEAD-NOSCRIPT-IMG",
        "FX-RQ-C21-HEAD-STRAY-BODY",
        "FX-RQ-C21-HEAD-STRAY-VOID-END",
        "FX-RQ-C21-IMPLIED-HEAD-META",
        "FX-RQ-C21-IMPLIED-HEAD-TITLE",
        "FX-RQ-C05-TYPE-UPPERCASE",
        "FX-RQ-C29-MANIFEST-DEFAULT-MEMBERS",
    ] {
        let full = fixture_body(id);
        let document = read(&full).unwrap_or_else(|e| panic!("{id}: {e:?}"));
        assert!(document.original().len() < full.len(), "{id}");
    }
}

// ---------------------------------------------------------------------------
// Order of the checks.
// ---------------------------------------------------------------------------

#[test]
fn full_order_locate_before_manifest() {
    // No manifest element, and the document would also fail every later check.
    let error = read(b"<p data-rq-block=\"x\"><div>").unwrap_err();
    assert_eq!(error, FullDocumentError::Locate(LocateError::NoManifest));
    // A wrong position wins over a manifest text that is not JSON.
    let full = insert_at(PAGE, PAGE.find("<title").unwrap(), "not json");
    assert_eq!(
        read(full.as_bytes()).unwrap_err(),
        FullDocumentError::Locate(LocateError::WrongPosition)
    );
}

/// O with a markup error (a second block with the same id).
const BAD_PAGE: &str = "<html><head><title data-rq-block=\"a\">x</title></head><body><p data-rq-block=\"a\">y</p></body>";

#[test]
fn full_order_manifest_too_large_before_markup() {
    let text = format!("{}{}", manifest("r1.x", ""), " ".repeat(64 * 1024));
    let full = full_of(BAD_PAGE, &text);
    assert_eq!(
        read(full.as_bytes()).unwrap_err(),
        FullDocumentError::Manifest(ManifestError::TooLarge)
    );
}

#[test]
fn full_order_manifest_json_and_schema_before_markup() {
    let full = full_of(BAD_PAGE, "{\"protocol\":\"rqp/1\",");
    assert!(matches!(
        read(full.as_bytes()).unwrap_err(),
        FullDocumentError::Manifest(ManifestError::Json(_))
    ));
    // A duplicate member is a JSON rule of section 3.2.
    let full = full_of(BAD_PAGE, "{\"protocol\":\"rqp/1\",\"protocol\":\"rqp/1\"}");
    assert!(matches!(
        read(full.as_bytes()).unwrap_err(),
        FullDocumentError::Manifest(ManifestError::Json(_))
    ));
    let full = full_of(BAD_PAGE, "{\"protocol\":\"rqp/1\"}");
    assert!(matches!(
        read(full.as_bytes()).unwrap_err(),
        FullDocumentError::Manifest(ManifestError::Schema(_))
    ));
    // The form before H-103: a block list is not in the schema.
    let full = fixture_body("FX-RQ-C29-MANIFEST-OLD-FORM");
    assert!(matches!(
        read(&full).unwrap_err(),
        FullDocumentError::Manifest(ManifestError::Schema(_))
    ));
    let full = fixture_body("FX-RQ-C17-MANIFEST-DUPLICATE-MEMBER");
    assert!(matches!(
        read(&full).unwrap_err(),
        FullDocumentError::Manifest(ManifestError::Json(_))
    ));
}

#[test]
fn full_order_url_mismatch_before_markup() {
    let text = manifest(&revision(PAGE, None, None), "");
    let full = full_of(BAD_PAGE, &text);
    let limits = TemplateLimits::default();
    for (origin, path) in [
        ("https://shop.example.org", PATH),
        ("http://shop.example.com", PATH),
        (ORIGIN, "/catalog/"),
        (ORIGIN, "/Catalog"),
    ] {
        assert_eq!(
            read_full_document(full.as_bytes(), origin, path, &limits).unwrap_err(),
            FullDocumentError::Manifest(ManifestError::UrlMismatch),
            "{origin}{path}"
        );
    }
}

#[test]
fn full_order_markup_before_page_revision() {
    let full = full_of(BAD_PAGE, &manifest(&revision(PAGE, None, None), ""));
    let error = read(full.as_bytes()).unwrap_err();
    assert!(
        matches!(error, FullDocumentError::Markup(e) if e.code() == MarkupCode::Duplicate),
        "{error:?}"
    );
}

#[test]
fn full_order_markup_uses_the_limits() {
    let full = valid_full();
    // Two blocks at most: the page has three.
    let limits = TemplateLimits::new(1 << 20, 2, 1 << 20, 128, 256);
    let error = read_full_document(full.as_bytes(), ORIGIN, PATH, &limits).unwrap_err();
    assert!(matches!(error, FullDocumentError::Markup(e) if e.code() == MarkupCode::Limit));
}

// ---------------------------------------------------------------------------
// Section 1.8, item 5: the page revision.
// ---------------------------------------------------------------------------

#[test]
fn full_page_revision_of_another_page_rejects() {
    let other = PAGE.replace("120.00", "130.00");
    let full = full_of(PAGE, &manifest(&revision(&other, None, None), ""));
    assert_eq!(
        read(full.as_bytes()).unwrap_err(),
        FullDocumentError::PageRevisionDiffers
    );
    let full = fixture_body("FX-RQ-C11-MANIFEST-OTHER-PAGE");
    assert_eq!(
        read(&full).unwrap_err(),
        FullDocumentError::PageRevisionDiffers
    );
}

#[test]
fn full_page_revision_covers_policy_and_limits_members() {
    let none = revision(PAGE, None, None);
    let policy = Policy {
        mode: Some(Mode::Public),
        ttl_seconds: Some(60),
        offline: None,
    };
    let limits = Limits {
        max_patch_bytes: Some(4096),
    };
    let with = revision(PAGE, Some(&policy), Some(&limits));
    let members =
        r#","policy":{"mode":"public","ttl_seconds":60},"limits":{"max_patch_bytes":4096}"#;
    // The revision with the members: accepted.
    let full = full_of(PAGE, &manifest(&with, members));
    let document = read(full.as_bytes()).unwrap();
    assert_eq!(document.manifest().policy, Some(policy));
    assert_eq!(document.page_revision().to_string(), with);
    // New members with the old revision, and the reverse: rejected.
    for text in [
        manifest(&none, members),
        manifest(&none, r#","policy":{}"#),
        manifest(&none, r#","limits":{}"#),
        manifest(&with, ""),
        manifest(
            &with,
            r#","policy":{"mode":"public","ttl_seconds":61},"limits":{"max_patch_bytes":4096}"#,
        ),
    ] {
        let full = full_of(PAGE, &text);
        assert_eq!(
            read(full.as_bytes()).unwrap_err(),
            FullDocumentError::PageRevisionDiffers,
            "{text}"
        );
    }
}

#[test]
fn full_page_revision_member_order_and_white_space_do_not_matter() {
    let text = format!(
        "\n {{ \"page_revision\" : \"{}\" ,\"path\":\"{PATH}\",\t\"origin\":\"{ORIGIN}\",\"protocol\":\"rqp/1\" }} \n",
        revision(PAGE, None, None)
    );
    let full = full_of(PAGE, &text);
    assert!(read(full.as_bytes()).is_ok());
}

// ---------------------------------------------------------------------------
// Item 2 and the manifest text.
// ---------------------------------------------------------------------------

#[test]
fn full_two_manifests_rejects() {
    // The second element is in O; M-13 rejects it there (DL-172).
    let text = manifest(&revision(PAGE, None, None), "");
    let full = full_of(PAGE, &text);
    for second in [
        format!("{START}{text}{END}"),
        format!("<script TYPE=\"Application/Rivqen-Manifest+Json\">{text}{END}"),
        format!("<svg>{START}{text}{END}</svg>"),
    ] {
        let twice = full.replace("</body>", &format!("{second}</body>"));
        let error = read(twice.as_bytes()).unwrap_err();
        assert!(
            matches!(error, FullDocumentError::Markup(e) if e.code() == MarkupCode::Reserved),
            "{second}: {error:?}"
        );
    }
    // Two elements next to each other at the insertion point.
    let twice = full_of(&full, &text);
    assert!(matches!(
        read(twice.as_bytes()).unwrap_err(),
        FullDocumentError::Markup(e) if e.code() == MarkupCode::Reserved
    ));
    let error = read(&fixture_body("FX-RQ-C13-TWO-MANIFESTS")).unwrap_err();
    assert!(matches!(error, FullDocumentError::Markup(e) if e.code() == MarkupCode::Reserved));
}

#[test]
fn full_text_with_lt_rejects() {
    let valid = manifest(&revision(PAGE, None, None), "");
    for text in [
        format!("{valid}<!--<script>"),
        format!("<!--{valid}-->"),
        format!("{valid}<!--<script>x</script>-->"),
        valid.replace("/catalog", "/cat<alog"),
        valid.replace("rqp/1", "rqp/1<"),
        format!("{valid}<"),
    ] {
        let full = full_of(PAGE, &text);
        let result = read(full.as_bytes());
        assert!(
            matches!(
                result,
                Err(FullDocumentError::Manifest(_) | FullDocumentError::Locate(_))
            ),
            "{text}: {result:?}"
        );
    }
    // `<!--<script>` without `-->`: the double-escaped script data never
    // ends, so the element has no end tag.
    let full = full_of(PAGE, &format!("{valid}<!--<script>"));
    assert_eq!(
        read(full.as_bytes()).unwrap_err(),
        FullDocumentError::Locate(LocateError::Unterminated)
    );
}

// ---------------------------------------------------------------------------
// Errors.
// ---------------------------------------------------------------------------

const SECRET: &str = "SECRETqz";

#[test]
fn full_errors_are_manifest_invalid() {
    let errors = [
        FullDocumentError::Locate(LocateError::NoManifest),
        FullDocumentError::Locate(LocateError::Unterminated),
        FullDocumentError::Locate(LocateError::NoHead),
        FullDocumentError::Locate(LocateError::WrongPosition),
        FullDocumentError::Manifest(ManifestError::TooLarge),
        FullDocumentError::Manifest(ManifestError::UrlMismatch),
        FullDocumentError::Markup(rivqen_template::rqp::MarkupError::new(
            MarkupCode::Reserved,
            "M-13",
        )),
        FullDocumentError::PageRevisionDiffers,
    ];
    for error in errors {
        assert_eq!(error.code(), ErrorCode::ManifestInvalid, "{error:?}");
        assert_eq!(error.to_string(), error.reason());
    }
}

#[test]
fn full_errors_hold_no_document_bytes() {
    let good = revision(PAGE, None, None);
    let secret_page = PAGE.replace("Static header", SECRET);
    let cases = [
        // Locate.
        format!("<p>{SECRET}</p>"),
        format!("<html>{SECRET}{START}{{}}{END}"),
        // Manifest: JSON, schema, URL.
        full_of(PAGE, &format!("{{\"{SECRET}\"")),
        full_of(PAGE, &format!("{{\"{SECRET}\":1}}")),
        full_of(
            PAGE,
            &manifest(&good, "").replace(
                ORIGIN,
                &format!("https://{}.example", SECRET.to_lowercase()),
            ),
        ),
        // Markup.
        full_of(
            &format!("<html><head></head><body><p data-rq-block=\"{SECRET}\">x</p>"),
            &manifest(&good, ""),
        ),
        // Page revision.
        full_of(&secret_page, &manifest(&good, "")),
    ];
    for full in cases {
        let error = read(full.as_bytes()).unwrap_err();
        let text = format!("{error:?} {error}");
        assert!(!text.contains(SECRET), "{text}");
        assert!(!text.contains(&SECRET.to_lowercase()), "{text}");
        assert!(!text.contains('<'), "{text}");
    }
}

// ---------------------------------------------------------------------------
// Property: the element at the insertion point, and only there.
// ---------------------------------------------------------------------------

fn prefix() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(
            &[
                "<!DOCTYPE html>",
                "<!-- c -->",
                "<?pi?>",
                "<html>",
                "<html lang=\"en\">",
                "</p>",
                "</div>",
                " ",
                "\n",
                "\r\n",
                "\t",
            ][..],
        ),
        0..6,
    )
    .prop_map(|pieces| pieces.concat())
}

/// The head start tag or the first tag of an implied head, and more head
/// content.
fn head() -> impl Strategy<Value = String> {
    prop::sample::select(
        &[
            "<head>",
            "<HEAD lang=\"en\">",
            "<head/>",
            "<meta charset=\"utf-8\">",
            "<link rel=\"icon\" href=\"/i.png\">",
            "<base href=\"/\">",
            "<title data-rq-block=\"title\">Shop</title>",
            "<style>p{}</style>",
            "<script>var a = 1;</script>",
            "<noframes>x</noframes>",
        ][..],
    )
    .prop_map(str::to_owned)
}

fn rest() -> impl Strategy<Value = String> {
    (
        prop::sample::select(&["", "\n", "<!-- h -->", "<meta name=\"a\">", "<title>T</title>"][..]),
        prop::sample::select(&["", "</head>", "</head>\n"][..]),
        "[a-z0-9 .]{0,8}",
        prop::bool::ANY,
    )
        .prop_map(|(head_content, head_end, price, json)| {
            let cart = if json {
                "<script type=\"application/json\" data-rq-block=\"cart\">{\"n\":1}</script>"
            } else {
                ""
            };
            format!(
                "{head_content}{head_end}<body>\n<p data-rq-block=\"price\">{price}</p>{cart}\n</body>\n</html>"
            )
        })
}

fn policy_member() -> impl Strategy<Value = (Option<Policy>, Option<Limits>, String)> {
    prop::sample::select(&[0_u8, 1, 2, 3][..]).prop_map(|choice| match choice {
        0 => (None, None, String::new()),
        1 => (
            Some(Policy {
                mode: None,
                ttl_seconds: Some(60),
                offline: None,
            }),
            None,
            r#","policy":{"ttl_seconds":60}"#.to_owned(),
        ),
        2 => (
            None,
            Some(Limits {
                max_patch_bytes: Some(2048),
            }),
            r#","limits":{"max_patch_bytes":2048}"#.to_owned(),
        ),
        _ => (Some(Policy::default()), None, r#","policy":{}"#.to_owned()),
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn full_prop_insert_at_point(
        prefix in prefix(),
        head in head(),
        rest in rest(),
        (policy, limits, members) in policy_member(),
    ) {
        let original = format!("{prefix}{head}{rest}");
        let template_limits = TemplateLimits::default();
        prop_assume!(split_rqp(original.as_bytes(), &template_limits).is_ok());
        let point = insertion_point(original.as_bytes(), &template_limits).unwrap();
        let point = point.expect("a head or an implied head");
        let text = manifest(&revision(&original, policy.as_ref(), limits.as_ref()), &members);

        let full = insert_at(&original, point, &text);
        let document = read(full.as_bytes()).unwrap();
        prop_assert_eq!(document.original(), original.as_bytes());
        prop_assert_eq!(document.manifest().policy, policy);
        prop_assert_eq!(document.manifest().limits, limits);

        // Every other token boundary of O rejects.
        let mut boundaries: Vec<usize> = scan(original.as_bytes(), &template_limits)
            .map(|token| token.unwrap().span().start)
            .collect();
        boundaries.push(original.len());
        for boundary in boundaries.into_iter().filter(|&b| b != point) {
            let full = insert_at(&original, boundary, &text);
            let result = read(full.as_bytes());
            prop_assert!(result.is_err(), "boundary {}", boundary);
            prop_assert_eq!(result.unwrap_err().code(), ErrorCode::ManifestInvalid);
        }
    }
}

#[test]
fn full_debug_of_an_accepted_document_holds_no_document_bytes() {
    let page = PAGE.replace("Static header", SECRET);
    let full = full_of(&page, &manifest(&revision(&page, None, None), ""));
    let document = read(full.as_bytes()).unwrap();
    let text = format!("{document:?} {document:#?}");
    assert!(!text.contains(SECRET), "{text}");
    assert!(!text.contains("shop.example.com"), "{text}");
    assert!(!text.contains('<'), "{text}");
    assert!(
        text.contains(&format!("original_len: {}", page.len())),
        "{text}"
    );
    assert!(text.contains("blocks: 3"), "{text}");
}
