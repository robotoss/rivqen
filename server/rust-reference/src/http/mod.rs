// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! HTTP/1.1 reader and writer on `std` (H-113), and the server loop.
//!
//! - [`reader`]: one request head and body from bytes, with the limits of
//!   CONTRACT.md §3 (64 KiB head, 256 field lines, 8 MiB body);
//! - [`writer`]: one response, field lines exactly as given;
//! - [`server`]: the accept loop, one request per connection.

pub mod reader;
pub mod server;
pub mod writer;
