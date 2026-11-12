// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Schema violations of `spec/rqp/schemas/` and the reader steps that the
//! manifest and envelope checks share.

use std::borrow::Cow;
use std::fmt;

use super::{Event, JsonError, JsonRule, Reader};

/// The JSON Schema keyword whose check failed (the names of JSON Schema Draft
/// 2020-12, as the invalid examples of `spec/rqp/examples/invalid` name them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SchemaKeyword {
    /// `type`
    Type,
    /// `const`
    Const,
    /// `enum`
    Enum,
    /// `pattern`
    Pattern,
    /// `minLength`
    MinLength,
    /// `maxLength`
    MaxLength,
    /// `minimum`
    Minimum,
    /// `maximum`
    Maximum,
    /// `minItems`
    MinItems,
    /// `maxItems`
    MaxItems,
    /// `required`
    Required,
    /// `additionalProperties` (an unknown member)
    AdditionalProperties,
}

impl SchemaKeyword {
    /// The keyword as JSON Schema writes it, for example `"minItems"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Const => "const",
            Self::Enum => "enum",
            Self::Pattern => "pattern",
            Self::MinLength => "minLength",
            Self::MaxLength => "maxLength",
            Self::Minimum => "minimum",
            Self::Maximum => "maximum",
            Self::MinItems => "minItems",
            Self::MaxItems => "maxItems",
            Self::Required => "required",
            Self::AdditionalProperties => "additionalProperties",
        }
    }
}

impl fmt::Display for SchemaKeyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A failed schema check: the keyword and where it failed. It holds no value
/// of the document.
///
/// The place is a JSON Pointer template from a fixed list (for example
/// `"/policy/ttl_seconds"` or `"/operations/*/block_id"`) and, for a place in
/// an operation, the operation index that replaces `*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchemaViolation {
    keyword: SchemaKeyword,
    pointer: &'static str,
    operation: Option<usize>,
}

impl SchemaViolation {
    pub(crate) const fn at(keyword: SchemaKeyword, pointer: &'static str) -> Self {
        Self {
            keyword,
            pointer,
            operation: None,
        }
    }

    pub(crate) const fn in_operation(
        keyword: SchemaKeyword,
        pointer: &'static str,
        operation: usize,
    ) -> Self {
        Self {
            keyword,
            pointer,
            operation: Some(operation),
        }
    }

    /// The failed keyword.
    #[must_use]
    pub const fn keyword(&self) -> SchemaKeyword {
        self.keyword
    }

    /// The JSON Pointer template of the place (`*` for the operation index).
    #[must_use]
    pub const fn pointer(&self) -> &'static str {
        self.pointer
    }

    /// The operation index, for a place in an operation.
    #[must_use]
    pub const fn operation(&self) -> Option<usize> {
        self.operation
    }

    /// The JSON Pointer of the place (`instancePath` of JSON Schema), with the
    /// operation index in place of `*`.
    #[must_use]
    pub fn instance_path(&self) -> String {
        match self.operation {
            Some(i) => self.pointer.replacen('*', &i.to_string(), 1),
            None => self.pointer.to_owned(),
        }
    }
}

impl fmt::Display for SchemaViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "schema keyword {} fails at '{}'",
            self.keyword,
            self.instance_path()
        )
    }
}

impl core::error::Error for SchemaViolation {}

/// The start of the next value, as a schema check needs it.
pub(crate) enum Item<'a> {
    String(Cow<'a, str>),
    Integer(u64),
    Bool(bool),
    Null,
    /// `{`: the members follow.
    Object,
    /// `[`: the items follow.
    Array,
    /// `]`: the end of the array whose items the caller reads.
    End,
}

/// The next value (or the end of the array that the caller reads).
pub(crate) fn next_item<'a>(reader: &mut Reader<'a>) -> Result<Item<'a>, JsonError> {
    match reader.next_event()? {
        Some(Event::String(s)) => Ok(Item::String(s)),
        Some(Event::Integer(n)) => Ok(Item::Integer(n)),
        Some(Event::Bool(b)) => Ok(Item::Bool(b)),
        Some(Event::Null) => Ok(Item::Null),
        Some(Event::StartObject) => Ok(Item::Object),
        Some(Event::StartArray) => Ok(Item::Array),
        Some(Event::EndArray) => Ok(Item::End),
        // The reader gives no other event where a value or `]` comes.
        Some(Event::Name(_) | Event::EndObject) | None => Err(reader.fail(JsonRule::Syntax)),
    }
}

/// The next member name of the object that the caller reads, or `None` at its
/// end.
pub(crate) fn next_member<'a>(reader: &mut Reader<'a>) -> Result<Option<Cow<'a, str>>, JsonError> {
    match reader.next_event()? {
        Some(Event::Name(name)) => Ok(Some(name)),
        Some(Event::EndObject) => Ok(None),
        // The reader gives no other event where a name or `}` comes.
        _ => Err(reader.fail(JsonRule::Syntax)),
    }
}
