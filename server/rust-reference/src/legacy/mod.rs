// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Legacy engine: `legacy-canonical` (legacy wire §10, C-01…C-16) and the
//! upstream profiles (§10.1, P-01…P-11). Lane T-03.

use crate::model::{AppResponse, Profile, Request, Response, pass_through};

/// The response of a server in `profile` to `request`, given the application
/// response `app`.
///
/// Contract: never fails and never panics; every input that the rules do not
/// process gives the pass-through. Linear in the size of the input (C-16).
#[must_use]
pub fn respond(profile: Profile, request: &Request, app: &AppResponse) -> Response {
    // Stub of the contract (WP-03 S1); lane T-03 replaces it.
    let _ = profile;
    pass_through(request, app)
}
