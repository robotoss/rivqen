// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Rivqen diff engine (WP-08 S1 contract; docs/engineering/core/diff-engine.md).
//!
//! - [`legacy`] (feature `legacy`): the data diff of a legacy data update
//!   (`diff_v1`, legacy client CL-42) and the rebuilt page that exists only
//!   after its check (`Rebuilt`, CL-29; DL-169).
//! - [`rqp`]: checks 4 to 13 of an RQP delta in their order and the
//!   validated patch with its apply plan (diff engine sections 3 and 4).
//!   Checks 1 to 3 (navigation, offer, partition) are session checks.
//!
//! The crate is sans-I/O. It never applies a patch that it did not validate
//! (R-CORE-05), and no function panics.

#![forbid(unsafe_code)]

#[cfg(feature = "legacy")]
pub mod legacy;
pub mod rqp;
