// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Linear time of the legacy split and rebuild (legacy wire C-16, DL-077):
//! 256 KiB of adversarial input in less than 100 ms.
//!
//! Wall-clock budgets hold only in an optimized build, so the test is
//! ignored by default. Run it with:
//!
//! ```text
//! cargo test -p rivqen-template --release --test legacy_timing -- --ignored
//! ```

#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::arithmetic_side_effects
)]

use std::time::{Duration, Instant};

use rivqen_template::legacy::{DataBlock, render_v1, split_v1};
use rivqen_template::limits::TemplateLimits;

const SIZE: usize = 256 * 1024;
const BUDGET: Duration = Duration::from_millis(100);

/// `unit` repeated to at least [`SIZE`] bytes, then `end`.
fn repeat(unit: &str, end: &str) -> Vec<u8> {
    let mut text = unit.repeat(SIZE / unit.len() + 1);
    text.push_str(end);
    text.into_bytes()
}

/// The best of three runs, so that one scheduler pause does not fail it.
fn best_of_three(run: impl Fn()) -> Duration {
    (0..3)
        .map(|_| {
            let start = Instant::now();
            run();
            start.elapsed()
        })
        .min()
        .unwrap()
}

#[test]
#[ignore = "timing: run in release with --ignored"]
fn legacy_linear_time() {
    let limits = TemplateLimits::default();
    let shapes: [(&str, Vec<u8>); 9] = [
        ("title openers without LF", repeat("<title", "")),
        (
            "title openers, LF before the close",
            repeat("<title", "\n</title>"),
        ),
        ("title openers in all cases", repeat("<TiTlE x", "")),
        ("unclosed block openers", repeat("<!--sonicdiff-a-->x", "")),
        (
            "title and block openers",
            repeat("<title<!--sonicdiff-a-->x", ""),
        ),
        ("end marker prefixes after one opener", {
            let mut page = b"<!--sonicdiff-a-->x".to_vec();
            page.extend(repeat("<!--sonicdiff-a-end--", ""));
            page
        }),
        (
            "long names without a tail",
            repeat(&format!("<!--sonicdiff-{}", "n".repeat(500)), ""),
        ),
        ("all `<`", repeat("<", "")),
        (
            "many small blocks",
            repeat("<!--sonicdiff-->x<!--sonicdiff-end-->", ""),
        ),
    ];
    for (shape, page) in &shapes {
        assert!(page.len() >= SIZE);
        let took = best_of_three(|| {
            split_v1(page, &limits).unwrap();
        });
        eprintln!("split {shape}: {took:?}");
        assert!(took < BUDGET, "split {shape}: {took:?}");
    }

    // The rebuild: a template of key-like text and 256 keys that are not in
    // it (every `{` is a candidate).
    let template = String::from_utf8(repeat("{k1{k2}{", "")).unwrap();
    let mut data = vec![DataBlock {
        key: "{title}".to_owned(),
        value: String::new(),
    }];
    data.extend((0..256).map(|i| DataBlock {
        key: format!("{{k{}}}", 1000 + i),
        value: "v".to_owned(),
    }));
    let took = best_of_three(|| {
        assert!(render_v1(&template, &data, &limits).is_err());
    });
    eprintln!("render: {took:?}");
    assert!(took < BUDGET, "render: {took:?}");
}
