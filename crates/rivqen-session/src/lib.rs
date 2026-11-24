// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Rivqen session state machine (WP-08; docs/engineering/core/session-fsm.md).
//!
//! The crate is sans-I/O (ADR-001): the adapter gives inputs, the session
//! returns effects, and the adapter executes them in order and reports their
//! results as new inputs. It starts no thread, reads no clock and holds no
//! global state; the same inputs in the same order give the same effects.
//!
//! - [`legacy`] (feature `legacy`): the legacy session of the Rivqen
//!   canonical client (legacy client section 9, CL rules), with the
//!   presentations Quick and Standard (WP-08 S2).
//!
//! The RQP session follows in WP-08 S3; the facade of `rivqen-core` maps its
//! public events and actions to these types.

#![forbid(unsafe_code)]

#[cfg(feature = "legacy")]
pub mod legacy;
