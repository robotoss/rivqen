// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Strict JSON reader of manifest-patch section 3.2. Owner: WP-05 S1 T-04.
//!
//! A JSON text (RFC 8259) with the extra rules of manifest-patch section 3.2:
//!
//! 1. UTF-8, no byte order mark ([`JsonRule::Encoding`], [`JsonRule::Bom`]).
//! 2. No duplicate member names in any object, compared after the escapes
//!    are decoded ([`JsonRule::Duplicate`]).
//! 3. No lone surrogate escape ([`JsonRule::Surrogate`]).
//! 4. Numbers are integers written as digits only: no sign, no fraction, no
//!    exponent ([`JsonRule::Integer`]). The only numbers of `rqp/1`
//!    documents are such integers, so the reader applies the rule to every
//!    number (as the reference reader `tools/rqp-protocol/src/json-strict.mjs`).
//!    An integer above [`u64::MAX`] also breaks this rule: every `rqp/1` range
//!    is far below it.
//! 5. Nesting depth at most `max_depth` ([`JsonRule::Depth`]); the top-level
//!    object or array has depth 1. The default is
//!    [`MAX_JSON_DEPTH`](crate::types::MAX_JSON_DEPTH) (16).
//!
//! Every other break of RFC 8259 is [`JsonRule::Syntax`].
//!
//! Two interfaces:
//!
//! - [`Reader`] gives the text as a stream of [`Event`]s. It keeps no value
//!   tree, so a consumer that checks a schema while it reads (the manifest and
//!   the envelope) needs memory only for the values it keeps.
//! - [`parse`] builds a [`Value`] tree with the reader.
//!
//! Bounds: the reader never panics and never recurses. Its work is linear in
//! the input, except the duplicate check: each member name goes into an
//! ordered set of its object, so the check is `O(n log n)` in the input length
//! `n`. Its memory is the container stack (at most `max_depth` frames) and the
//! member names of the open objects (at most the input length). Errors carry a
//! rule and a byte offset, never input bytes.

mod reader;
mod schema;
mod value;

use core::fmt;

pub use reader::{Event, Reader};
pub(crate) use schema::{Item, next_item, next_member};
pub use schema::{SchemaKeyword, SchemaViolation};
pub use value::{Member, Value, parse, parse_with_max_depth};

/// The rule of manifest-patch section 3.2 (or RFC 8259) that a text breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JsonRule {
    /// Rule 1: the text is not UTF-8.
    Encoding,
    /// Rule 1: the text starts with a byte order mark.
    Bom,
    /// RFC 8259 grammar.
    Syntax,
    /// Rule 2: an object has two members with the same name.
    Duplicate,
    /// Rule 3: a `\u` escape of a surrogate without its pair.
    Surrogate,
    /// Rule 4: a number with a sign, a fraction or an exponent, or an integer
    /// above [`u64::MAX`].
    Integer,
    /// Rule 5: nesting deeper than the depth limit.
    Depth,
}

impl JsonRule {
    /// A short name of the rule: `"encoding"`, `"bom"`, `"syntax"`,
    /// `"duplicate"`, `"surrogate"`, `"integer"` or `"depth"` (the names of
    /// the reference reader).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Encoding => "encoding",
            Self::Bom => "bom",
            Self::Syntax => "syntax",
            Self::Duplicate => "duplicate",
            Self::Surrogate => "surrogate",
            Self::Integer => "integer",
            Self::Depth => "depth",
        }
    }

    const fn describe(self) -> &'static str {
        match self {
            Self::Encoding => "not UTF-8 (manifest-patch 3.2 rule 1)",
            Self::Bom => "byte order mark (manifest-patch 3.2 rule 1)",
            Self::Syntax => "not a JSON text (RFC 8259)",
            Self::Duplicate => "duplicate member name (manifest-patch 3.2 rule 2)",
            Self::Surrogate => "lone surrogate escape (manifest-patch 3.2 rule 3)",
            Self::Integer => "number is not an integer of digits only (manifest-patch 3.2 rule 4)",
            Self::Depth => "nesting too deep (manifest-patch 3.2 rule 5)",
        }
    }
}

impl fmt::Display for JsonRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A text that the strict reader rejects: the rule and the byte offset where
/// the reader found the break. It holds no byte of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JsonError {
    rule: JsonRule,
    offset: usize,
}

impl JsonError {
    pub(crate) const fn new(rule: JsonRule, offset: usize) -> Self {
        Self { rule, offset }
    }

    /// The broken rule.
    #[must_use]
    pub const fn rule(&self) -> JsonRule {
        self.rule
    }

    /// Byte offset in the text where the reader found the break. For
    /// [`JsonRule::Encoding`] this is the length of the valid UTF-8 prefix.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.rule.describe(), self.offset)
    }
}

impl core::error::Error for JsonError {}
