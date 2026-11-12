// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Legacy rebuild: `render_v1` on a template and data built from the bytes.
//! The bytes are read as lossy UTF-8 and split at U+0001: the first part is
//! the template; each other part is one data entry, `key U+0002 value` or,
//! without U+0002, a value with a generated key (`{title}` for the first).
//! Invariants: no panic; an `Ok` page is at most `max_document_bytes`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_template::legacy::{DataBlock, render_v1};
use rivqen_template::limits::TemplateLimits;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let mut parts = text.split('\u{1}');
    let template = parts.next().unwrap_or_default();
    let entries: Vec<DataBlock> = parts
        .enumerate()
        .map(|(index, part)| match part.split_once('\u{2}') {
            Some((key, value)) => DataBlock {
                key: key.to_owned(),
                value: value.to_owned(),
            },
            None => DataBlock {
                key: if index == 0 {
                    "{title}".to_owned()
                } else {
                    format!("{{b{index}}}")
                },
                value: part.to_owned(),
            },
        })
        .collect();
    let small = TemplateLimits {
        max_document_bytes: 64,
        max_blocks: 4,
        max_block_bytes: 16,
        ..TemplateLimits::default()
    };
    for limits in [TemplateLimits::default(), small] {
        if let Ok(page) = render_v1(template, &entries, &limits) {
            assert!(page.len() <= limits.max_document_bytes);
        }
    }
});
