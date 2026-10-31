// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! RQP engine: response selection of negotiation §4 (lane T-06, [`select`]), with
//! the request fields of negotiation §3 (lane T-05, [`request`]) and the server
//! configuration ([`config`]).

pub mod config;
pub mod request;
pub mod select;
pub mod sf;
pub mod types;

use crate::model::{AppResponse, Request, Response};
use config::RqpConfig;

/// The response of an RQP server to `request`, given the application response `app`.
///
/// Contract: never fails and never panics; an input that the rules do not process
/// gives A without its `Rq-*` fields (negotiation §4). Linear in the size of the
/// input. [`select::select`] also gives the step that sent the response.
#[must_use]
pub fn respond(config: &RqpConfig, request: &Request, app: &AppResponse) -> Response {
    select::select(config, request, app).0
}
