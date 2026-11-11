// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! RQP request fields built from a snapshot and response fields read by the
//! client (negotiation section 3). Owner: WP-05 S1 T-03.
//!
//! # Request (negotiation sections 3 and 6.2)
//!
//! [`request_fields`] gives the field lines of one network attempt, in this
//! order: `rq-version`, `rq-capabilities`, `accept`, and for a delta request
//! `if-none-match`, `rq-have` and `rq-nonce`. Names are in lower case (the
//! HTTP/2 and HTTP/3 form). The values equal the request lines of the client
//! fixtures `FX-RQ-C*` byte for byte; the oracle is `requestHeaders` of
//! `tools/rqp-protocol/src/client.mjs`.
//!
//! The caller decides whether RQP fields go to the origin at all (allowlist,
//! scheme, user information, the `public` partition setting; negotiation
//! section 6.1; WP-08) and gives [`RequestBasis::FirstVisit`] when it has no
//! usable snapshot.
//!
//! # Response (negotiation section 6.3, field parts)
//!
//! [`ResponseHead::check`] tests the field lines against `maxHeaderBytes`
//! (DL-153). The methods [`ResponseHead::rq_version_is_1`],
//! [`ResponseHead::media_type`], [`ResponseHead::content_type_charset`],
//! [`ResponseHead::decoded_as_utf8`] and
//! [`ResponseHead::etag_matches_snapshot`] read the response fields as
//! `handleResponse`, `decodedAsUtf8` and `notModifiedAccepted` of the oracle
//! do. No other path reads a response field.
//!
//! # Safety rules
//!
//! No function logs, and no error or `Debug` output holds a byte of a field
//! value. Every read is linear in the field values (DL-063).

mod request;
mod response;

pub use request::{RequestBasis, RequestFieldsError, request_fields};
pub use response::{ContentTypeCharset, HeadLimitError, ResponseHead};

/// The delta media type (negotiation section 3.6).
pub const DELTA_MEDIA_TYPE: &str = "application/vnd.rivqen.delta+json";
/// The media type of a full document.
pub const HTML_MEDIA_TYPE: &str = "text/html";
/// The capability token of a client that can apply a delta (negotiation
/// section 3.3).
pub const CAPABILITY_BLOCK_PATCH: &str = "block-patch";
/// The RQP major version of `rqp/1` (negotiation section 3.2).
pub const RQP_VERSION: i64 = 1;

/// Field name `Rq-Version`, lower case.
pub const RQ_VERSION: &str = "rq-version";
/// Field name `Rq-Capabilities`, lower case.
pub const RQ_CAPABILITIES: &str = "rq-capabilities";
/// Field name `Rq-Have`, lower case.
pub const RQ_HAVE: &str = "rq-have";
/// Field name `Rq-Nonce`, lower case.
pub const RQ_NONCE: &str = "rq-nonce";
/// Field name `Accept`, lower case.
pub const ACCEPT: &str = "accept";
/// Field name `If-None-Match`, lower case.
pub const IF_NONE_MATCH: &str = "if-none-match";
/// Field name `Content-Type`, lower case.
pub const CONTENT_TYPE: &str = "content-type";
/// Field name `ETag`, lower case.
pub const ETAG: &str = "etag";
