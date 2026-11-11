// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Rivqen core facade (DL-143): the engine configuration and its limit check,
//! and the event, action and outcome types of docs/engineering/core/api.md and
//! docs/engineering/core/data-model.md section 5.
//!
//! Skeleton only: there is no session logic. The session (WP-08), the cache
//! (WP-07), streaming and the FFI (WP-09) fill the payloads and the engine
//! calls. Owner: WP-05 S2 T-04.

#![forbid(unsafe_code)]

pub mod action;
pub mod config;
pub mod event;
