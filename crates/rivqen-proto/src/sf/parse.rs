// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The parsing algorithms of RFC 9651 section 4.2.
//!
//! The parser is a cursor that only moves forward. Each loop consumes at
//! least one byte per turn or ends, so the work is linear in the input; the
//! only other cost is the ordered index of [`super::Map`] for repeated keys.

use super::{
    BareItem, Decimal, Dictionary, InnerList, Item, List, Member, Parameters, ParseError, base64,
    is_key_char, is_key_start, is_token_char, is_token_start,
};

/// Parse `input` as an Item (RFC 9651 section 4.2, field type "item").
///
/// # Errors
///
/// [`ParseError`] when `input` is not a valid Item.
pub fn parse_item(input: &str) -> Result<Item, ParseError> {
    parse_top(input, Parser::item)
}

/// Parse `input` as a List (RFC 9651 section 4.2, field type "list").
///
/// # Errors
///
/// [`ParseError`] when `input` is not a valid List.
pub fn parse_list(input: &str) -> Result<List, ParseError> {
    parse_top(input, Parser::list)
}

/// Parse `input` as a Dictionary (RFC 9651 section 4.2, field type
/// "dictionary").
///
/// # Errors
///
/// [`ParseError`] when `input` is not a valid Dictionary.
pub fn parse_dictionary(input: &str) -> Result<Dictionary, ParseError> {
    parse_top(input, Parser::dictionary)
}

type Parsed<T> = Result<T, ParseError>;

/// RFC 9651 section 4.2 steps 1 to 8 around the parser of one field type.
fn parse_top<'a, T>(input: &'a str, parse: fn(&mut Parser<'a>) -> Parsed<T>) -> Parsed<T> {
    // Step 1: the field value must be ASCII.
    if let Some(offset) = input.bytes().position(|c| !c.is_ascii()) {
        return Err(ParseError {
            offset,
            reason: "the field value is not ASCII",
        });
    }
    let mut p = Parser {
        input: input.as_bytes(),
        pos: 0,
    };
    p.skip_sp();
    let output = parse(&mut p)?;
    p.skip_sp();
    if p.at_end() {
        Ok(output)
    } else {
        p.fail("characters after the value")
    }
}

/// A Number before the type check of its caller.
enum Number {
    Integer(i64),
    Decimal(Decimal),
}

/// A cursor over an ASCII field value. Every method moves forward only.
struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn fail<T>(&self, reason: &'static str) -> Parsed<T> {
        Err(ParseError {
            offset: self.pos,
            reason,
        })
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    fn next_byte(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.advance();
        Some(c)
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn skip_sp(&mut self) {
        while self.eat(b' ') {}
    }

    /// OWS: SP and HTAB (RFC 9110 section 5.6.3).
    fn skip_ows(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.advance();
        }
    }

    /// The bytes from `start` to the cursor, as text (the input is ASCII).
    fn text_from(&self, start: usize) -> Parsed<String> {
        match self
            .input
            .get(start..self.pos)
            .and_then(|b| core::str::from_utf8(b).ok())
        {
            Some(text) => Ok(text.to_owned()),
            None => self.fail("internal: no text"),
        }
    }

    /// RFC 9651 section 4.2.1.
    fn list(&mut self) -> Parsed<List> {
        let mut members = Vec::new();
        while !self.at_end() {
            members.push(self.item_or_inner_list()?);
            if self.end_of_member()? {
                break;
            }
        }
        Ok(members)
    }

    /// RFC 9651 section 4.2.2.
    fn dictionary(&mut self) -> Parsed<Dictionary> {
        let mut members = Vec::new();
        while !self.at_end() {
            let key = self.key()?;
            let member = if self.eat(b'=') {
                self.item_or_inner_list()?
            } else {
                Member::Item(Item {
                    bare_item: BareItem::Boolean(true),
                    params: self.parameters()?,
                })
            };
            members.push((key, member));
            if self.end_of_member()? {
                break;
            }
        }
        Ok(members.into_iter().collect())
    }

    /// The separator after a member of a List or a Dictionary (RFC 9651
    /// section 4.2.1 steps 2.2 to 2.6, section 4.2.2 steps 2.5 to 2.9). True at
    /// the end of the input.
    fn end_of_member(&mut self) -> Parsed<bool> {
        self.skip_ows();
        if self.at_end() {
            return Ok(true);
        }
        if !self.eat(b',') {
            return self.fail("expected a comma after a member");
        }
        self.skip_ows();
        if self.at_end() {
            return self.fail("trailing comma");
        }
        Ok(false)
    }

    /// RFC 9651 sections 4.2.1.1 and 4.2.1.2.
    fn item_or_inner_list(&mut self) -> Parsed<Member> {
        if !self.eat(b'(') {
            return Ok(Member::Item(self.item()?));
        }
        let mut items = Vec::new();
        while !self.at_end() {
            self.skip_sp();
            if self.eat(b')') {
                let params = self.parameters()?;
                return Ok(Member::InnerList(InnerList { items, params }));
            }
            items.push(self.item()?);
            if !matches!(self.peek(), Some(b' ' | b')')) {
                return self.fail("expected a space or ')' in an inner list");
            }
        }
        self.fail("inner list without ')'")
    }

    /// RFC 9651 section 4.2.3.
    fn item(&mut self) -> Parsed<Item> {
        let bare_item = self.bare_item()?;
        let params = self.parameters()?;
        Ok(Item { bare_item, params })
    }

    /// RFC 9651 section 4.2.3.2.
    fn parameters(&mut self) -> Parsed<Parameters> {
        let mut entries = Vec::new();
        while self.eat(b';') {
            self.skip_sp();
            let key = self.key()?;
            let value = if self.eat(b'=') {
                self.bare_item()?
            } else {
                BareItem::Boolean(true)
            };
            entries.push((key, value));
        }
        Ok(entries.into_iter().collect())
    }

    /// RFC 9651 section 4.2.3.3.
    fn key(&mut self) -> Parsed<String> {
        if !self.peek().is_some_and(is_key_start) {
            return self.fail("a key starts with a lowercase letter or '*'");
        }
        let start = self.pos;
        while self.peek().is_some_and(is_key_char) {
            self.advance();
        }
        self.text_from(start)
    }

    /// RFC 9651 section 4.2.3.1.
    fn bare_item(&mut self) -> Parsed<BareItem> {
        match self.peek() {
            Some(b'-' | b'0'..=b'9') => Ok(match self.number()? {
                Number::Integer(v) => BareItem::Integer(v),
                Number::Decimal(v) => BareItem::Decimal(v),
            }),
            Some(b'"') => {
                self.advance();
                self.string().map(BareItem::String)
            }
            Some(c) if is_token_start(c) => self.token().map(BareItem::Token),
            Some(b':') => {
                self.advance();
                self.byte_sequence().map(BareItem::ByteSequence)
            }
            Some(b'?') => {
                self.advance();
                self.boolean().map(BareItem::Boolean)
            }
            Some(b'@') => {
                self.advance();
                match self.number()? {
                    Number::Integer(v) => Ok(BareItem::Date(v)),
                    Number::Decimal(_) => self.fail("a date is an integer"),
                }
            }
            Some(b'%') => {
                self.advance();
                self.display_string().map(BareItem::DisplayString)
            }
            _ => self.fail("not the start of a bare item"),
        }
    }

    /// RFC 9651 section 4.2.4. Fails as soon as a digit limit is passed, so
    /// the loop reads at most 17 bytes.
    fn number(&mut self) -> Parsed<Number> {
        let negative = self.eat(b'-');
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return self.fail("expected a digit");
        }
        let mut integer: i64 = 0;
        let mut integer_digits: u32 = 0;
        let mut fraction: i64 = 0;
        let mut fraction_digits: u32 = 0;
        let mut decimal = false;
        loop {
            match self.peek() {
                Some(c @ b'0'..=b'9') => {
                    self.advance();
                    let digit = i64::from(c.wrapping_sub(b'0'));
                    if decimal {
                        fraction = push_digit(fraction, digit);
                        fraction_digits = fraction_digits.saturating_add(1);
                        if fraction_digits > 3 {
                            return self.fail("a decimal has at most 3 fractional digits");
                        }
                    } else {
                        integer = push_digit(integer, digit);
                        integer_digits = integer_digits.saturating_add(1);
                        if integer_digits > 15 {
                            return self.fail("an integer has at most 15 digits");
                        }
                    }
                }
                Some(b'.') if !decimal => {
                    if integer_digits > 12 {
                        return self.fail("a decimal has at most 12 integer digits");
                    }
                    self.advance();
                    decimal = true;
                }
                _ => break,
            }
        }
        let sign: i64 = if negative { -1 } else { 1 };
        if !decimal {
            return Ok(Number::Integer(integer.saturating_mul(sign)));
        }
        if fraction_digits == 0 {
            return self.fail("a decimal ends with a digit");
        }
        let scale = 10_i64.saturating_pow(3_u32.saturating_sub(fraction_digits));
        let thousandths = integer
            .saturating_mul(1000)
            .saturating_add(fraction.saturating_mul(scale))
            .saturating_mul(sign);
        match Decimal::from_thousandths(thousandths) {
            Some(d) => Ok(Number::Decimal(d)),
            None => self.fail("internal: decimal out of range"),
        }
    }

    /// RFC 9651 section 4.2.5, after the opening DQUOTE.
    fn string(&mut self) -> Parsed<String> {
        let mut out = String::new();
        loop {
            match self.next_byte() {
                None => return self.fail("string without its closing quote"),
                Some(b'\\') => match self.next_byte() {
                    Some(c @ (b'"' | b'\\')) => out.push(char::from(c)),
                    _ => return self.fail("a string escapes only '\"' and '\\'"),
                },
                Some(b'"') => return Ok(out),
                Some(c @ 0x20..=0x7e) => out.push(char::from(c)),
                Some(_) => return self.fail("a control character in a string"),
            }
        }
    }

    /// RFC 9651 section 4.2.6; the caller checked the first character.
    fn token(&mut self) -> Parsed<String> {
        let start = self.pos;
        self.advance();
        while self.peek().is_some_and(is_token_char) {
            self.advance();
        }
        self.text_from(start)
    }

    /// RFC 9651 section 4.2.7, after the opening ':'; padding by DL-089
    /// item 7.
    fn byte_sequence(&mut self) -> Parsed<Vec<u8>> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c != b':') {
            self.advance();
        }
        let content = self.input.get(start..self.pos).unwrap_or_default();
        if !self.eat(b':') {
            return self.fail("byte sequence without its closing ':'");
        }
        match base64::decode(content) {
            Some(bytes) => Ok(bytes),
            None => self.fail("byte sequence is not valid base64"),
        }
    }

    /// RFC 9651 section 4.2.8, after the '?'.
    fn boolean(&mut self) -> Parsed<bool> {
        if self.eat(b'1') {
            Ok(true)
        } else if self.eat(b'0') {
            Ok(false)
        } else {
            self.fail("a boolean is ?0 or ?1")
        }
    }

    /// RFC 9651 section 4.2.10, after the '%'.
    fn display_string(&mut self) -> Parsed<String> {
        if !self.eat(b'"') {
            return self.fail("a display string starts with %\"");
        }
        let mut bytes = Vec::new();
        loop {
            match self.next_byte() {
                None => return self.fail("display string without its closing quote"),
                Some(b'%') => {
                    let high = self.next_byte().and_then(lower_hex);
                    let low = self.next_byte().and_then(lower_hex);
                    match (high, low) {
                        (Some(h), Some(l)) => bytes.push(h.wrapping_shl(4) | l),
                        _ => {
                            return self
                                .fail("'%' in a display string needs 2 lowercase hex digits");
                        }
                    }
                }
                Some(b'"') => {
                    return match String::from_utf8(bytes) {
                        Ok(text) => Ok(text),
                        Err(_) => self.fail("display string is not UTF-8"),
                    };
                }
                Some(c @ 0x20..=0x7e) => bytes.push(c),
                Some(_) => return self.fail("a control character in a display string"),
            }
        }
    }
}

/// `value * 10 + digit`. Callers stop at 16 digits, so the value stays far
/// below `i64::MAX`; the saturation is never reached.
const fn push_digit(value: i64, digit: i64) -> i64 {
    value.saturating_mul(10).saturating_add(digit)
}

/// A lowercase hex digit (RFC 9651 section 4.2.10 step 6.3.2).
const fn lower_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(c.wrapping_sub(b'a').wrapping_add(10)),
        _ => None,
    }
}
