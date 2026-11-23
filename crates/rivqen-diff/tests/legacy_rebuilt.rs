// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `Rebuilt`: the checked rebuild of a legacy data update (CL-27, CL-29;
//! DL-169 S1-5), and the errors of `diff_v1`.

#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use core::error::Error as _;

use rivqen_diff::legacy::{DiffError, RebuildError, RebuildInput, Rebuilt, diff_v1};
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_template::legacy::{
    DataBlock, LimitError, RenderError, SplitOutcome, etag_v1, render_v1, split_v1,
};
use rivqen_template::limits::{MAX_LEGACY_NAME_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits};
use rivqen_testkit::fixtures::{Fixture, load_legacy, load_malformed};

/// A text that must never appear in an error or in a `Debug` output.
const MARKER: &str = "SECRET-MARKER-7f3a";

/// A template tag that must never appear in an error or in a `Debug` output.
const TAG: &str = "TAG-MARKER-91c2";

const PAGE: &str = "<html><head><title>Shop SECRET-MARKER-7f3a</title></head><body>\
<!--sonicdiff-price-->10 EUR<!--sonicdiff-price-end-->\
<!--sonicdiff-stock-->in stock<!--sonicdiff-stock-end--></body></html>";

fn block(key: &str, value: &str) -> DataBlock {
    DataBlock {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

/// The stored entry of `PAGE`: its template and data.
fn stored() -> (String, Vec<DataBlock>) {
    match split_v1(PAGE.as_bytes(), &TemplateLimits::default()).unwrap() {
        SplitOutcome::Applied(split) => (split.template, split.data),
        SplitOutcome::NotApplied(reason) => panic!("{reason:?}"),
    }
}

/// The server data of an update: the price changes.
fn update(data: &[DataBlock]) -> Vec<DataBlock> {
    data.iter()
        .map(|b| match b.key.as_str() {
            "{price}" => block(
                "{price}",
                "<!--sonicdiff-price-->12 EUR SECRET-MARKER-7f3a<!--sonicdiff-price-end-->",
            ),
            _ => b.clone(),
        })
        .collect()
}

/// The page of the server for `server`, and its SHA-1.
fn server_page(template: &str, server: &[DataBlock]) -> (String, String) {
    let page = render_v1(template, server, &TemplateLimits::default()).unwrap();
    let sha1 = etag_v1(page.as_bytes()).to_string();
    (page, sha1)
}

fn rebuild(
    template: &str,
    stored: &[DataBlock],
    server: Vec<DataBlock>,
    html_sha1: &str,
    etag: &str,
) -> Result<Rebuilt, RebuildError> {
    Rebuilt::new(
        RebuildInput {
            template,
            stored,
            server,
            html_sha1,
            etag,
            response_template_tag: TAG,
            stored_template_tag: TAG,
        },
        &TemplateLimits::default(),
    )
}

#[test]
fn legacy_rebuilt_data_update_gives_page_data_and_diff() {
    let (template, data) = stored();
    let server = update(&data);
    let (page, sha1) = server_page(&template, &server);
    let rebuilt = rebuild(&template, &data, server.clone(), &sha1, &sha1).unwrap();
    assert_eq!(rebuilt.page(), page);
    assert_ne!(rebuilt.page(), PAGE);
    assert_eq!(rebuilt.etag().as_str(), sha1);
    assert_eq!(rebuilt.data(), server.as_slice());
    let diff_keys: Vec<&str> = rebuilt.diff().iter().map(|b| b.key.as_str()).collect();
    assert_eq!(diff_keys, ["{price}"]);
    assert_eq!(rebuilt.diff()[0], server[1]);
}

#[test]
fn legacy_rebuilt_same_data_gives_the_page_and_an_empty_diff() {
    let (template, data) = stored();
    let sha1 = etag_v1(PAGE.as_bytes()).to_string();
    let rebuilt = rebuild(&template, &data, data.clone(), &sha1, &sha1).unwrap();
    assert_eq!(rebuilt.page(), PAGE);
    assert_eq!(rebuilt.diff(), &[]);
}

#[test]
fn legacy_rebuilt_key_missing_in_stored_data_is_in_the_diff() {
    let (template, data) = stored();
    let sha1 = etag_v1(PAGE.as_bytes()).to_string();
    let partial: Vec<DataBlock> = data
        .iter()
        .filter(|b| b.key != "{stock}")
        .cloned()
        .collect();
    let rebuilt = rebuild(&template, &partial, data.clone(), &sha1, &sha1).unwrap();
    let diff_keys: Vec<&str> = rebuilt.diff().iter().map(|b| b.key.as_str()).collect();
    assert_eq!(diff_keys, ["{stock}"]);
}

#[test]
fn legacy_rebuilt_into_parts_gives_the_checked_values() {
    let (template, data) = stored();
    let server = update(&data);
    let (page, sha1) = server_page(&template, &server);
    let rebuilt = rebuild(&template, &data, server.clone(), &sha1, &sha1).unwrap();
    let diff = rebuilt.diff().to_vec();
    let parts = rebuilt.into_parts();
    assert_eq!(parts.page, page);
    assert_eq!(parts.etag.as_str(), sha1);
    assert_eq!(parts.data, server);
    assert_eq!(parts.diff, diff);
}

#[test]
fn legacy_rebuilt_equal_template_tags_pass() {
    let (template, data) = stored();
    let sha1 = etag_v1(PAGE.as_bytes()).to_string();
    assert!(rebuild(&template, &data, data.clone(), &sha1, &sha1).is_ok());
}

fn rebuild_with_tags(
    template: &str,
    stored: &[DataBlock],
    server: Vec<DataBlock>,
    hash: &str,
    response_tag: &str,
    stored_tag: &str,
) -> Result<Rebuilt, RebuildError> {
    Rebuilt::new(
        RebuildInput {
            template,
            stored,
            server,
            html_sha1: hash,
            etag: hash,
            response_template_tag: response_tag,
            stored_template_tag: stored_tag,
        },
        &TemplateLimits::default(),
    )
}

#[test]
fn legacy_rebuilt_different_template_tags_give_no_page() {
    let (template, data) = stored();
    let sha1 = etag_v1(PAGE.as_bytes()).to_string();
    let upper = TAG.to_ascii_uppercase();
    let padded = format!("{TAG} ");
    for (response, stored_tag) in [
        ("other-tag", TAG),
        (TAG, "other-tag"),
        (upper.as_str(), TAG),
        (padded.as_str(), TAG),
        ("", TAG),
        (TAG, ""),
        ("", ""),
    ] {
        let error = rebuild_with_tags(&template, &data, data.clone(), &sha1, response, stored_tag)
            .unwrap_err();
        assert_eq!(
            error,
            RebuildError::TemplateTagMismatch,
            "{response:?} {stored_tag:?}"
        );
        assert_eq!(error.code(), ErrorCode::IntegrityError);
        assert!(error.source().is_none());
    }
}

#[test]
fn legacy_rebuilt_checks_template_tags_before_the_render() {
    let (_, data) = stored();
    // A template that cannot render with this data, and wrong hashes.
    let error =
        rebuild_with_tags("<p>no block</p>", &data, data.clone(), "", "a", "b").unwrap_err();
    assert_eq!(error, RebuildError::TemplateTagMismatch);
    // With equal tags the same input fails at the render.
    let error =
        rebuild_with_tags("<p>no block</p>", &data, data.clone(), "", "a", "a").unwrap_err();
    assert!(matches!(error, RebuildError::Render(_)), "{error:?}");
}

#[test]
fn legacy_rebuilt_template_tag_error_text_has_no_tag() {
    let text = RebuildError::TemplateTagMismatch.to_string();
    assert!(!text.contains(TAG));
    assert_eq!(
        text,
        "the legacy response template-tag is not the stored template tag"
    );
}

#[test]
fn legacy_rebuilt_html_sha1_mismatch_gives_no_page() {
    let (template, data) = stored();
    let server = update(&data);
    let (_, sha1) = server_page(&template, &server);
    // The server says the old page: the stored page hash.
    let old = etag_v1(PAGE.as_bytes()).to_string();
    let error = rebuild(&template, &data, server, &old, &sha1).unwrap_err();
    assert_eq!(error, RebuildError::HtmlSha1Mismatch);
    assert_eq!(error.code(), ErrorCode::IntegrityError);
    assert!(error.source().is_none());
}

#[test]
fn legacy_rebuilt_etag_mismatch_gives_no_page() {
    let (template, data) = stored();
    let server = update(&data);
    let (_, sha1) = server_page(&template, &server);
    let old = etag_v1(PAGE.as_bytes()).to_string();
    let error = rebuild(&template, &data, server, &sha1, &old).unwrap_err();
    assert_eq!(error, RebuildError::EtagMismatch);
    assert_eq!(error.code(), ErrorCode::IntegrityError);
    assert!(error.source().is_none());
}

#[test]
fn legacy_rebuilt_checks_html_sha1_before_the_etag() {
    let (template, data) = stored();
    let error = rebuild(&template, &data, data.clone(), "", "").unwrap_err();
    assert_eq!(error, RebuildError::HtmlSha1Mismatch);
}

#[test]
fn legacy_rebuilt_hash_comparison_is_exact() {
    let (template, data) = stored();
    let sha1 = etag_v1(PAGE.as_bytes()).to_string();
    let upper = sha1.to_ascii_uppercase();
    let quoted = format!("\"{sha1}\"");
    let weak = format!("W/{sha1}");
    let padded = format!(" {sha1}");
    for other in [
        upper.as_str(),
        quoted.as_str(),
        weak.as_str(),
        padded.as_str(),
        "",
    ] {
        assert_eq!(
            rebuild(&template, &data, data.clone(), other, &sha1).unwrap_err(),
            RebuildError::HtmlSha1Mismatch,
            "html-sha1 {other:?}"
        );
        assert_eq!(
            rebuild(&template, &data, data.clone(), &sha1, other).unwrap_err(),
            RebuildError::EtagMismatch,
            "etag {other:?}"
        );
    }
}

#[test]
fn legacy_rebuilt_data_that_does_not_fit_the_template_gives_no_page() {
    let (template, data) = stored();
    let sha1 = etag_v1(PAGE.as_bytes()).to_string();
    // A key without a placeholder; a missing `{title}`; a repeated key.
    let mut extra = data.clone();
    extra.push(block("{other}", "x"));
    let no_title: Vec<DataBlock> = data[1..].to_vec();
    let mut repeated = data.clone();
    repeated.push(data[1].clone());
    for server in [extra, no_title, repeated] {
        let error = rebuild(&template, &data, server, &sha1, &sha1).unwrap_err();
        assert_eq!(error, RebuildError::Render(RenderError::Mismatch));
        assert_eq!(error.code(), ErrorCode::IntegrityError);
        assert!(error.source().is_some());
    }
}

#[test]
fn legacy_rebuilt_over_a_limit_gives_no_page() {
    let (template, data) = stored();
    let sha1 = etag_v1(PAGE.as_bytes()).to_string();
    let cases = [
        // The rebuilt page is one byte over the limit.
        (
            TemplateLimits::new(PAGE.len() - 1, 256, 1 << 20, 128, MAX_OPEN_ELEMENTS),
            LimitError::DocumentTooLarge,
        ),
        (
            TemplateLimits::new(1 << 20, 1, 1 << 20, 128, MAX_OPEN_ELEMENTS),
            LimitError::TooManyBlocks,
        ),
        (
            TemplateLimits::new(1 << 20, 256, 10, MAX_LEGACY_NAME_BYTES, MAX_OPEN_ELEMENTS),
            LimitError::BlockTooLarge,
        ),
    ];
    for (limits, limit) in cases {
        let input = RebuildInput {
            template: &template,
            stored: &data,
            server: data.clone(),
            html_sha1: &sha1,
            etag: &sha1,
            response_template_tag: TAG,
            stored_template_tag: TAG,
        };
        let error = Rebuilt::new(input, &limits).unwrap_err();
        assert_eq!(error, RebuildError::Render(RenderError::Limit(limit)));
        assert_eq!(error.code(), ErrorCode::DocumentTooLarge);
    }
    // At the limit, the page is rebuilt.
    let limits = TemplateLimits::new(PAGE.len(), 2, 1 << 20, 128, MAX_OPEN_ELEMENTS);
    let input = RebuildInput {
        template: &template,
        stored: &data,
        server: data.clone(),
        html_sha1: &sha1,
        etag: &sha1,
        response_template_tag: TAG,
        stored_template_tag: TAG,
    };
    assert_eq!(Rebuilt::new(input, &limits).unwrap().page(), PAGE);
}

#[test]
fn legacy_rebuilt_repeated_stored_key_gives_no_page() {
    let (template, data) = stored();
    let sha1 = etag_v1(PAGE.as_bytes()).to_string();
    let mut damaged = data.clone();
    damaged.push(data[1].clone());
    let error = rebuild(&template, &damaged, data.clone(), &sha1, &sha1).unwrap_err();
    assert_eq!(error, RebuildError::Diff(DiffError::DuplicateLocalKey));
    assert_eq!(error.code(), ErrorCode::CacheCorrupted);
    assert!(error.source().is_some());
}

#[test]
fn legacy_diff_errors_have_codes() {
    let data = [block("{a}", "x"), block("{a}", "y")];
    let one = [block("{a}", "x")];
    let local = diff_v1(Some(&data), &one).unwrap_err();
    assert_eq!(local, DiffError::DuplicateLocalKey);
    assert_eq!(local.code(), ErrorCode::CacheCorrupted);
    let server = diff_v1(Some(&one), &data).unwrap_err();
    assert_eq!(server, DiffError::DuplicateServerKey);
    assert_eq!(server.code(), ErrorCode::IntegrityError);
    // Without stored data the server list is still checked.
    assert_eq!(
        diff_v1(None, &data).unwrap_err(),
        DiffError::DuplicateServerKey
    );
    // Keys are case-sensitive: `{A}` and `{a}` are two keys.
    let cased = [block("{a}", "x"), block("{A}", "x")];
    assert_eq!(diff_v1(Some(&one), &cased).unwrap(), [&cased[1]]);
}

#[test]
fn legacy_rebuilt_errors_have_no_content() {
    let errors = [
        RebuildError::Render(RenderError::Mismatch),
        RebuildError::Render(RenderError::Limit(LimitError::DocumentTooLarge)),
        RebuildError::TemplateTagMismatch,
        RebuildError::HtmlSha1Mismatch,
        RebuildError::EtagMismatch,
        RebuildError::Diff(DiffError::DuplicateLocalKey),
        RebuildError::Diff(DiffError::DuplicateServerKey),
    ];
    let mut texts = Vec::new();
    for error in errors {
        let text = error.to_string();
        assert!(!text.contains(MARKER));
        texts.push(text);
    }
    texts.sort();
    texts.dedup();
    assert_eq!(texts.len(), errors.len(), "each error has its own text");
    assert_eq!(
        RebuildError::Diff(DiffError::DuplicateServerKey).to_string(),
        "the legacy diff failed: a key repeats in the legacy server data"
    );
    assert_eq!(
        RebuildError::Render(RenderError::Mismatch).to_string(),
        "the legacy rebuild failed: the legacy data does not fit the template"
    );
}

#[test]
fn legacy_rebuilt_debug() {
    let (template, data) = stored();
    let server = update(&data);
    let (page, sha1) = server_page(&template, &server);
    let input = RebuildInput {
        template: &template,
        stored: &data,
        server: server.clone(),
        html_sha1: &sha1,
        etag: &sha1,
        response_template_tag: TAG,
        stored_template_tag: TAG,
    };
    let input_debug = format!("{input:?}");
    let rebuilt = Rebuilt::new(input, &TemplateLimits::default()).unwrap();
    let rebuilt_debug = format!("{rebuilt:?}");
    let parts_debug = format!("{:?}", rebuilt.clone().into_parts());
    for text in [&input_debug, &rebuilt_debug, &parts_debug] {
        assert!(!text.contains(MARKER), "{text}");
        assert!(!text.contains(&sha1), "{text}");
        assert!(!text.contains("EUR"), "{text}");
        assert!(!text.contains(TAG), "{text}");
    }
    assert_eq!(
        rebuilt_debug,
        format!(
            "Rebuilt {{ page_bytes: {}, data_entries: 3, diff_entries: 1, .. }}",
            page.len()
        )
    );
    assert_eq!(
        parts_debug,
        format!(
            "RebuiltParts {{ page_bytes: {}, data_entries: 3, diff_entries: 1, .. }}",
            page.len()
        )
    );
    assert_eq!(
        input_debug,
        format!(
            "RebuildInput {{ template_bytes: {}, stored_entries: 3, server_entries: 3, \
             html_sha1_bytes: 40, etag_bytes: 40, \
             response_template_tag_bytes: {}, stored_template_tag_bytes: {} }}",
            template.len(),
            TAG.len(),
            TAG.len()
        )
    );
}

/// One fixture page: its split as the stored entry and as the server data,
/// with `html-sha1` and the etag of the page. `Rebuilt` must give the page
/// exactly when the fixture says that the rebuild has the page hash
/// (`split.rebuild_sha1`), else an error (P-DIFF-5 through `Rebuilt`).
fn check_page(f: &Fixture, ok: &mut Vec<String>, refused: &mut Vec<String>) {
    // A fixture whose server does not apply the split (scope rule C-01 or a
    // body rule) has no rebuild.
    if f.expected["split"]["applied"].as_bool() != Some(true) {
        return;
    }
    let html = f.page.as_deref().unwrap();
    let limits = TemplateLimits::default();
    let SplitOutcome::Applied(split) = split_v1(html, &limits).unwrap() else {
        panic!("{}: the split does not apply", f.id);
    };
    let etag = etag_v1(html).to_string();
    let same = f.expected["split"]["rebuild_sha1"].as_str() == Some(etag.as_str());
    let input = RebuildInput {
        template: &split.template,
        stored: &split.data,
        server: split.data.clone(),
        html_sha1: &etag,
        etag: &etag,
        response_template_tag: TAG,
        stored_template_tag: TAG,
    };
    match Rebuilt::new(input, &limits) {
        Ok(rebuilt) => {
            assert!(same, "{}: rebuilt, but rebuild_sha1 differs", f.id);
            assert_eq!(rebuilt.page().as_bytes(), html, "{}", f.id);
            assert!(rebuilt.diff().is_empty(), "{}", f.id);
            ok.push(f.id.clone());
        }
        Err(error) => {
            assert!(!same, "{}: {error:?}, but rebuild_sha1 is the etag", f.id);
            refused.push(f.id.clone());
        }
    }
}

#[test]
fn legacy_rebuilt_fixture_pages() {
    let (mut ok, mut refused) = (Vec::new(), Vec::new());
    for f in load_legacy().unwrap() {
        if !f.id.contains("--") && f.expected.get("split").is_some() {
            check_page(&f, &mut ok, &mut refused);
        }
    }
    for f in load_malformed().unwrap().cases {
        if f.expected.get("split").is_some() {
            check_page(&f, &mut ok, &mut refused);
        }
    }
    // 50 page fixtures and 17 malformed fixtures apply the split (the counts
    // of the template fixture tests). The one page whose rebuild differs
    // (the key text before its block, DL-166) is refused, never given as a
    // wrong page.
    assert_eq!(ok.len(), 66, "{ok:?}");
    assert_eq!(refused, ["FX-LEG-MAL-PLACEHOLDER-TEXT"]);
}
