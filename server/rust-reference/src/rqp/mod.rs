// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! RQP engine: response selection of negotiation §4 (lane T-06), with the request
//! fields of negotiation §3 (lane T-05) and the server configuration.

pub mod config;
pub mod request;
pub mod sf;
pub mod types;

use crate::model::{AppResponse, Request, Response, pass_through};
use config::RqpConfig;

/// The response of an RQP server to `request`, given the application response `app`.
///
/// Contract: never fails and never panics; an input that the rules do not process
/// gives A without its `Rq-*` fields (negotiation §4). Linear in the size of the
/// input.
#[must_use]
pub fn respond(config: &RqpConfig, request: &Request, app: &AppResponse) -> Response {
    // Stub of the contract (WP-03 S1); lane T-06 replaces it.
    let _ = config;
    pass_through(request, app)
}
