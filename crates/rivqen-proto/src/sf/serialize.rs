// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The serialization algorithms of RFC 9651 section 4.1.

use core::fmt::Write as _;

use super::{
    BareItem, Decimal, Dictionary, InnerList, Item, List, MAX_INTEGER, Member, Parameters,
    SerializeError, base64, is_key_char, is_key_start, is_token_char, is_token_start,
};

type Serialized = Result<(), SerializeError>;

const fn fail(reason: &'static str) -> Serialized {
    Err(SerializeError { reason })
}

/// Serialize an Item (RFC 9651 section 4.1.3).
///
/// # Errors
///
/// [`SerializeError`] when a bare item or a key has no serialization.
pub fn serialize_item(item: &Item) -> Result<String, SerializeError> {
    let mut out = String::new();
    item_into(item, &mut out)?;
    Ok(out)
}

/// Serialize a List (RFC 9651 section 4.1.1). An empty List gives the empty
/// string: the caller does not send the field (RFC 9651 section 4.1).
///
/// # Errors
///
/// [`SerializeError`] when a bare item or a key has no serialization.
pub fn serialize_list(list: &List) -> Result<String, SerializeError> {
    let mut out = String::new();
    for (i, member) in list.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        member_into(member, &mut out)?;
    }
    Ok(out)
}

/// Serialize a Dictionary (RFC 9651 section 4.1.2). An empty Dictionary gives
/// the empty string: the caller does not send the field (RFC 9651 section
/// 4.1).
///
/// # Errors
///
/// [`SerializeError`] when a bare item or a key has no serialization.
pub fn serialize_dictionary(dictionary: &Dictionary) -> Result<String, SerializeError> {
    let mut out = String::new();
    for (i, (key, member)) in dictionary.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        key_into(key, &mut out)?;
        match member {
            // Section 4.1.2 step 1.3: a member whose value is the Boolean true
            // is the key and the Parameters only.
            Member::Item(Item {
                bare_item: BareItem::Boolean(true),
                params,
            }) => params_into(params, &mut out)?,
            other => {
                out.push('=');
                member_into(other, &mut out)?;
            }
        }
    }
    Ok(out)
}

fn member_into(member: &Member, out: &mut String) -> Serialized {
    match member {
        Member::Item(item) => item_into(item, out),
        Member::InnerList(inner) => inner_list_into(inner, out),
    }
}

/// RFC 9651 section 4.1.1.1.
fn inner_list_into(inner: &InnerList, out: &mut String) -> Serialized {
    out.push('(');
    for (i, item) in inner.items.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        item_into(item, out)?;
    }
    out.push(')');
    params_into(&inner.params, out)
}

/// RFC 9651 section 4.1.3.
fn item_into(item: &Item, out: &mut String) -> Serialized {
    bare_item_into(&item.bare_item, out)?;
    params_into(&item.params, out)
}

/// RFC 9651 section 4.1.1.2.
fn params_into(params: &Parameters, out: &mut String) -> Serialized {
    for (key, value) in params.iter() {
        out.push(';');
        key_into(key, out)?;
        if *value != BareItem::Boolean(true) {
            out.push('=');
            bare_item_into(value, out)?;
        }
    }
    Ok(())
}

/// RFC 9651 section 4.1.1.3.
fn key_into(key: &str, out: &mut String) -> Serialized {
    let mut bytes = key.bytes();
    if !bytes.next().is_some_and(is_key_start) || !bytes.all(is_key_char) {
        return fail("a key has a character outside its grammar");
    }
    out.push_str(key);
    Ok(())
}

/// RFC 9651 section 4.1.3.1.
fn bare_item_into(bare: &BareItem, out: &mut String) -> Serialized {
    match bare {
        BareItem::Integer(v) => integer_into(*v, out),
        BareItem::Decimal(d) => decimal_into(*d, out),
        BareItem::String(s) => string_into(s, out),
        BareItem::Token(t) => token_into(t, out),
        BareItem::ByteSequence(b) => {
            out.push(':');
            base64::encode_into(b, out);
            out.push(':');
            Ok(())
        }
        BareItem::Boolean(b) => {
            out.push_str(if *b { "?1" } else { "?0" });
            Ok(())
        }
        BareItem::Date(v) => {
            out.push('@');
            integer_into(*v, out)
        }
        BareItem::DisplayString(s) => {
            display_string_into(s, out);
            Ok(())
        }
    }
}

/// RFC 9651 section 4.1.4.
fn integer_into(v: i64, out: &mut String) -> Serialized {
    if !(-MAX_INTEGER..=MAX_INTEGER).contains(&v) {
        return fail("an integer has at most 15 digits");
    }
    write!(out, "{v}").map_err(write_failed)
}

/// Writing to a `String` does not fail; the error exists for the type only.
const fn write_failed(_: core::fmt::Error) -> SerializeError {
    SerializeError {
        reason: "internal: write",
    }
}

/// RFC 9651 section 4.1.5. A [`Decimal`] is always in range and has at most
/// 3 fractional digits, so the range step of the RFC does not fail.
fn decimal_into(d: Decimal, out: &mut String) -> Serialized {
    let t = d.thousandths();
    if t < 0 {
        out.push('-');
    }
    let abs = t.unsigned_abs();
    let integer = abs.checked_div(1000).unwrap_or(0);
    let mut fraction = abs.checked_rem(1000).unwrap_or(0);
    let mut digits = 3_usize;
    // At least one fractional digit; trailing zeros removed.
    while digits > 1 && fraction.checked_rem(10) == Some(0) {
        fraction = fraction.checked_div(10).unwrap_or(0);
        digits = digits.saturating_sub(1);
    }
    write!(out, "{integer}.{fraction:0digits$}").map_err(write_failed)
}

/// RFC 9651 section 4.1.6.
fn string_into(s: &str, out: &mut String) -> Serialized {
    if !s.bytes().all(|c| (0x20..=0x7e).contains(&c)) {
        return fail("a string has a character outside printable ASCII");
    }
    out.push('"');
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    Ok(())
}

/// RFC 9651 section 4.1.7.
fn token_into(t: &str, out: &mut String) -> Serialized {
    let mut bytes = t.bytes();
    if !bytes.next().is_some_and(is_token_start) || !bytes.all(is_token_char) {
        return fail("a token has a character outside its grammar");
    }
    out.push_str(t);
    Ok(())
}

/// RFC 9651 section 4.1.11: `%"`, the UTF-8 bytes with `%`, `"` and bytes
/// outside printable ASCII percent-encoded in lowercase hex, then `"`.
fn display_string_into(s: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push_str("%\"");
    for c in s.bytes() {
        if c == b'%' || c == b'"' || !(0x20..=0x7e).contains(&c) {
            out.push('%');
            for nibble in [c.wrapping_shr(4), c & 0x0f] {
                out.push(char::from(
                    HEX.get(usize::from(nibble)).copied().unwrap_or(b'0'),
                ));
            }
        } else {
            out.push(char::from(c));
        }
    }
    out.push('"');
}
