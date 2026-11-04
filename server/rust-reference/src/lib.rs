// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Rivqen reference server (oracle, WP-03).
//!
//! An independent implementation of the server side of RQP `rqp/1`
//! (`docs/engineering/protocol/negotiation.md` §4, `manifest-patch.md`) and of the
//! legacy profiles of ADR-005 (`docs/engineering/protocol/legacy-wire.md` §10 and
//! §10.1). It is written from these pages, not from the JavaScript models that
//! generated the fixtures (DL-087). The conformance runner tests it by URL
//! (`server/conformance/CONTRACT.md`).
//!
//! Layout (WP-03 S1):
//! - [`model`]: the contract types of all lanes;
//! - [`legacy`]: the legacy engine;
//! - [`rqp`]: the RQP engine;
//! - [`http`]: the HTTP/1.1 reader and writer on `std` (H-113);
//! - [`control`]: the conformance control endpoint (DL-086).

#![forbid(unsafe_code)]

pub mod control;
pub mod http;
pub mod legacy;
pub mod model;
pub mod rqp;
