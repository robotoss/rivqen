// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! HTTP/1.1 reader and writer on `std` (H-113), the handler interface and the
//! server loop.
//!
//! - [`reader`]: one request head and body from bytes, with the limits of
//!   CONTRACT.md §3 (64 KiB head, 256 field lines, 8 MiB body);
//! - [`writer`]: one response, field lines exactly as given;
//! - [`handler`]: the interface between the server loop and a mode of the server;
//! - [`server`]: the accept loop, the queue and the workers, one request per
//!   connection.

pub mod handler;
pub mod reader;
pub mod server;
pub mod writer;
