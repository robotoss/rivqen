// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Patch content rule on arbitrary block content (WP-06 S2 contract; the
//! lane T-05 writes the body and the invariants).
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::content::check_attribute;

fuzz_target!(|data: &[u8]| {
    let _limits = TemplateLimits::default();
    if let Err(violation) = check_attribute(b"a", false, b"href", data) {
        let _reason = violation.reason();
    }
});
