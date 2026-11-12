// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Data model of the Rivqen core (docs/engineering/core/data-model.md).
//!
//! This crate is the base of the core graph (DL-143): identifiers, revisions,
//! secret-safe wrappers, the error catalog of docs/engineering/core/errors.md,
//! resource limits and the clock contract. It has no I/O and no dependency.

#![forbid(unsafe_code)]

pub mod context;
pub mod error;
pub mod http;
pub mod ids;
pub mod limits;
pub mod safe;
pub mod time;
