// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The default `ResourceLimits` of `rivqen-model` and the safe ranges of the
//! limits fixed by `rqp/1` agree with the protocol constants of this crate,
//! so the two tables cannot drift (WP-05 S2, T-04 review note).

#![allow(clippy::unwrap_used, reason = "test code")]

use rivqen_model::limits::{Limit, ResourceLimits};
use rivqen_proto::types::{
    MAX_BLOCK_BYTES, MAX_BLOCK_ID_BYTES, MAX_BLOCKS, MAX_JSON_DEPTH, MAX_MANIFEST_BYTES,
    MAX_PATCH_BYTES,
};

fn wide(value: usize) -> u64 {
    u64::try_from(value).unwrap()
}

#[test]
fn limits_defaults_equal_the_protocol_constants() {
    let defaults = ResourceLimits::default();
    for (limit, constant) in [
        (Limit::MaxBlocks, MAX_BLOCKS),
        (Limit::MaxBlockBytes, MAX_BLOCK_BYTES),
        (Limit::MaxBlockKeyBytesRqp, MAX_BLOCK_ID_BYTES),
        (Limit::MaxManifestBytes, MAX_MANIFEST_BYTES),
        (Limit::MaxPatchBytes, MAX_PATCH_BYTES),
        (Limit::MaxJsonDepth, MAX_JSON_DEPTH),
    ] {
        assert_eq!(defaults.get(limit), wide(constant), "{}", limit.name());
    }
}

#[test]
fn limits_fixed_by_rqp1_have_a_range_of_one_value() {
    for (limit, constant) in [
        (Limit::MaxBlocks, MAX_BLOCKS),
        (Limit::MaxBlockBytes, MAX_BLOCK_BYTES),
        (Limit::MaxBlockKeyBytesRqp, MAX_BLOCK_ID_BYTES),
        (Limit::MaxManifestBytes, MAX_MANIFEST_BYTES),
    ] {
        let range = limit.range();
        assert_eq!(range.min(), wide(constant), "{}", limit.name());
        assert_eq!(range.max(), wide(constant), "{}", limit.name());
    }
    // The client JSON depth may only be lower than the reader cap (DL-152 S-1).
    assert_eq!(Limit::MaxJsonDepth.range().max(), wide(MAX_JSON_DEPTH));
}
