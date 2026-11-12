// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `TemplateLimits` from `ResourceLimits` (DL-163) and its caps (WP-06 S1
//! security review, finding S1-4).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_model::limits::{Limit, ResourceLimits};
use rivqen_template::limits::{
    MAX_LEGACY_NAME_BYTES, MAX_MARKUP_INPUT_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits,
};

/// The top of the safe range of `maxDocumentBytes` (32 MiB).
fn document_cap() -> usize {
    usize::try_from(Limit::MaxDocumentBytes.range().max()).unwrap()
}

/// The five values in the order of `TemplateLimits::new`.
fn values(limits: &TemplateLimits) -> [usize; 5] {
    [
        limits.max_document_bytes(),
        limits.max_blocks(),
        limits.max_block_bytes(),
        limits.max_name_bytes(),
        limits.max_open_elements(),
    ]
}

#[test]
fn limits_default_equals_the_resource_defaults() {
    let from = TemplateLimits::try_from(&ResourceLimits::default()).unwrap();
    assert_eq!(from, TemplateLimits::default());
    assert_eq!(
        values(&from),
        [
            MAX_MARKUP_INPUT_BYTES,
            256,
            1024 * 1024,
            MAX_LEGACY_NAME_BYTES,
            MAX_OPEN_ELEMENTS,
        ]
    );
}

#[test]
fn limits_protocol_values_are_capped_and_can_be_lowered() {
    let mut resource = ResourceLimits::default();
    resource.max_document_bytes = 32 * 1024 * 1024;
    resource.max_blocks = 1000;
    resource.max_block_bytes = 8 * 1024 * 1024;
    resource.max_block_key_bytes_legacy = 1024;
    resource.max_open_elements = 999;
    let capped = TemplateLimits::try_from(&resource).unwrap();
    assert_eq!(
        values(&capped),
        [
            32 * 1024 * 1024,
            256,
            1024 * 1024,
            MAX_LEGACY_NAME_BYTES,
            MAX_OPEN_ELEMENTS,
        ]
    );

    resource.max_document_bytes = 1024 * 1024;
    resource.max_blocks = 3;
    resource.max_block_bytes = 10;
    resource.max_block_key_bytes_legacy = 4;
    resource.max_open_elements = 7;
    let lower = TemplateLimits::try_from(&resource).unwrap();
    assert_eq!(lower, TemplateLimits::new(1024 * 1024, 3, 10, 4, 7));
    assert_eq!(values(&lower), [1024 * 1024, 3, 10, 4, 7]);
}

/// `new` caps each value at its cap, keeps each value at or below it, and
/// keeps the values apart (no value goes to another field).
#[test]
fn limits_new_caps_every_value() {
    const CONST_LIMITS: TemplateLimits =
        TemplateLimits::new(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX);
    let caps = [
        document_cap(),
        256,
        1024 * 1024,
        MAX_LEGACY_NAME_BYTES,
        MAX_OPEN_ELEMENTS,
    ];
    assert_eq!(caps[0], 32 * 1024 * 1024);
    let max = TemplateLimits::new(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX);
    assert_eq!(values(&max), caps);
    let one_above = TemplateLimits::new(
        caps[0] + 1,
        caps[1] + 1,
        caps[2] + 1,
        caps[3] + 1,
        caps[4] + 1,
    );
    assert_eq!(values(&one_above), caps);
    let at = TemplateLimits::new(caps[0], caps[1], caps[2], caps[3], caps[4]);
    assert_eq!(values(&at), caps);
    let one_below = TemplateLimits::new(
        caps[0] - 1,
        caps[1] - 1,
        caps[2] - 1,
        caps[3] - 1,
        caps[4] - 1,
    );
    assert_eq!(values(&one_below), caps.map(|cap| cap - 1));
    assert_eq!(values(&TemplateLimits::new(1, 2, 3, 4, 5)), [1, 2, 3, 4, 5]);
    assert_eq!(values(&TemplateLimits::new(0, 0, 0, 0, 0)), [0; 5]);
    // A `const` value is capped the same way.
    assert_eq!(CONST_LIMITS, max);
}

/// `try_from` caps `maxDocumentBytes` at the top of its safe range, as
/// `new` does: a configuration that skipped the range check cannot raise
/// it.
#[test]
fn limits_try_from_caps_document() {
    let mut resource = ResourceLimits::default();
    for (configured, expected) in [
        (u64::MAX, document_cap()),
        (Limit::MaxDocumentBytes.range().max() + 1, document_cap()),
        (Limit::MaxDocumentBytes.range().max(), document_cap()),
        (
            Limit::MaxDocumentBytes.range().max() - 1,
            document_cap() - 1,
        ),
    ] {
        resource.max_document_bytes = configured;
        let limits = TemplateLimits::try_from(&resource);
        if usize::try_from(configured).is_err() {
            // A 32-bit target: the value does not fit `usize`.
            assert_eq!(limits.unwrap_err().limit(), Limit::MaxDocumentBytes);
        } else {
            assert_eq!(limits.unwrap().max_document_bytes(), expected);
        }
    }
}
