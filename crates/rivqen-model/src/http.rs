// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! HTTP field lines as the core sees them.

use core::fmt;

/// One HTTP field line: a name and a value, in the order of the message.
///
/// The value can hold a cookie or a token, so [`fmt::Debug`] writes the name
/// and the value length only (errors-logging section 2.4: headers are never
/// logged).
#[derive(Clone, PartialEq, Eq)]
pub struct FieldLine {
    /// Field name as received or sent; compare it ASCII case-insensitively.
    pub name: String,
    /// Field value without the leading and trailing white space of the line.
    pub value: String,
}

impl FieldLine {
    /// A field line.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    /// True when the name equals `name`, ASCII case-insensitively.
    #[must_use]
    pub fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }
}

impl fmt::Debug for FieldLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldLine({}, len={})", self.name, self.value.len())
    }
}
