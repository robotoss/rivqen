// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `Cache-Control`, `Age` and `Vary` parsers of the cache (WP-07 S1, T-02).
//! Placeholder target: T-02 writes the invariants.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_cache::header::{parse_age, parse_cache_control, parse_vary};
use rivqen_model::http::FieldLine;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let fields = [
        FieldLine::new("cache-control", text),
        FieldLine::new("age", text),
        FieldLine::new("vary", text),
    ];
    let _ = parse_cache_control(&fields);
    let _ = parse_age(&fields);
    let _ = parse_vary(&fields);
});
