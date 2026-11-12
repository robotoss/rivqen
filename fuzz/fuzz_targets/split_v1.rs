// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Legacy split: `split_v1` with the default limits on any bytes.
//! Invariants: no panic; `{title}` is the first data entry; the rebuild with
//! `render_v1` gives the input back whenever each placeholder key stands
//! once in the template (P-TPL-1, DL-166, DL-167); outside that domain a
//! rebuild may differ from the input or fail with `Mismatch`, never panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_template::legacy::{RenderError, SplitOutcome, render_v1, split_v1};
use rivqen_template::limits::TemplateLimits;

fuzz_target!(|data: &[u8]| {
    let limits = TemplateLimits::default();
    let Ok(SplitOutcome::Applied(split)) = split_v1(data, &limits) else {
        return;
    };
    assert_eq!(split.data.first().map(|b| b.key.as_str()), Some("{title}"));
    // The domain: every entry that needs a placeholder has its key exactly
    // once in the template (so the placeholder is the first occurrence). An
    // empty title has no placeholder.
    let in_domain = split
        .data
        .iter()
        .filter(|b| !(b.key == "{title}" && b.value.is_empty()))
        .all(|b| split.template.matches(b.key.as_str()).count() == 1);
    match render_v1(&split.template, &split.data, &limits) {
        Ok(page) => {
            if in_domain {
                assert_eq!(page.as_bytes(), data, "round trip inside the domain");
            }
            assert!(page.len() <= limits.max_document_bytes);
        }
        Err(error) => {
            assert!(!in_domain, "rebuild failed inside the domain: {error:?}");
            assert!(matches!(
                error,
                RenderError::Mismatch | RenderError::Limit(_)
            ));
        }
    }
});
