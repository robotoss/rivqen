// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Rivqen template engine (WP-06; docs/engineering/core/template-engine.md).
//!
//! - [`legacy`] (feature `legacy`, DL-164): the canonical legacy marker split
//!   and rebuild (legacy wire C-04, C-06 to C-08, C-16; CL-41; DL-159) and the
//!   legacy SHA-1 values.
//! - [`rqp`]: the RQP markup analysis (markup M-01 to M-27) and its token
//!   scanner (DL-161).
//! - [`limits`]: the one limits value that every parser takes (DL-163).
//!
//! The crate is sans-I/O. Every parser takes untrusted bytes, never panics
//! and stays within [`limits::TemplateLimits`].

#![forbid(unsafe_code)]

#[cfg(feature = "legacy")]
pub mod legacy;
pub mod limits;
pub mod rqp;
