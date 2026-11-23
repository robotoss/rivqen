// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Rivqen legacy protocol on the client (WP-08 S1 contract; DL-160, DL-222).
//!
//! The whole crate is legacy: WP-23 removes it in 1.5. It is sans-I/O and
//! reads untrusted field lines and bodies; no function panics, and every
//! input that is not valid gives the answer "not a legacy response" or a
//! typed error, so that the session loads the page normally.
//!
//! Normative text: docs/engineering/protocol/legacy-client.md section 9
//! (CL-09, CL-14 to CL-17, CL-21, CL-22, CL-24, CL-29) and
//! docs/engineering/protocol/legacy-wire.md. The lane T-01 defines the public
//! items of the modules and lists them in its report.
//!
//! - [`body`]
//! - [`fields`]
//! - [`message`]
//! - [`request`]
//! - [`response`]

#![forbid(unsafe_code)]

pub mod body;
pub mod fields;
pub mod message;
pub mod request;
pub mod response;
