// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Structured Field Values (RFC 9651 section 4.2): Item, List and Dictionary,
//! linear (DL-063), with the RQP padding rule of Byte Sequences (DL-089).
//! Owner: WP-05 S1 T-03.
//!
//! # Parse
//!
//! [`parse_item`], [`parse_list`] and [`parse_dictionary`] follow the parsing
//! algorithms of RFC 9651 section 4.2 step by step, with all bare item types of
//! section 3.3 (Integer, Decimal, String, Token, Byte Sequence, Boolean, Date,
//! Display String). The input is the field value: for a field with several
//! field lines, the caller joins the lines with `", "` first (RFC 9651 section
//! 4.2, DL-029) and checks the RQP byte limit of the field before the parse
//! (negotiation section 7).
//!
//! One rule is stricter than the "SHOULD NOT fail" text of RFC 9651 section
//! 4.2.7 (negotiation section 4, "Inputs that the steps leave open", item 7;
//! DL-089): the padding of a Byte Sequence is absent or complete, and any other
//! `=` fails the parse. Non-zero pad bits are accepted.
//!
//! A parse failure is a [`ParseError`]: RFC 9651 section 2.2 says that the
//! caller then ignores the whole field. The error has a byte offset and a fixed
//! reason, never a byte of the field value.
//!
//! # Serialize
//!
//! [`serialize_item`], [`serialize_list`] and [`serialize_dictionary`] follow
//! RFC 9651 section 4.1. They fail with a [`SerializeError`] for a value that
//! has no serialization (an Integer out of range, a Token or a key with a
//! character outside its grammar, a String with a character outside printable
//! ASCII). An empty List or Dictionary serializes to the empty string; RFC 9651
//! section 4.1 says that the field is then not sent at all.
//!
//! # Resource rules
//!
//! - No panic on any input (workspace lints deny `unwrap`, indexing and
//!   unchecked arithmetic).
//! - Time: every parse function reads its input once, from left to right, with
//!   no backtracking. A key that repeats in a Dictionary or in Parameters is
//!   found through an ordered index, so the work is at most `O(n log n)` byte
//!   comparisons for an input of `n` bytes.
//! - Memory: the output holds at most the bytes of the input plus a fixed cost
//!   per member, so it is linear in the input. The parser allocates nothing
//!   that the input length does not bound.

mod base64;
mod parse;
mod serialize;

use std::collections::BTreeMap;
use std::fmt;

pub use parse::{parse_dictionary, parse_item, parse_list};
pub use serialize::{serialize_dictionary, serialize_item, serialize_list};

/// The largest absolute value of an Integer or a Date: 15 decimal digits
/// (RFC 9651 section 3.3.1).
pub const MAX_INTEGER: i64 = 999_999_999_999_999;

/// A Decimal (RFC 9651 section 3.3.2): at most 12 integer digits and 3
/// fractional digits, kept exactly as a count of thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Decimal {
    thousandths: i64,
}

impl Decimal {
    /// The largest absolute number of thousandths (999 999 999 999.999).
    pub const MAX_THOUSANDTHS: i64 = 999_999_999_999_999;

    /// The Decimal `thousandths / 1000`, or `None` outside the range of RFC 9651
    /// section 3.3.2.
    #[must_use]
    pub fn from_thousandths(thousandths: i64) -> Option<Self> {
        (-Self::MAX_THOUSANDTHS..=Self::MAX_THOUSANDTHS)
            .contains(&thousandths)
            .then_some(Self { thousandths })
    }

    /// The value in thousandths: `4.5` is `4500`.
    #[must_use]
    pub const fn thousandths(self) -> i64 {
        self.thousandths
    }
}

/// A bare item (RFC 9651 section 3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BareItem {
    /// An Integer, in `-MAX_INTEGER..=MAX_INTEGER` after a parse.
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
    /// A Date: seconds since the Unix epoch, in `-MAX_INTEGER..=MAX_INTEGER`
    /// after a parse.
    Date(i64),
    /// A Display String, decoded to Unicode.
    DisplayString(String),
}

/// An ordered map with unique keys: the Parameters of an Item or an Inner
/// List and the members of a Dictionary (RFC 9651 sections 3.1.2 and 3.2).
///
/// Built with [`FromIterator`]: when a key repeats, the later value replaces
/// the earlier one at the position of the first (RFC 9651 sections 4.2.2 and
/// 4.2.3.2).
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

    /// The value of `key`, compared byte for byte. Linear in the number of
    /// entries.
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

/// An Item: a bare item and its Parameters (RFC 9651 section 3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The bare item.
    pub bare_item: BareItem,
    /// The Parameters.
    pub params: Parameters,
}

impl Item {
    /// An Item without Parameters.
    #[must_use]
    pub fn new(bare_item: BareItem) -> Self {
        Self {
            bare_item,
            params: Parameters::new(),
        }
    }
}

/// An Inner List: Items and the Parameters of the list (RFC 9651 section
/// 3.1.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerList {
    /// The Items, in order.
    pub items: Vec<Item>,
    /// The Parameters of the Inner List.
    pub params: Parameters,
}

/// A member of a List or the value of a Dictionary member: an Item or an
/// Inner List (RFC 9651 sections 3.1 and 3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    /// An Item.
    Item(Item),
    /// An Inner List.
    InnerList(InnerList),
}

/// A List (RFC 9651 section 3.1).
pub type List = Vec<Member>;

/// A Dictionary (RFC 9651 section 3.2).
pub type Dictionary = Map<Member>;

/// A parse failure. RFC 9651 section 2.2: the caller ignores the whole field.
///
/// The error holds a byte offset and a fixed reason, never field content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    offset: usize,
    reason: &'static str,
}

impl ParseError {
    /// The byte offset in the field value where the parse failed.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// A short, fixed description of the failed rule (never field content).
    #[must_use]
    pub const fn reason(&self) -> &'static str {
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

impl core::error::Error for ParseError {}

/// A value that has no serialization (RFC 9651 section 4.1). Holds a fixed
/// reason, never the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerializeError {
    reason: &'static str,
}

impl SerializeError {
    /// A short, fixed description of the failed rule (never field content).
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "structured field serialize error: {}", self.reason)
    }
}

impl core::error::Error for SerializeError {}

/// `tchar` of RFC 9110 section 5.6.2.
const fn is_tchar(c: u8) -> bool {
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

/// A character after the first one of a Token (RFC 9651 sections 3.3.4 and
/// 4.2.6): `tchar`, `:` or `/`.
const fn is_token_char(c: u8) -> bool {
    is_tchar(c) || c == b':' || c == b'/'
}

/// The first character of a Token: ALPHA or `*`.
const fn is_token_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'*'
}

/// The first character of a key: lcalpha or `*` (RFC 9651 section 4.2.3.3).
const fn is_key_start(c: u8) -> bool {
    c.is_ascii_lowercase() || c == b'*'
}

/// A character after the first one of a key: lcalpha, DIGIT, `_`, `-`, `.`
/// or `*`.
const fn is_key_char(c: u8) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-' | b'.' | b'*')
}
