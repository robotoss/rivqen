// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Tests of the size limit of the full document F (WP-08 S1 T-07, H-158).
//!
//! The client reads F up to M-01 plus the largest manifest element. The
//! original document O keeps M-01. The inputs are built in memory.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_template::limits::{
    MANIFEST_END_TAG_BYTES, MANIFEST_START_TAG_BYTES, MAX_MANIFEST_ELEMENT_BYTES,
    MAX_MARKUP_INPUT_BYTES, TemplateLimits,
};
use rivqen_template::rqp::MarkupCode;
use rivqen_template::rqp::full::{FullDocumentError, read_full_document};
use rivqen_template::rqp::locate::{LocateError, insertion_point, locate_manifest};
use rivqen_template::rqp::split_rqp;

const ORIGIN: &str = "https://shop.example.com";
const PATH: &str = "/catalog";
const START: &str = r#"<script type="application/rivqen-manifest+json">"#;
const END: &str = "</script>";
const HEAD: &str = "<!DOCTYPE html>\n<html>\n<head>\n<title>Shop</title>\n</head>\n<body>\n";
const TAIL: &str = "\n<p data-rq-block=\"price\">1</p>\n</body>\n</html>\n";
const M01: usize = MAX_MARKUP_INPUT_BYTES;

/// A page of exactly `len` bytes: padding in an HTML comment.
fn page(len: usize) -> Vec<u8> {
    let fixed = HEAD.len() + "<!---->".len() + TAIL.len();
    let pad = len.checked_sub(fixed).unwrap();
    let mut out = String::with_capacity(len);
    out.push_str(HEAD);
    out.push_str("<!--");
    out.push_str(&"x".repeat(pad));
    out.push_str("-->");
    out.push_str(TAIL);
    assert_eq!(out.len(), len);
    out.into_bytes()
}

fn manifest(original: &[u8], limits: &TemplateLimits) -> String {
    let revision = split_rqp(original, limits)
        .unwrap()
        .page_revision(None, None)
        .to_string();
    format!(
        r#"{{"protocol":"rqp/1","origin":"{ORIGIN}","path":"{PATH}","page_revision":"{revision}"}}"#
    )
}

/// F with the element `start` + `text` + end tag at the insertion point.
fn full_with(original: &[u8], start: &str, text: &[u8], limits: &TemplateLimits) -> Vec<u8> {
    // Every page starts with HEAD, so the point is found on that prefix
    // (the page itself can be over M-01).
    let point = insertion_point(&original[..HEAD.len()], limits)
        .unwrap()
        .unwrap();
    let mut out = original[..point].to_vec();
    out.extend_from_slice(start.as_bytes());
    out.extend_from_slice(text);
    out.extend_from_slice(END.as_bytes());
    out.extend_from_slice(&original[point..]);
    out
}

/// F with a valid manifest element at the insertion point of `original`.
fn full_of(original: &[u8], limits: &TemplateLimits) -> Vec<u8> {
    let text = manifest(original, limits);
    full_with(original, START, text.as_bytes(), limits)
}

fn read(full: &[u8], limits: &TemplateLimits) -> Result<(), FullDocumentError> {
    read_full_document(full, ORIGIN, PATH, limits).map(|_| ())
}

fn assert_m01(result: Result<(), FullDocumentError>) {
    match result {
        Err(error @ FullDocumentError::Locate(LocateError::Markup(markup))) => {
            assert_eq!(markup.code(), MarkupCode::Limit);
            assert_eq!(markup.rule(), "M-01");
            assert_eq!(error.code(), ErrorCode::ManifestInvalid);
        }
        other => panic!("expected the M-01 error, got {other:?}"),
    }
}

#[test]
fn rqp_full_size_o_at_limit() {
    let limits = TemplateLimits::default();
    let original = page(M01);
    let full = full_of(&original, &limits);
    assert!(full.len() > M01);
    let document = read_full_document(&full, ORIGIN, PATH, &limits).unwrap();
    assert_eq!(document.original(), original.as_slice());
}

#[test]
fn rqp_full_size_o_over_limit() {
    let limits = TemplateLimits::default();
    // The revision of O over M-01 cannot be computed, so the manifest holds
    // the revision of a small page; the size check comes first.
    let text = manifest(&page(1000), &limits);
    let full = full_with(&page(M01 + 1), START, text.as_bytes(), &limits);
    assert!(full.len() <= limits.max_full_bytes());
    assert_m01(read(&full, &limits));
}

#[test]
fn rqp_full_size_over_f_limit() {
    let limits = TemplateLimits::default();
    let mut full = full_of(&page(M01), &limits);
    full.resize(limits.max_full_bytes() + 1, b' ');
    assert_m01(read(&full, &limits));
    match locate_manifest(&full, &limits) {
        Err(LocateError::Markup(markup)) => assert_eq!(markup.rule(), "M-01"),
        other => panic!("expected the M-01 error, got {other:?}"),
    }
}

#[test]
fn rqp_full_size_f_at_f_limit_is_read() {
    // O at M-01 and a manifest element of the largest server form: F is
    // exactly at the F limit and is read.
    let limits = TemplateLimits::default();
    let original = page(M01);
    let text = manifest(&original, &limits);
    // Spaces after the JSON object keep the manifest valid.
    let pad = 65_536 - text.len();
    let text = format!("{text}{}", " ".repeat(pad));
    let full = full_with(&original, START, text.as_bytes(), &limits);
    assert_eq!(full.len(), limits.max_full_bytes());
    assert!(read(&full, &limits).is_ok());
}

#[test]
fn rqp_full_size_plain_page() {
    let limits = TemplateLimits::default();
    // A plain page over M-01 without the manifest type is not analysed.
    assert_m01(read(&page(M01 + 1), &limits));
    assert_m01(read(&page(limits.max_full_bytes() + 1000), &limits));
    // A plain page at M-01 has no manifest.
    assert!(matches!(
        locate_manifest(&page(M01), &limits),
        Err(LocateError::NoManifest)
    ));
}

#[test]
fn rqp_full_size_type_text_only_in_comment() {
    let limits = TemplateLimits::default();
    // Over M-01, the type text is only in a comment: the search chooses the
    // F limit, the scan finds no manifest element and the read fails.
    let mut original = page(M01 - 100);
    original.extend_from_slice(b"<!-- application/rivqen-manifest+json -->");
    original.resize(M01 + 10, b' ');
    assert!(original.len() <= limits.max_full_bytes());
    assert!(matches!(
        locate_manifest(&original, &limits),
        Err(LocateError::NoManifest)
    ));
    assert!(read(&original, &limits).is_err());
    // Over the F limit, the same text gives the M-01 error.
    let mut huge = original.clone();
    huge.resize(limits.max_full_bytes() + 1, b' ');
    assert_m01(read(&huge, &limits));
}

#[test]
fn rqp_full_size_type_text_in_attribute_is_not_a_manifest() {
    let limits = TemplateLimits::default();
    let mut original = page(M01 - 200);
    original.extend_from_slice(b"<p title=\"application/rivqen-manifest+json\">x</p>");
    original.resize(M01 + 10, b' ');
    assert!(matches!(
        locate_manifest(&original, &limits),
        Err(LocateError::NoManifest)
    ));
}

#[test]
fn rqp_full_size_constant() {
    assert_eq!(START.len(), MANIFEST_START_TAG_BYTES);
    assert_eq!(END.len(), MANIFEST_END_TAG_BYTES);
    assert_eq!(MAX_MANIFEST_ELEMENT_BYTES, START.len() + 65_536 + END.len());
    let limits = TemplateLimits::default();
    assert_eq!(limits.max_original_bytes(), M01);
    assert_eq!(limits.max_full_bytes(), M01 + MAX_MANIFEST_ELEMENT_BYTES);
}

#[test]
fn rqp_full_size_long_start_tag_fails_closed() {
    // H-158, strictest reading: a start tag longer than the server form,
    // a manifest text at 64 KiB and O at M-01 give F over the F limit.
    let limits = TemplateLimits::default();
    let original = page(M01);
    let start = r#"<script  type="application/rivqen-manifest+json">"#;
    let text = vec![b' '; 65_536];
    let full = full_with(&original, start, &text, &limits);
    assert!(full.len() > limits.max_full_bytes());
    assert_m01(read(&full, &limits));
}

#[test]
fn rqp_full_size_one_mib_limit() {
    let m = 1024 * 1024;
    let limits = TemplateLimits::new(m, 4096, 1 << 20, 128, 256);
    assert_eq!(limits.max_original_bytes(), m);
    assert_eq!(limits.max_full_bytes(), m + MAX_MANIFEST_ELEMENT_BYTES);
    // O at 1 MiB is read.
    let original = page(m);
    assert!(read(&full_of(&original, &limits), &limits).is_ok());
    // O one byte over is rejected.
    let text = manifest(&page(1000), &limits);
    let full = full_with(&page(m + 1), START, text.as_bytes(), &limits);
    assert_m01(read(&full, &limits));
    // F over the lowered F limit is rejected.
    let mut over = full_of(&original, &limits);
    over.resize(limits.max_full_bytes() + 1, b' ');
    assert_m01(read(&over, &limits));
    // A plain page over the lowered limit is rejected.
    assert_m01(read(&page(m + 1), &limits));
}

/// Many copies of the type text in a comment, then padding up to `len`.
fn with_type_text(len: usize, copies: usize) -> Vec<u8> {
    let mut out = page(M01 - 1000);
    out.extend_from_slice(b"<!--");
    for _ in 0..copies {
        out.extend_from_slice(b" application/rivqen-manifest+json");
    }
    out.extend_from_slice(b" -->");
    assert!(out.len() <= len);
    out.resize(len, b' ');
    out
}

#[test]
fn rqp_full_size_over_f_limit_with_many_type_texts() {
    let limits = TemplateLimits::default();
    let huge = with_type_text(limits.max_full_bytes() + 1, 1000);
    assert_m01(read(&huge, &limits));
    assert!(matches!(
        locate_manifest(&huge, &limits),
        Err(LocateError::Markup(m)) if m.rule() == "M-01"
    ));
}

#[test]
fn rqp_full_size_over_f_limit_without_type_text() {
    let limits = TemplateLimits::default();
    let mut huge = page(M01);
    huge.resize(limits.max_full_bytes() + 1, b' ');
    assert_m01(read(&huge, &limits));
}

mod around_limits {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        /// The error class of an input depends only on its length relative
        /// to O and F, and on the type text between the two.
        #[test]
        fn rqp_full_size_class_by_length(
            around_f in any::<bool>(),
            delta in -3_isize..=3,
            typed in any::<bool>(),
            copies in 1_usize..20,
        ) {
            let limits = TemplateLimits::default();
            let base = if around_f { limits.max_full_bytes() } else { limits.max_original_bytes() };
            let len = base.checked_add_signed(delta).unwrap();
            let input = if typed {
                with_type_text(len, copies)
            } else {
                let mut v = page(M01);
                v.resize(len, b' ');
                v
            };
            let result = locate_manifest(&input, &limits);
            let over_f = len > limits.max_full_bytes();
            let over_o = len > limits.max_original_bytes();
            if over_f || (over_o && !typed) {
                prop_assert!(matches!(
                    result,
                    Err(LocateError::Markup(m)) if m.rule() == "M-01"
                ));
            } else {
                prop_assert!(matches!(result, Err(LocateError::NoManifest)));
            }
        }
    }
}
