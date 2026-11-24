// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The legacy session (WP-08 S2 contract; legacy client section 9).
//!
//! - [`types`]: the inputs, the effects, the configuration and the open
//!   context (architect contract; the lanes do not change it).
//! - [`engine`]: [`engine::LegacyEngine`], every open session of one
//!   engine: the second session for one key (CL-07), the partition change
//!   (CL-06), the session state machine (CL-08 to CL-37). Lane L1.
//! - `fsm` (private): the state machine of one session (CL-05 to CL-37).
//!   Lane L1.
//! - [`record`]: the cache record of a legacy session (CL-38): the key of
//!   one entry per URL (H-160), the visible entry (CL-12, CL-40) and the
//!   cache transactions of the session with the request precondition
//!   (DL-220, legacy client section 9.12). Lane L2.

pub mod engine;
mod fsm;
pub mod record;
pub mod types;
