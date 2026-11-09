// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The pull reader: a JSON text as a stream of events, with the rules of
//! manifest-patch section 3.2. No recursion: an explicit stack of at most
//! `max_depth` frames.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;

use super::{JsonError, JsonRule};

/// One step of a JSON text.
///
/// Strings and member names are decoded (escapes resolved). They borrow from
/// the text when it has no escape. [`fmt::Debug`] writes their length only,
/// and the kind of a number or boolean without its value: they can be page
/// content.
#[derive(Clone, PartialEq, Eq)]
pub enum Event<'a> {
    /// `{`
    StartObject,
    /// `}`
    EndObject,
    /// `[`
    StartArray,
    /// `]`
    EndArray,
    /// A member name; the value follows. Unique in its object.
    Name(Cow<'a, str>),
    /// A string value.
    String(Cow<'a, str>),
    /// An integer value (digits only, at most [`u64::MAX`]).
    Integer(u64),
    /// `true` or `false`.
    Bool(bool),
    /// `null`.
    Null,
}

impl fmt::Debug for Event<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StartObject => f.write_str("StartObject"),
            Self::EndObject => f.write_str("EndObject"),
            Self::StartArray => f.write_str("StartArray"),
            Self::EndArray => f.write_str("EndArray"),
            Self::Name(name) => write!(f, "Name(len={})", name.len()),
            Self::String(text) => write!(f, "String(len={})", text.len()),
            Self::Integer(_) => f.write_str("Integer(..)"),
            Self::Bool(_) => f.write_str("Bool(..)"),
            Self::Null => f.write_str("Null"),
        }
    }
}

/// An open container.
enum Frame<'a> {
    Array,
    /// The member names read so far (rule 2).
    Object(BTreeSet<Cow<'a, str>>),
}

/// What the reader accepts next.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// A value (the top-level value, after `,` in an array, after `:`).
    Value,
    /// A value or `]`.
    ArrayFirst,
    /// `,` or `]`.
    ArrayNext,
    /// A member name or `}`.
    ObjectFirst,
    /// `,` or `}`.
    ObjectNext,
    /// White space up to the end of the text.
    End,
    /// The text is complete.
    Done,
}

/// Strict JSON pull reader (manifest-patch section 3.2).
///
/// Call [`Reader::next_event`] until it gives `Ok(None)`: then the text was
/// one complete JSON value. After an error the reader gives the same error
/// again.
pub struct Reader<'a> {
    text: &'a str,
    pos: usize,
    max_depth: usize,
    stack: Vec<Frame<'a>>,
    expect: Expect,
    failed: Option<JsonError>,
}

impl fmt::Debug for Reader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Reader(len={}, pos={}, depth={})",
            self.text.len(),
            self.pos,
            self.stack.len()
        )
    }
}

const fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

impl<'a> Reader<'a> {
    /// A reader of `text` with the depth limit `max_depth` (rule 5).
    ///
    /// # Errors
    ///
    /// [`JsonRule::Encoding`] when `text` is not UTF-8, [`JsonRule::Bom`]
    /// when it starts with a byte order mark (rule 1). Both are checked over
    /// the whole text before the first event.
    pub fn new(text: &'a [u8], max_depth: usize) -> Result<Self, JsonError> {
        let text = core::str::from_utf8(text)
            .map_err(|e| JsonError::new(JsonRule::Encoding, e.valid_up_to()))?;
        if text.starts_with('\u{feff}') {
            return Err(JsonError::new(JsonRule::Bom, 0));
        }
        Ok(Self {
            text,
            pos: 0,
            max_depth,
            stack: Vec::new(),
            expect: Expect::Value,
            failed: None,
        })
    }

    /// The next event, or `None` when the text is complete.
    ///
    /// # Errors
    ///
    /// A [`JsonError`] for the first break of a rule.
    pub fn next_event(&mut self) -> Result<Option<Event<'a>>, JsonError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        let result = self.step();
        if let Err(error) = result {
            self.failed = Some(error);
        }
        result
    }

    /// Check that the text ends here: only white space follows the value
    /// that the caller has read.
    ///
    /// # Errors
    ///
    /// [`JsonRule::Syntax`] when more JSON follows or the value is not
    /// complete, or the earlier error of the reader.
    pub fn finish(&mut self) -> Result<(), JsonError> {
        match self.next_event()? {
            None => Ok(()),
            Some(_) => Err(self.fail(JsonRule::Syntax)),
        }
    }

    /// An error at the current offset; the reader keeps it.
    pub(crate) fn fail(&mut self, rule: JsonRule) -> JsonError {
        let error = JsonError::new(rule, self.pos);
        self.failed = Some(error);
        error
    }

    fn err(&self, rule: JsonRule) -> JsonError {
        JsonError::new(rule, self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn bump(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(is_ws) {
            self.bump();
        }
    }

    fn step(&mut self) -> Result<Option<Event<'a>>, JsonError> {
        self.skip_ws();
        match self.expect {
            Expect::Value => self.value().map(Some),
            Expect::ArrayFirst => {
                if self.peek() == Some(b']') {
                    self.close().map(Some)
                } else {
                    self.value().map(Some)
                }
            }
            Expect::ArrayNext => match self.peek() {
                Some(b',') => {
                    self.bump();
                    self.skip_ws();
                    self.value().map(Some)
                }
                Some(b']') => self.close().map(Some),
                _ => Err(self.err(JsonRule::Syntax)),
            },
            Expect::ObjectFirst => {
                if self.peek() == Some(b'}') {
                    self.close().map(Some)
                } else {
                    self.name().map(Some)
                }
            }
            Expect::ObjectNext => match self.peek() {
                Some(b',') => {
                    self.bump();
                    self.skip_ws();
                    self.name().map(Some)
                }
                Some(b'}') => self.close().map(Some),
                _ => Err(self.err(JsonRule::Syntax)),
            },
            Expect::End => {
                if self.pos == self.text.len() {
                    self.expect = Expect::Done;
                    Ok(None)
                } else {
                    Err(self.err(JsonRule::Syntax))
                }
            }
            Expect::Done => Ok(None),
        }
    }

    /// The state after a complete value.
    fn after_value(&mut self) {
        self.expect = match self.stack.last() {
            None => Expect::End,
            Some(Frame::Array) => Expect::ArrayNext,
            Some(Frame::Object(_)) => Expect::ObjectNext,
        };
    }

    fn open(&mut self, frame: Frame<'a>) -> Result<(), JsonError> {
        // The new container has depth stack.len() + 1 (the top level is 1).
        if self.stack.len() >= self.max_depth {
            return Err(self.err(JsonRule::Depth));
        }
        self.stack.push(frame);
        self.bump();
        Ok(())
    }

    /// `]` or `}` of the open container. The state machine calls it only for
    /// the bracket of the container on top of the stack.
    fn close(&mut self) -> Result<Event<'a>, JsonError> {
        let event = match self.stack.pop() {
            Some(Frame::Array) => Event::EndArray,
            Some(Frame::Object(_)) => Event::EndObject,
            None => return Err(self.err(JsonRule::Syntax)),
        };
        self.bump();
        self.after_value();
        Ok(event)
    }

    fn value(&mut self) -> Result<Event<'a>, JsonError> {
        match self.peek() {
            Some(b'{') => {
                self.open(Frame::Object(BTreeSet::new()))?;
                self.expect = Expect::ObjectFirst;
                Ok(Event::StartObject)
            }
            Some(b'[') => {
                self.open(Frame::Array)?;
                self.expect = Expect::ArrayFirst;
                Ok(Event::StartArray)
            }
            Some(b'"') => {
                let text = self.string()?;
                self.after_value();
                Ok(Event::String(text))
            }
            Some(b'-' | b'0'..=b'9') => {
                let n = self.number()?;
                self.after_value();
                Ok(Event::Integer(n))
            }
            Some(b't') => self.literal("true", Event::Bool(true)),
            Some(b'f') => self.literal("false", Event::Bool(false)),
            Some(b'n') => self.literal("null", Event::Null),
            _ => Err(self.err(JsonRule::Syntax)),
        }
    }

    fn literal(&mut self, word: &str, event: Event<'a>) -> Result<Event<'a>, JsonError> {
        let found = self
            .text
            .as_bytes()
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(word.as_bytes()));
        if !found {
            return Err(self.err(JsonRule::Syntax));
        }
        self.pos = self.pos.saturating_add(word.len());
        self.after_value();
        Ok(event)
    }

    /// A member name, the white space after it and `:` (rule 2 on the name).
    fn name(&mut self) -> Result<Event<'a>, JsonError> {
        if self.peek() != Some(b'"') {
            return Err(self.err(JsonRule::Syntax));
        }
        let at = self.pos;
        let name = self.string()?;
        let Some(Frame::Object(names)) = self.stack.last_mut() else {
            return Err(JsonError::new(JsonRule::Syntax, at));
        };
        if !names.insert(name.clone()) {
            return Err(JsonError::new(JsonRule::Duplicate, at));
        }
        self.skip_ws();
        if self.peek() != Some(b':') {
            return Err(self.err(JsonRule::Syntax));
        }
        self.bump();
        self.expect = Expect::Value;
        Ok(Event::Name(name))
    }

    /// RFC 8259 number grammar; rule 4 on the result.
    fn number(&mut self) -> Result<u64, JsonError> {
        let bytes = self.text.as_bytes();
        let start = self.pos;
        let mut i = start;
        let digit_at = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
        let skip_digits = |mut i: usize| {
            while digit_at(i) {
                i = i.saturating_add(1);
            }
            i
        };
        let mut digits_only = true;
        if bytes.get(i) == Some(&b'-') {
            digits_only = false;
            i = i.saturating_add(1);
        }
        match bytes.get(i) {
            Some(b'0') => i = i.saturating_add(1),
            Some(b'1'..=b'9') => i = skip_digits(i.saturating_add(1)),
            _ => return Err(JsonError::new(JsonRule::Syntax, i)),
        }
        if bytes.get(i) == Some(&b'.') {
            digits_only = false;
            i = i.saturating_add(1);
            if !digit_at(i) {
                return Err(JsonError::new(JsonRule::Syntax, i));
            }
            i = skip_digits(i);
        }
        if matches!(bytes.get(i), Some(b'e' | b'E')) {
            digits_only = false;
            i = i.saturating_add(1);
            if matches!(bytes.get(i), Some(b'+' | b'-')) {
                i = i.saturating_add(1);
            }
            if !digit_at(i) {
                return Err(JsonError::new(JsonRule::Syntax, i));
            }
            i = skip_digits(i);
        }
        if !digits_only {
            return Err(JsonError::new(JsonRule::Integer, start));
        }
        let n = self
            .text
            .get(start..i)
            .and_then(|digits| digits.parse::<u64>().ok())
            .ok_or(JsonError::new(JsonRule::Integer, start))?;
        self.pos = i;
        Ok(n)
    }

    /// A string at `"`: decoded, borrowed when it has no escape.
    fn string(&mut self) -> Result<Cow<'a, str>, JsonError> {
        let text = self.text;
        let bytes = text.as_bytes();
        let start = self.pos.saturating_add(1);
        let mut i = start;
        // Fast path: no escape.
        loop {
            match bytes.get(i) {
                Some(b'"') => {
                    let s = text
                        .get(start..i)
                        .ok_or(JsonError::new(JsonRule::Syntax, i))?;
                    self.pos = i.saturating_add(1);
                    return Ok(Cow::Borrowed(s));
                }
                Some(b'\\') => break,
                Some(&c) if c >= 0x20 => i = i.saturating_add(1),
                _ => return Err(JsonError::new(JsonRule::Syntax, i)),
            }
        }
        let mut out = String::from(
            text.get(start..i)
                .ok_or(JsonError::new(JsonRule::Syntax, i))?,
        );
        loop {
            match bytes.get(i) {
                Some(b'"') => {
                    self.pos = i.saturating_add(1);
                    return Ok(Cow::Owned(out));
                }
                Some(b'\\') => i = escape(text, i, &mut out)?,
                Some(&c) if c >= 0x20 => {
                    // A run of plain characters; it ends at an ASCII byte or at
                    // the end, so both ends are character boundaries.
                    let mut j = i.saturating_add(1);
                    while bytes
                        .get(j)
                        .is_some_and(|&b| b >= 0x20 && b != b'"' && b != b'\\')
                    {
                        j = j.saturating_add(1);
                    }
                    out.push_str(text.get(i..j).ok_or(JsonError::new(JsonRule::Syntax, i))?);
                    i = j;
                }
                _ => return Err(JsonError::new(JsonRule::Syntax, i)),
            }
        }
    }
}

/// One escape at `at` (the backslash): its character goes to `out`. Returns
/// the offset after the escape (rule 3 for `\u` escapes of surrogates).
fn escape(text: &str, at: usize, out: &mut String) -> Result<usize, JsonError> {
    let bytes = text.as_bytes();
    let kind = at.saturating_add(1);
    let simple = match bytes.get(kind) {
        Some(b'"') => '"',
        Some(b'\\') => '\\',
        Some(b'/') => '/',
        Some(b'b') => '\u{8}',
        Some(b'f') => '\u{c}',
        Some(b'n') => '\n',
        Some(b'r') => '\r',
        Some(b't') => '\t',
        Some(b'u') => return unicode_escape(text, at, out),
        _ => return Err(JsonError::new(JsonRule::Syntax, at)),
    };
    out.push(simple);
    Ok(at.saturating_add(2))
}

/// A `\uXXXX` escape at `at`, with its low surrogate escape when it is a high
/// surrogate.
fn unicode_escape(text: &str, at: usize, out: &mut String) -> Result<usize, JsonError> {
    let bytes = text.as_bytes();
    let unit = hex4(text, at.saturating_add(2))?;
    let next = at.saturating_add(6);
    if (0xdc00..=0xdfff).contains(&unit) {
        return Err(JsonError::new(JsonRule::Surrogate, at));
    }
    if !(0xd800..=0xdbff).contains(&unit) {
        let c = char::from_u32(u32::from(unit)).ok_or(JsonError::new(JsonRule::Syntax, at))?;
        out.push(c);
        return Ok(next);
    }
    if bytes.get(next) != Some(&b'\\') || bytes.get(next.saturating_add(1)) != Some(&b'u') {
        return Err(JsonError::new(JsonRule::Surrogate, at));
    }
    let low = hex4(text, next.saturating_add(2))?;
    if !(0xdc00..=0xdfff).contains(&low) {
        return Err(JsonError::new(JsonRule::Surrogate, at));
    }
    let c = char::decode_utf16([unit, low])
        .next()
        .and_then(Result::ok)
        .ok_or(JsonError::new(JsonRule::Surrogate, at))?;
    out.push(c);
    Ok(next.saturating_add(6))
}

/// Four hex digits at `at`.
fn hex4(text: &str, at: usize) -> Result<u16, JsonError> {
    let syntax = JsonError::new(JsonRule::Syntax, at);
    // Exactly 4 bytes, or `None` at the end of the text or inside a character.
    let digits = text.get(at..at.saturating_add(4)).ok_or(syntax)?;
    // from_str_radix alone would accept a leading `+`.
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(syntax);
    }
    u16::from_str_radix(digits, 16).map_err(|_| syntax)
}
