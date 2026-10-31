// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Request fields of negotiation §3 (lane T-05).
//!
//! Each member of [`RqpRequest`] comes from one function below, which follows the
//! steps of its section. Common rules:
//!
//! - Field names compare ASCII case-insensitively ([`Field::is`]).
//! - Each field line value is trimmed of leading and trailing spaces and tabs
//!   (RFC 9110 §5.5); a control case can give untrimmed values.
//! - The field value of a field with several lines is the trimmed line values
//!   joined with `", "` in order (RFC 9651 §4.2; DL-029). The byte limits of
//!   §3.3, §3.4 and §3.9 apply to this value and are checked before any parse
//!   and before the joined value is allocated.
//! - A field that fails its rules is absent (negotiation §3.1, RFC 9651 §2.2),
//!   except `Rq-Have` member `b` (§3.4 steps 5 and 6).
//!
//! Every function is linear in the size of the request fields (negotiation §7).

use std::collections::BTreeSet;

use super::sf::{self, BareItem, Member, is_tchar};
use super::types::{EntityTag, Have, IfNoneMatch, RqpRequest};
use crate::model::{Field, Request};

/// `maxRqCapabilitiesBytes` (negotiation §3.3, §7).
pub const MAX_RQ_CAPABILITIES_BYTES: usize = 1024;

/// `maxRqHaveBytes` (negotiation §3.4, §7).
pub const MAX_RQ_HAVE_BYTES: usize = 4096;

/// `maxRqNonceBytes` (negotiation §3.9, §7).
pub const MAX_RQ_NONCE_BYTES: usize = 128;

/// The delta media type (negotiation §3.6): type and subtype.
const DELTA_TYPE: &str = "application";
const DELTA_SUBTYPE: &str = "vnd.rivqen.delta+json";

/// Read the RQP request fields of `request` (negotiation §3.2–§3.6, §3.9).
///
/// Contract: never fails and never panics; a field that is invalid is treated as
/// the spec says (mostly: absent). Linear in the size of the fields.
#[must_use]
pub fn parse(request: &Request) -> RqpRequest {
    let fields = request.fields.as_slice();
    let (if_none_match, base) = if_none_match(fields);
    RqpRequest {
        version_1: version_1(fields),
        capabilities: capabilities(fields),
        accepts_delta: accepts_delta(fields),
        if_none_match,
        base,
        have: have(fields),
        nonce: nonce(fields),
        has_range: has_field(fields, "range"),
        other_precondition: has_field(fields, "if-match")
            || has_field(fields, "if-unmodified-since")
            || has_field(fields, "if-range"),
    }
}

/// The value of a field line without leading and trailing spaces and tabs.
fn trim_ows(value: &str) -> &str {
    value.trim_matches(|c| c == ' ' || c == '\t')
}

/// The trimmed values of the field lines named `name`, in order.
fn lines<'a>(fields: &'a [Field], name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    fields
        .iter()
        .filter(move |f| f.is(name))
        .map(|f| trim_ows(&f.value))
}

fn has_field(fields: &[Field], name: &str) -> bool {
    fields.iter().any(|f| f.is(name))
}

/// The combined field value of `name` (DL-029), or `None` when the request has
/// no line of that name or the combined value is longer than `limit` bytes. The
/// length is computed first; a value over the limit is never built.
fn combined(fields: &[Field], name: &str, limit: usize) -> Option<String> {
    let mut count: usize = 0;
    let mut length: usize = 0;
    for value in lines(fields, name) {
        if count > 0 {
            length = length.saturating_add(2);
        }
        length = length.saturating_add(value.len());
        count = count.saturating_add(1);
    }
    if count == 0 || length > limit {
        return None;
    }
    let mut out = String::with_capacity(length);
    for (i, value) in lines(fields, name).enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(value);
    }
    Some(out)
}

/// `Rq-Version` (§3.2): an Item whose bare item is the Integer `1`; parameters
/// are ignored. No RQP byte limit: the HTTP reader bounds the field.
fn version_1(fields: &[Field]) -> bool {
    combined(fields, "rq-version", usize::MAX)
        .and_then(|value| sf::parse_item(&value).ok())
        .is_some_and(|item| item.bare_item == BareItem::Integer(1))
}

/// `Rq-Capabilities` (§3.3, steps 1 to 5): one Inner List of Tokens. The set has
/// each Token once, in the order of its first occurrence; unknown Tokens stay in
/// the set (step 5: they have no effect). Empty when the field is absent or
/// invalid.
fn capabilities(fields: &[Field]) -> Vec<String> {
    let Some(value) = combined(fields, "rq-capabilities", MAX_RQ_CAPABILITIES_BYTES) else {
        return Vec::new();
    };
    let Ok(list) = sf::parse_list(&value) else {
        return Vec::new();
    };
    let [Member::InnerList(inner)] = list.as_slice() else {
        return Vec::new();
    };
    let mut tokens = Vec::new();
    for item in &inner.items {
        match &item.bare_item {
            BareItem::Token(token) => tokens.push(token.as_str()),
            _ => return Vec::new(),
        }
    }
    let mut seen = BTreeSet::new();
    tokens
        .into_iter()
        .filter(|t| seen.insert(*t))
        .map(str::to_owned)
        .collect()
}

/// `Rq-Have` (§3.4, server steps 1, 2, 3 and 5, and "L is a multiple of 8" of
/// step 6). Steps 4 and the `8 × N` check of step 6 need the page (lane T-06).
fn have(fields: &[Field]) -> Option<Have> {
    // Step 1.
    let value = combined(fields, "rq-have", MAX_RQ_HAVE_BYTES)?;
    // Step 2. A repeated key keeps its last value (RFC 9651 §4.2.2).
    let dictionary = sf::parse_dictionary(&value).ok()?;
    // Step 3: `t` is a Token; its parameters are ignored.
    let Some(Member::Item(sf::Item {
        bare_item: BareItem::Token(template),
        ..
    })) = dictionary.get("t")
    else {
        return None;
    };
    // Steps 5 and 6 (first part): else `b` is absent.
    let digests = match dictionary.get("b") {
        Some(Member::Item(sf::Item {
            bare_item: BareItem::ByteSequence(bytes),
            ..
        })) if bytes.len() % 8 == 0 => Some(bytes.clone()),
        _ => None,
    };
    // Step 7: other members and all parameters are ignored.
    Some(Have {
        template: template.clone(),
        digests,
    })
}

/// `Rq-Nonce` (§3.9, server steps 1 to 5).
fn nonce(fields: &[Field]) -> Option<[u8; 16]> {
    // Step 1: exactly one field line of at most 128 bytes.
    let mut values = lines(fields, "rq-nonce");
    let value = values.next()?;
    if values.next().is_some() || value.len() > MAX_RQ_NONCE_BYTES {
        return None;
    }
    // Steps 2 to 4: an Item whose bare item is a Byte Sequence of 16 bytes;
    // parameters are ignored.
    match sf::parse_item(value).ok()?.bare_item {
        BareItem::ByteSequence(bytes) => <[u8; 16]>::try_from(bytes).ok(),
        _ => None,
    }
}

/// `Accept` (§3.6; DL-089 item 4): true when one media range is exactly
/// `application/vnd.rivqen.delta+json` (ASCII case-insensitive) and has no `q`
/// parameter or one valid qvalue above 0 (RFC 9110 §12.4.2, §12.5.1).
///
/// The field is a comma list (RFC 9110 §5.6.1); a comma inside a quoted string
/// does not separate. Each element is read alone: a malformed element accepts
/// nothing and does not change the other elements.
fn accepts_delta(fields: &[Field]) -> bool {
    let Some(value) = combined(fields, "accept", usize::MAX) else {
        return false;
    };
    list_elements(&value, true).any(media_range_accepts_delta)
}

/// The elements of a comma list (RFC 9110 §5.6.1), trimmed of OWS, empty ones
/// included. A comma between double quotes does not separate; an unclosed quote
/// runs to the end of the value. With `backslash_escapes`, a backslash inside quotes
/// escapes the next byte (quoted-string, RFC 9110 §5.6.4); without it, as in an
/// opaque-tag (RFC 9110 §8.8.3), a backslash is an ordinary byte.
fn list_elements(value: &str, backslash_escapes: bool) -> impl Iterator<Item = &str> {
    let mut rest = Some(value);
    std::iter::from_fn(move || {
        let text = rest?;
        let mut quoted = false;
        let mut escaped = false;
        let mut split = None;
        for (i, c) in text.bytes().enumerate() {
            if escaped {
                escaped = false;
            } else if quoted {
                match c {
                    b'\\' if backslash_escapes => escaped = true,
                    b'"' => quoted = false,
                    _ => {}
                }
            } else if c == b'"' {
                quoted = true;
            } else if c == b',' {
                split = Some(i);
                break;
            }
        }
        let element = if let Some(i) = split {
            rest = text.get(i.saturating_add(1)..);
            text.get(..i)
        } else {
            rest = None;
            Some(text)
        };
        element.map(trim_ows)
    })
}

/// One element of `Accept`: `type "/" subtype *( OWS ";" OWS [ parameter ] )`
/// (RFC 9110 §5.6.6, §12.5.1). Every parameter named `q` (any case) is the
/// weight, whatever its position (RFC 9110 §12.5.1).
fn media_range_accepts_delta(element: &str) -> bool {
    let mut cursor = Cursor::new(element);
    let media_type = cursor.token();
    if !cursor.eat(b'/') {
        return false;
    }
    let subtype = cursor.token();
    if !media_type.eq_ignore_ascii_case(DELTA_TYPE) || !subtype.eq_ignore_ascii_case(DELTA_SUBTYPE)
    {
        return false;
    }
    let mut weights: usize = 0;
    loop {
        cursor.skip_ows();
        if cursor.at_end() {
            break;
        }
        if !cursor.eat(b';') {
            return false;
        }
        cursor.skip_ows();
        // `[ parameter ]`: the parameter after a ';' is optional.
        if cursor.at_end() || cursor.peek() == Some(b';') {
            continue;
        }
        let name = cursor.token();
        if name.is_empty() || !cursor.eat(b'=') {
            return false;
        }
        let quoted = cursor.peek() == Some(b'"');
        let Some(value) = cursor.parameter_value() else {
            return false;
        };
        if name.eq_ignore_ascii_case("q") {
            weights = weights.saturating_add(1);
            // `weight = OWS ";" OWS "q=" qvalue`: a qvalue is never quoted.
            if quoted || !qvalue_above_zero(value) {
                return false;
            }
        }
    }
    // More than one weight is not the grammar of RFC 9110 §12.5.1.
    weights <= 1
}

/// `qvalue = ( "0" [ "." 0*3DIGIT ] ) / ( "1" [ "." 0*3("0") ] )` (RFC 9110
/// §12.4.2), and the value is above 0. `None`-free: an invalid value is false.
fn qvalue_above_zero(value: &str) -> bool {
    let bytes = value.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    let fraction = match rest.split_first() {
        None => &[][..],
        Some((b'.', fraction)) if fraction.len() <= 3 => fraction,
        Some(_) => return false,
    };
    match first {
        b'0' => fraction.iter().all(u8::is_ascii_digit) && fraction.iter().any(|&c| c != b'0'),
        b'1' => fraction.iter().all(|&c| c == b'0'),
        _ => false,
    }
}

/// `If-None-Match` (§3.5; RFC 9110 §8.8.3, §13.1.2; DL-089 item 3) and the delta
/// base B (§3.5 item 2).
///
/// `*` alone is [`IfNoneMatch::Any`]; a list of one or more entity-tags (empty
/// list elements ignored, RFC 9110 §5.6.1.2) is [`IfNoneMatch::Tags`]; any other
/// syntax, also a list with no tag, is [`IfNoneMatch::Invalid`]. B exists only
/// when the combined field value is exactly one entity-tag (no other list
/// element, also no empty one) whose opaque-tag is `"r1.` + 43 base64url
/// characters + `"`, strong or weak.
fn if_none_match(fields: &[Field]) -> (IfNoneMatch, Option<String>) {
    let Some(value) = combined(fields, "if-none-match", usize::MAX) else {
        return (IfNoneMatch::Absent, None);
    };
    if value == "*" {
        return (IfNoneMatch::Any, None);
    }
    let mut tags = Vec::new();
    let mut elements: usize = 0;
    for element in list_elements(&value, false) {
        elements = elements.saturating_add(1);
        if element.is_empty() {
            continue;
        }
        match entity_tag(element) {
            Some(tag) => tags.push(tag),
            None => return (IfNoneMatch::Invalid, None),
        }
    }
    if tags.is_empty() {
        return (IfNoneMatch::Invalid, None);
    }
    let base = match tags.as_slice() {
        [tag] if elements == 1 && is_page_revision(&tag.opaque) => Some(tag.opaque.clone()),
        _ => None,
    };
    (IfNoneMatch::Tags(tags), base)
}

/// `entity-tag = [ weak ] opaque-tag`, `weak = %s"W/"`, `opaque-tag = DQUOTE
/// *etagc DQUOTE`, `etagc = %x21 / %x23-7E / obs-text` (RFC 9110 §8.8.3). The
/// element is the whole text: nothing may follow the closing quote.
fn entity_tag(element: &str) -> Option<EntityTag> {
    let (weak, rest) = match element.strip_prefix("W/") {
        Some(rest) => (true, rest),
        None => (false, element),
    };
    let opaque = rest.strip_prefix('"')?.strip_suffix('"')?;
    if opaque
        .bytes()
        .all(|c| c == 0x21 || (0x23..=0x7e).contains(&c) || c >= 0x80)
    {
        Some(EntityTag {
            weak,
            opaque: opaque.to_owned(),
        })
    } else {
        None
    }
}

/// `r1.` + 43 base64url characters (negotiation §3.7, RQP-07).
fn is_page_revision(opaque: &str) -> bool {
    opaque.strip_prefix("r1.").is_some_and(|digest| {
        digest.len() == 43
            && digest
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    })
}

/// A forward-only cursor over one `Accept` element.
struct Cursor<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn at_end(&self) -> bool {
        self.pos >= self.text.len()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.pos = self.pos.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn skip_ows(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos = self.pos.saturating_add(1);
        }
    }

    /// `token = 1*tchar` (RFC 9110 §5.6.2); empty when no tchar follows.
    fn token(&mut self) -> &'a str {
        let start = self.pos;
        while self.peek().is_some_and(is_tchar) {
            self.pos = self.pos.saturating_add(1);
        }
        self.text.get(start..self.pos).unwrap_or_default()
    }

    /// `parameter-value = ( token / quoted-string )` (RFC 9110 §5.6.6, §5.6.4).
    /// For a quoted string the result is the raw text between the quotes; the
    /// caller only needs to know that it is valid.
    fn parameter_value(&mut self) -> Option<&'a str> {
        if !self.eat(b'"') {
            let token = self.token();
            return (!token.is_empty()).then_some(token);
        }
        let start = self.pos;
        loop {
            let c = self.peek()?;
            let end = self.pos;
            self.pos = self.pos.saturating_add(1);
            match c {
                b'"' => return self.text.get(start..end),
                b'\\' => {
                    // quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )
                    if !matches!(self.peek()?, b'\t' | 0x20..=0x7e | 0x80..=0xff) {
                        return None;
                    }
                    self.pos = self.pos.saturating_add(1);
                }
                // qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
                b'\t' | 0x20..=0x7e | 0x80..=0xff => {}
                _ => return None,
            }
        }
    }
}
