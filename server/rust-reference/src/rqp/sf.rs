// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Structured Field Values parser (RFC 9651 §4.2): Item, List, Dictionary;
//! linear time (negotiation §7). Lane T-05.
//!
//! The input of each parse function is the field value: for a field with several
//! field lines, the caller joins the lines with `", "` first (RFC 9651 §4.2,
//! DL-029). The functions follow the parsing algorithms of RFC 9651 §4.2 step by
//! step, with all bare item types of §3.3 (Integer, Decimal, String, Token, Byte
//! Sequence, Boolean, Date, Display String).
//!
//! One rule is stricter than the "SHOULD NOT fail" text of RFC 9651 §4.2.7
//! (negotiation §4, "Inputs that the steps leave open", item 7; DL-089): the
//! padding of a Byte Sequence is absent or complete, and any other `=` fails the
//! parse. Non-zero pad bits are accepted.
//!
//! Resource rules: every function reads its input once, from left to right, with
//! no backtracking. A key that repeats in a Dictionary or in Parameters is found
//! through an ordered index, so the work is at most `O(n log n)` key comparisons.
//! The output is never larger than a small multiple of the input.

use std::collections::BTreeMap;
use std::fmt;

use base64::Engine as _;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};

/// The largest absolute value of an Integer or a Date: 15 decimal digits
/// (RFC 9651 §3.3.1).
pub const MAX_INTEGER: i64 = 999_999_999_999_999;

/// A Decimal (RFC 9651 §3.3.2): at most 12 integer digits and 3 fractional
/// digits, kept exactly as a count of thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Decimal {
    thousandths: i64,
}

impl Decimal {
    /// The largest absolute number of thousandths (999 999 999 999.999).
    pub const MAX_THOUSANDTHS: i64 = 999_999_999_999_999;

    /// The Decimal `thousandths / 1000`, or `None` outside the range of RFC 9651
    /// §3.3.2.
    #[must_use]
    pub fn from_thousandths(thousandths: i64) -> Option<Self> {
        (-Self::MAX_THOUSANDTHS..=Self::MAX_THOUSANDTHS)
            .contains(&thousandths)
            .then_some(Self { thousandths })
    }

    /// The value in thousandths: `4.5` is `4500`.
    #[must_use]
    pub fn thousandths(self) -> i64 {
        self.thousandths
    }
}

/// A bare item (RFC 9651 §3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BareItem {
    /// An Integer, in `-MAX_INTEGER..=MAX_INTEGER`.
    Integer(i64),
    /// A Decimal.
    Decimal(Decimal),
    /// A String: printable ASCII, with the escapes removed.
    String(String),
    /// A Token.
    Token(String),
    /// A Byte Sequence, decoded.
    ByteSequence(Vec<u8>),
    /// A Boolean.
    Boolean(bool),
    /// A Date: seconds since the Unix epoch, in `-MAX_INTEGER..=MAX_INTEGER`.
    Date(i64),
    /// A Display String, decoded to Unicode.
    DisplayString(String),
}

/// An ordered map with unique keys: the Parameters of an Item or an Inner List
/// and the members of a Dictionary (RFC 9651 §3.1.2, §3.2).
///
/// Built with [`FromIterator`]: when a key repeats, the later value replaces the
/// earlier one at the position of the first (RFC 9651 §4.2.2, §4.2.3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map<V> {
    entries: Vec<(String, V)>,
}

impl<V> Default for Map<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<V> Map<V> {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The value of `key`, compared character for character. Linear in the number
    /// of entries.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&V> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// The entries in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the map has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<V> FromIterator<(String, V)> for Map<V> {
    fn from_iter<I: IntoIterator<Item = (String, V)>>(iter: I) -> Self {
        let mut entries: Vec<(String, V)> = Vec::new();
        let mut index: BTreeMap<String, usize> = BTreeMap::new();
        for (key, value) in iter {
            if let Some(slot) = index.get(&key).and_then(|&i| entries.get_mut(i)) {
                slot.1 = value;
            } else {
                index.insert(key.clone(), entries.len());
                entries.push((key, value));
            }
        }
        Self { entries }
    }
}

/// The Parameters of an Item or an Inner List.
pub type Parameters = Map<BareItem>;

/// An Item: a bare item and its Parameters (RFC 9651 §3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The bare item.
    pub bare_item: BareItem,
    /// The Parameters.
    pub params: Parameters,
}

/// An Inner List: Items and the Parameters of the list (RFC 9651 §3.1.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerList {
    /// The Items, in order.
    pub items: Vec<Item>,
    /// The Parameters of the Inner List.
    pub params: Parameters,
}

/// A member of a List or the value of a Dictionary member: an Item or an Inner
/// List (RFC 9651 §3.1, §3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    /// An Item.
    Item(Item),
    /// An Inner List.
    InnerList(InnerList),
}

/// A List (RFC 9651 §3.1).
pub type List = Vec<Member>;

/// A Dictionary (RFC 9651 §3.2).
pub type Dictionary = Map<Member>;

/// A parse failure. RFC 9651 §2.2: the caller ignores the whole field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    offset: usize,
    reason: &'static str,
}

impl ParseError {
    /// The byte offset in the field value where the parse failed.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// A short, fixed description of the failed rule (never field content).
    #[must_use]
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "structured field parse error at byte {}: {}",
            self.offset, self.reason
        )
    }
}

impl std::error::Error for ParseError {}

/// Parse `input` as an Item (RFC 9651 §4.2, field type "item").
///
/// # Errors
///
/// [`ParseError`] when `input` is not a valid Item.
pub fn parse_item(input: &str) -> Result<Item, ParseError> {
    parse_top(input, Parser::item)
}

/// Parse `input` as a List (RFC 9651 §4.2, field type "list").
///
/// # Errors
///
/// [`ParseError`] when `input` is not a valid List.
pub fn parse_list(input: &str) -> Result<List, ParseError> {
    parse_top(input, Parser::list)
}

/// Parse `input` as a Dictionary (RFC 9651 §4.2, field type "dictionary").
///
/// # Errors
///
/// [`ParseError`] when `input` is not a valid Dictionary.
pub fn parse_dictionary(input: &str) -> Result<Dictionary, ParseError> {
    parse_top(input, Parser::dictionary)
}

type Parsed<T> = Result<T, ParseError>;

/// RFC 9651 §4.2 steps 1 to 8 around the parser of one field type.
fn parse_top<'a, T>(input: &'a str, parse: fn(&mut Parser<'a>) -> Parsed<T>) -> Parsed<T> {
    // Step 1: the field value must be ASCII.
    if !input.is_ascii() {
        return Err(ParseError {
            offset: 0,
            reason: "the field value is not ASCII",
        });
    }
    let mut p = Parser { input, pos: 0 };
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
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn fail<T>(&self, reason: &'static str) -> Parsed<T> {
        Err(ParseError {
            offset: self.pos,
            reason,
        })
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
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

    /// OWS: SP and HTAB (RFC 9110 §5.6.3).
    fn skip_ows(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.advance();
        }
    }

    /// The text from `start` to the cursor.
    fn text_from(&self, start: usize) -> Parsed<&'a str> {
        match self.input.get(start..self.pos) {
            Some(text) => Ok(text),
            None => self.fail("internal: no text"),
        }
    }

    /// RFC 9651 §4.2.1.
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

    /// RFC 9651 §4.2.2.
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

    /// The separator after a member of a List or a Dictionary (RFC 9651 §4.2.1
    /// steps 2.2 to 2.6, §4.2.2 steps 2.5 to 2.9). True at the end of the input.
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

    /// RFC 9651 §4.2.1.1 and §4.2.1.2.
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

    /// RFC 9651 §4.2.3.
    fn item(&mut self) -> Parsed<Item> {
        let bare_item = self.bare_item()?;
        let params = self.parameters()?;
        Ok(Item { bare_item, params })
    }

    /// RFC 9651 §4.2.3.2.
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

    /// RFC 9651 §4.2.3.3.
    fn key(&mut self) -> Parsed<String> {
        if !matches!(self.peek(), Some(b'a'..=b'z' | b'*')) {
            return self.fail("a key starts with a lowercase letter or '*'");
        }
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'*')
        ) {
            self.advance();
        }
        Ok(self.text_from(start)?.to_owned())
    }

    /// RFC 9651 §4.2.3.1.
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
            Some(b'A'..=b'Z' | b'a'..=b'z' | b'*') => self.token().map(BareItem::Token),
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

    /// RFC 9651 §4.2.4. Fails as soon as a digit limit is passed, so the loop
    /// reads at most 17 bytes.
    fn number(&mut self) -> Parsed<Number> {
        let negative = self.eat(b'-');
        if !matches!(self.peek(), Some(b'0'..=b'9')) {
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

    /// RFC 9651 §4.2.5, after the opening DQUOTE.
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

    /// RFC 9651 §4.2.6; the caller checked the first character.
    fn token(&mut self) -> Parsed<String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if is_tchar(c) || c == b':' || c == b'/') {
            self.advance();
        }
        Ok(self.text_from(start)?.to_owned())
    }

    /// RFC 9651 §4.2.7, after the opening ':'; padding by DL-089 item 7.
    fn byte_sequence(&mut self) -> Parsed<Vec<u8>> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b':' {
                break;
            }
            self.advance();
        }
        let content = self.text_from(start)?;
        if !self.eat(b':') {
            return self.fail("byte sequence without its closing ':'");
        }
        match decode_base64(content.as_bytes()) {
            Some(bytes) => Ok(bytes),
            None => self.fail("byte sequence is not valid base64"),
        }
    }

    /// RFC 9651 §4.2.8, after the '?'.
    fn boolean(&mut self) -> Parsed<bool> {
        if self.eat(b'1') {
            Ok(true)
        } else if self.eat(b'0') {
            Ok(false)
        } else {
            self.fail("a boolean is ?0 or ?1")
        }
    }

    /// RFC 9651 §4.2.10, after the '%'.
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
                        (Some(h), Some(l)) => bytes.push(h.wrapping_mul(16) | l),
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

/// `value * 10 + digit`. Callers stop at 16 digits, so the value stays far below
/// `i64::MAX`; the saturation is never reached.
fn push_digit(value: i64, digit: i64) -> i64 {
    value.saturating_mul(10).saturating_add(digit)
}

/// `tchar` of RFC 9110 §5.6.2.
#[must_use]
pub(crate) fn is_tchar(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

/// A lowercase hex digit (RFC 9651 §4.2.10 step 6.3.2).
fn lower_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(c.wrapping_sub(b'a').wrapping_add(10)),
        _ => None,
    }
}

/// Standard base64 (RFC 4648 §4) that accepts no padding and non-zero pad bits.
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_allow_trailing_bits(true)
        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// The content of a Byte Sequence (RFC 9651 §4.2.7 steps 6 and 7; DL-089 item 7).
///
/// One pass checks the alphabet and the padding: `=` only at the end, at most two,
/// and only when the length is a multiple of 4 (then the padding is complete).
/// Without padding, the decoder accepts every length but `4k + 1`.
fn decode_base64(content: &[u8]) -> Option<Vec<u8>> {
    let mut padding: usize = 0;
    for &c in content {
        if c == b'=' {
            padding = padding.saturating_add(1);
        } else if padding > 0 || !(c.is_ascii_alphanumeric() || c == b'+' || c == b'/') {
            return None;
        }
    }
    if padding > 2 || (padding > 0 && content.len() % 4 != 0) {
        return None;
    }
    BASE64.decode(content).ok()
}
