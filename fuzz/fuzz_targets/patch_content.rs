// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Patch content rule on arbitrary block content: the input is the new
//! content of the `price` block of `fixtures/rqp/protocol/pages/page-a.html`.
//! The target renders the patched document (`render_rqp`), analyzes it
//! (`split_rqp`) and, when it is valid, runs `check_patch_content` on the
//! patched block.
//! Invariants: no panic; an accepted content has, by the scan, no start tag
//! in HTML context with a `PATCH_FORBIDDEN` name and no attribute name that
//! starts with `on`; the result is the same on a second call.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::content::{PATCH_FORBIDDEN, check_patch_content};
use rivqen_template::rqp::patch::{Replacement, render_rqp};
use rivqen_template::rqp::scan::{Token, scan};
use rivqen_template::rqp::split_rqp;

/// The stored page; block 1 is `price`.
const PAGE: &[u8] = include_bytes!("../../fixtures/rqp/protocol/pages/page-a.html");
const PRICE: usize = 1;

fuzz_target!(|data: &[u8]| {
    let limits = TemplateLimits::default();
    let Ok(stored) = split_rqp(PAGE, &limits) else {
        // The page is a fixture: it must split.
        panic!("page-a does not split");
    };
    let replacements = [Replacement {
        block: PRICE,
        content: data,
    }];
    let Ok(patched) = render_rqp(PAGE, &stored, &replacements, &limits) else {
        return;
    };
    let Ok(analysis) = split_rqp(&patched, &limits) else {
        return;
    };
    let first = check_patch_content(&patched, &analysis, &[PRICE], &limits);
    let second = check_patch_content(&patched, &analysis, &[PRICE], &limits);
    assert_eq!(first, second, "the rule is not deterministic");
    if first.is_err() {
        return;
    }
    // Accepted: read the patched block with the scan.
    let Some(block) = analysis.blocks.get(PRICE) else {
        return;
    };
    for item in scan(&patched, &limits) {
        let Ok(Token::StartTag(tag)) = item else {
            continue;
        };
        let span = tag.span();
        if span.start < block.start || span.end > block.end {
            continue;
        }
        if !tag.in_foreign_region() {
            let name = tag.name();
            assert!(
                !PATCH_FORBIDDEN
                    .iter()
                    .any(|forbidden| name.eq_ignore_ascii_case(forbidden.as_bytes())),
                "an accepted content has a forbidden start tag"
            );
        }
        for attribute in tag.attributes().flatten() {
            let name = attribute.name();
            assert!(
                !(name.len() >= 2 && name[..2].eq_ignore_ascii_case(b"on")),
                "an accepted content has an event handler attribute"
            );
        }
    }
});
