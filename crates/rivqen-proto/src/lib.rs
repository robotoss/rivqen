// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Rivqen protocol `rqp/1`: types and codecs (DL-144, DL-145).
//!
//! Everything here needs no HTML parse and no session state. Normative text:
//! docs/engineering/protocol/negotiation.md (fields) and
//! docs/engineering/protocol/manifest-patch.md (manifest, envelope, hashes).
//! The markup analysis, the build of the patched document and the patch
//! content rule are in `rivqen-template` (WP-06, H-142); the order of the
//! delta checks and the apply plan are in `rivqen-diff` (WP-08).

#![forbid(unsafe_code)]

pub mod compute;
pub mod delta;
pub mod envelope;
pub mod fields;
pub mod json;
pub mod manifest;
pub mod sf;
pub mod types;
