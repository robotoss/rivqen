// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The body of a data update (legacy client CL-29; legacy wire section 5): a
//! JSON object with `data` (string values, in the order of the text),
//! `template-tag`, `html-sha1` and `diff`, read once within the limits.
//!
//! Owner: WP-08 S1, T-01.
