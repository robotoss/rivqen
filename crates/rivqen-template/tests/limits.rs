// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `TemplateLimits` from `ResourceLimits` (DL-163).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default
)]

use rivqen_model::limits::ResourceLimits;
use rivqen_template::limits::{
    MAX_LEGACY_NAME_BYTES, MAX_MARKUP_INPUT_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits,
};

#[test]
fn limits_default_equals_the_resource_defaults() {
    let from = TemplateLimits::try_from(&ResourceLimits::default()).unwrap();
    assert_eq!(from, TemplateLimits::default());
    assert_eq!(from.max_document_bytes, MAX_MARKUP_INPUT_BYTES);
    assert_eq!(from.max_blocks, 256);
    assert_eq!(from.max_block_bytes, 1024 * 1024);
    assert_eq!(from.max_name_bytes, MAX_LEGACY_NAME_BYTES);
    assert_eq!(from.max_open_elements, MAX_OPEN_ELEMENTS);
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
    assert_eq!(capped.max_document_bytes, 32 * 1024 * 1024);
    assert_eq!(capped.max_blocks, 256);
    assert_eq!(capped.max_block_bytes, 1024 * 1024);
    assert_eq!(capped.max_name_bytes, MAX_LEGACY_NAME_BYTES);
    assert_eq!(capped.max_open_elements, MAX_OPEN_ELEMENTS);

    resource.max_document_bytes = 1024 * 1024;
    resource.max_blocks = 3;
    resource.max_block_bytes = 10;
    resource.max_block_key_bytes_legacy = 4;
    resource.max_open_elements = 7;
    let lower = TemplateLimits::try_from(&resource).unwrap();
    assert_eq!(
        lower,
        TemplateLimits {
            max_document_bytes: 1024 * 1024,
            max_blocks: 3,
            max_block_bytes: 10,
            max_name_bytes: 4,
            max_open_elements: 7,
        }
    );
}
