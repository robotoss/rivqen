// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! A value tree built with the strict reader.

use std::fmt;

use super::{Event, JsonError, JsonRule, Reader};
use crate::types::MAX_JSON_DEPTH;

/// A JSON value that passed the rules of manifest-patch section 3.2.
///
/// Numbers are integers (rule 4). Object members keep the order of the text
/// and have unique names (rule 2). [`fmt::Debug`] writes the shape and the
/// lengths of strings and names, never their text.
#[derive(Clone, PartialEq, Eq)]
pub enum Value {
    /// `null`
    Null,
    /// `true` or `false`
    Bool(bool),
    /// An integer of digits only.
    Integer(u64),
    /// A decoded string.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object, members in text order.
    Object(Vec<Member>),
}

/// One member of an object.
#[derive(Clone, PartialEq, Eq)]
pub struct Member {
    /// The decoded member name.
    pub name: String,
    /// The member value.
    pub value: Value,
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("Null"),
            Self::Bool(b) => write!(f, "Bool({b})"),
            Self::Integer(n) => write!(f, "Integer({n})"),
            Self::String(s) => write!(f, "String(len={})", s.len()),
            Self::Array(items) => f.debug_list().entries(items).finish(),
            Self::Object(members) => f.debug_list().entries(members).finish(),
        }
    }
}

impl fmt::Debug for Member {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Member(name len={}, {:?})", self.name.len(), self.value)
    }
}

/// Parse a JSON text with the depth limit [`MAX_JSON_DEPTH`].
///
/// # Errors
///
/// A [`JsonError`] for the first break of a rule of manifest-patch section 3.2
/// or of RFC 8259.
pub fn parse(text: &[u8]) -> Result<Value, JsonError> {
    parse_with_max_depth(text, MAX_JSON_DEPTH)
}

/// An open container of the builder.
enum Open {
    Array(Vec<Value>),
    /// The members so far and the name of the member whose value comes next.
    Object(Vec<Member>, Option<String>),
}

/// Parse a JSON text with the depth limit `max_depth` (rule 5).
///
/// # Errors
///
/// A [`JsonError`] for the first break of a rule of manifest-patch section 3.2
/// or of RFC 8259.
pub fn parse_with_max_depth(text: &[u8], max_depth: usize) -> Result<Value, JsonError> {
    let mut reader = Reader::new(text, max_depth)?;
    let mut stack: Vec<Open> = Vec::new();
    loop {
        // The reader gives a complete value before it gives `None`.
        let Some(event) = reader.next_event()? else {
            return Err(reader.fail(JsonRule::Syntax));
        };
        let done = match event {
            Event::StartObject => {
                stack.push(Open::Object(Vec::new(), None));
                continue;
            }
            Event::StartArray => {
                stack.push(Open::Array(Vec::new()));
                continue;
            }
            Event::Name(name) => {
                let Some(Open::Object(_, pending)) = stack.last_mut() else {
                    return Err(reader.fail(JsonRule::Syntax));
                };
                *pending = Some(name.into_owned());
                continue;
            }
            Event::EndObject => match stack.pop() {
                Some(Open::Object(members, None)) => Value::Object(members),
                _ => return Err(reader.fail(JsonRule::Syntax)),
            },
            Event::EndArray => match stack.pop() {
                Some(Open::Array(items)) => Value::Array(items),
                _ => return Err(reader.fail(JsonRule::Syntax)),
            },
            Event::String(text) => Value::String(text.into_owned()),
            Event::Integer(n) => Value::Integer(n),
            Event::Bool(b) => Value::Bool(b),
            Event::Null => Value::Null,
        };
        match stack.last_mut() {
            None => {
                reader.finish()?;
                return Ok(done);
            }
            Some(Open::Array(items)) => items.push(done),
            Some(Open::Object(members, pending)) => {
                let Some(name) = pending.take() else {
                    return Err(reader.fail(JsonRule::Syntax));
                };
                members.push(Member { name, value: done });
            }
        }
    }
}
