// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Golden test: every markup fixture of `fixtures/rqp/markup` (DL-161).
//! `split_rqp` with the default limits gives `valid`, `error`, `blocks` (id,
//! format, start, end, sha256), `template_revision` and `page_revision` (no
//! policy line) of `expected.json`. The expected values come from
//! `tools/rqp-markup/gen.py`, not from a parser.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rivqen_integrity::{PageRevision, Sha256Digest, TemplateRevision};
use rivqen_proto::types::{BlockFormat, BlockId};
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::{RqpBlock, split_rqp};
use rivqen_testkit::markup::{ExpectedBlock, MarkupFixture, load};

fn format(text: &str) -> BlockFormat {
    match text {
        "html" => BlockFormat::Html,
        "json" => BlockFormat::Json,
        other => panic!("unknown format {other:?}"),
    }
}

/// The differences of one fixture, as text; empty when it passes.
fn check(fixture: &MarkupFixture) -> Vec<String> {
    let expected = fixture.expected_result().unwrap();
    let result = split_rqp(&fixture.input, &TemplateLimits::default());
    let mut out = Vec::new();
    match (&result, expected.valid) {
        (Ok(doc), true) => {
            let want: Vec<(BlockId, BlockFormat, usize, usize, Sha256Digest)> = expected
                .blocks
                .iter()
                .map(|b: &ExpectedBlock| {
                    (
                        BlockId::parse(&b.id).unwrap(),
                        format(&b.format),
                        b.start,
                        b.end,
                        Sha256Digest::from_b64u(&b.sha256).unwrap(),
                    )
                })
                .collect();
            let got: Vec<(BlockId, BlockFormat, usize, usize, Sha256Digest)> = doc
                .blocks
                .iter()
                .map(|b: &RqpBlock| {
                    (
                        b.descriptor.id.clone(),
                        b.descriptor.format,
                        b.start,
                        b.end,
                        b.descriptor.sha256,
                    )
                })
                .collect();
            if got != want {
                out.push(format!("blocks: expected {want:?}, got {got:?}"));
            }
            let template =
                TemplateRevision::parse(expected.template_revision.as_deref().unwrap()).unwrap();
            if doc.template_revision != template {
                out.push(format!(
                    "template_revision: expected {template}, got {}",
                    doc.template_revision
                ));
            }
            let page = PageRevision::parse(expected.page_revision.as_deref().unwrap()).unwrap();
            let got_page = doc.page_revision(None, None);
            if got_page != page {
                out.push(format!("page_revision: expected {page}, got {got_page}"));
            }
        }
        (Err(error), false) => {
            let want = expected.error.as_deref().unwrap();
            if error.code().as_str() != want {
                out.push(format!("error: expected {want}, got {error}"));
            }
        }
        (Ok(_), false) => out.push(format!(
            "expected invalid ({:?}), got valid",
            expected.error
        )),
        (Err(error), true) => out.push(format!("expected valid, got {error}")),
    }
    out
}

#[test]
fn rqp_markup_fixtures() {
    let fixtures = load().unwrap();
    assert_eq!(fixtures.len(), 362);
    let mut valid = 0;
    let mut failures = Vec::new();
    for fixture in &fixtures {
        if fixture.expected_result().unwrap().valid {
            valid += 1;
        }
        failures.extend(
            check(fixture)
                .into_iter()
                .map(|line| format!("{} {line}", fixture.id)),
        );
    }
    assert_eq!(valid, 148);
    assert_eq!(fixtures.len() - valid, 214);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
