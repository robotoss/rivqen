// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The state machine of one legacy session (WP-08 S2, T-01; legacy client
//! section 9, CL-05 to CL-37). Private to the crate: the public surface is
//! [`crate::legacy::engine::LegacyEngine`].
//!
//! - [`port`]: the cache record functions that the session calls (the
//!   functions of `record.rs`; unit tests give fakes).
//! - [`head`]: the classification of a response head (CL-13) and the cache
//!   conditions of a response (CL-15, CL-25), as pure functions.
//! - [`exchange`]: one request of a session: its head, its body buffer
//!   with the incremental UTF-8 test (CL-16), and the intercepted requests
//!   that receive its body as a stream (CL-20 item 4).
//! - [`session`]: the state of a session, the request (CL-08 to CL-12),
//!   the routing of the inputs, the refresh (CL-35) and the stop (CL-06,
//!   CL-34).
//! - `outcome`: the outcomes (CL-17 to CL-19, CL-25 to CL-33, CL-44), the
//!   codes and the messages (CL-22 to CL-24, CL-28).
//! - `deliver`: the delivery to the web view (CL-20, CL-21).
//!
//! # Invariants
//!
//! 1. A stopped session gives no effect (CL-06, CL-34); the engine keeps it
//!    only to hold its key until `Destroy` (CL-07).
//! 2. An input of an older generation acts only on the streams of its
//!    exchange and on the cookies (CL-11 item 2, CL-35).
//! 3. Every cache write goes through [`port::RecordPort`] with a
//!    `WriteBasis` of the partition of the session and the epochs of its
//!    last lookup (DL-220).
//! 4. A message takes its diff only from a `rivqen_diff::legacy::Rebuilt`
//!    (DL-226).
//!
//! # Bounds
//!
//! A response body is buffered only for an outcome that needs it, and at
//! most up to the limit of `LegacyConfig` (`template.max_document_bytes`
//! for a page, `body.max_bytes` for a data body); a larger body is a body
//! failure (CL-33, fail closed). A session keeps at most
//! [`session::MAX_EXCHANGES`] exchanges. The UTF-8 test is incremental, so
//! the work per byte is constant.

mod deliver;
pub(crate) mod exchange;
pub(crate) mod head;
mod outcome;
pub(crate) mod port;
pub(crate) mod session;

#[cfg(test)]
mod tests;
