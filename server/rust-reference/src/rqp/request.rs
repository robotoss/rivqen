// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Request fields of negotiation §3 (lane T-05).

use super::types::{IfNoneMatch, RqpRequest};
use crate::model::Request;

/// Read the RQP request fields of `request` (negotiation §3.2–§3.6, §3.9).
///
/// Contract: never fails and never panics; a field that is invalid is treated as
/// the spec says (mostly: absent). Linear in the size of the fields.
#[must_use]
pub fn parse(request: &Request) -> RqpRequest {
    // Stub of the contract (WP-03 S1); lane T-05 replaces it.
    let _ = request;
    RqpRequest {
        version_1: false,
        capabilities: Vec::new(),
        accepts_delta: false,
        if_none_match: IfNoneMatch::Absent,
        base: None,
        have: None,
        nonce: None,
        has_range: false,
        other_precondition: false,
    }
}
