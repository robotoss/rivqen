// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Header fields of the application response A (negotiation §4): the removal of
//! the `Rq-*` fields and of the `Rq-*` members of `Vary` (DL-030, DL-063), the
//! scope test of the media type (step 1), the content coding (DL-089 item 5)
//! and the charset rule (step 2, H-62, H-66).

use crate::model::Field;

/// The framing fields that the HTTP writer owns (`model::Response`).
const FRAMING: [&str; 3] = ["content-length", "transfer-encoding", "connection"];

/// True when `name` starts with `Rq-` (ASCII case-insensitive).
fn is_rq(name: &str) -> bool {
    name.as_bytes()
        .get(..3)
        .is_some_and(|p| p.eq_ignore_ascii_case(b"rq-"))
}

/// Spaces and tabs (OWS, RFC 9110 §5.6.3).
fn trim_ows(value: &str) -> &str {
    value.trim_matches(|c| c == ' ' || c == '\t')
}

/// A without its `Rq-*` fields (DL-030), without the `Rq-*` members of each
/// `Vary` line (a line with no member left goes; DL-063 L2), and without the
/// framing fields. A `Vary` line without an `Rq-*` member stays byte for byte.
#[must_use]
pub fn clean(fields: &[Field]) -> Vec<Field> {
    let mut out = Vec::with_capacity(fields.len());
    for field in fields {
        if is_rq(&field.name) || FRAMING.iter().any(|n| field.is(n)) {
            continue;
        }
        if field.is("vary") && field.value.split(',').any(|m| is_rq(trim_ows(m))) {
            let kept: Vec<&str> = field
                .value
                .split(',')
                .map(trim_ows)
                .filter(|m| !m.is_empty() && !is_rq(m))
                .collect();
            if !kept.is_empty() {
                out.push(Field::new(&field.name, &kept.join(", ")));
            }
            continue;
        }
        out.push(field.clone());
    }
    out
}

/// True when A has a content coding other than `identity` (DL-089 item 5): a
/// member of a `Content-Encoding` line that is not `identity` (ASCII
/// case-insensitive). Empty list members do not count.
#[must_use]
pub fn has_content_coding(fields: &[Field]) -> bool {
    fields
        .iter()
        .filter(|f| f.is("content-encoding"))
        .flat_map(|f| f.value.split(','))
        .map(trim_ows)
        .any(|m| !m.is_empty() && !m.eq_ignore_ascii_case("identity"))
}

/// The `Content-Type` of A for steps 1 and 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    /// The media type is not `text/html`, or A has no `Content-Type` or one
    /// that does not parse (step 1: out of scope).
    NotHtml,
    /// `text/html`, but the charset rule does not give UTF-8 or no charset
    /// (step 2: invalid for RQP). Also more than one field line or media type.
    NotUtf8,
    /// `text/html` with no charset or a UTF-8 label.
    Utf8,
}

/// The labels of the encoding UTF-8 (WHATWG Encoding §4.2; negotiation §4,
/// charset rule item 3).
const UTF8_LABELS: [&str; 6] = [
    "unicode-1-1-utf-8",
    "unicode11utf8",
    "unicode20utf8",
    "utf-8",
    "utf8",
    "x-unicode20utf8",
];

/// Steps 1 and 2 on the `Content-Type` of A (negotiation §4, charset rule).
#[must_use]
pub fn content_type(fields: &[Field]) -> ContentType {
    let mut lines = fields.iter().filter(|f| f.is("content-type"));
    let Some(line) = lines.next() else {
        return ContentType::NotHtml;
    };
    let Some(mime) = parse_mime(&line.value) else {
        return ContentType::NotHtml;
    };
    if mime.essence != "text/html" {
        return ContentType::NotHtml;
    }
    // Item 1: exactly one field line with exactly one media type.
    if lines.next().is_some() || has_comma_outside_quotes(&line.value) {
        return ContentType::NotUtf8;
    }
    match mime.charset {
        None => ContentType::Utf8,
        Some(label) => {
            let label = label.trim_matches(|c| matches!(c, '\t' | '\n' | '\x0C' | '\r' | ' '));
            if UTF8_LABELS.iter().any(|l| l.eq_ignore_ascii_case(label)) {
                ContentType::Utf8
            } else {
                ContentType::NotUtf8
            }
        }
    }
}

/// A comma that is not inside a quoted string (RFC 9110 §5.6.4: a backslash in
/// a quoted string escapes the next byte; an unclosed quoted string runs to the
/// end of the value).
fn has_comma_outside_quotes(value: &str) -> bool {
    let mut quoted = false;
    let mut escaped = false;
    for b in value.bytes() {
        if quoted {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                quoted = false;
            }
        } else if b == b'"' {
            quoted = true;
        } else if b == b',' {
            return true;
        }
    }
    false
}

/// The parts of a MIME type that the rules read.
#[derive(Debug, PartialEq, Eq)]
struct Mime {
    /// `type/subtype`, ASCII lower case.
    essence: String,
    /// The value of the first valid parameter `charset`.
    charset: Option<String>,
}

fn is_http_ws(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\t' | ' ')
}

fn is_token(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

/// HTTP quoted-string token code points (WHATWG MIME Sniffing §2.1).
fn is_quoted_token(c: char) -> bool {
    matches!(c, '\t' | ' '..='~' | '\u{80}'..='\u{FF}')
}

/// A forward cursor over the code points of a value.
struct Cursor {
    chars: Vec<char>,
    pos: usize,
}

impl Cursor {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    fn done(&self) -> bool {
        self.pos >= self.chars.len()
    }

    /// Collect the code points while `keep` holds.
    fn collect(&mut self, keep: impl Fn(char) -> bool) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if !keep(c) {
                break;
            }
            out.push(c);
            self.bump();
        }
        out
    }

    /// "Collect an HTTP quoted string" with the extract-value flag (WHATWG
    /// Fetch §2.2.6). The cursor is at the `"`.
    fn quoted(&mut self) -> String {
        let mut value = String::new();
        self.bump();
        loop {
            value.push_str(&self.collect(|c| c != '"' && c != '\\'));
            let Some(c) = self.peek() else { break };
            self.bump();
            if c == '\\' {
                let Some(escaped) = self.peek() else {
                    value.push('\\');
                    break;
                };
                value.push(escaped);
                self.bump();
            } else {
                break;
            }
        }
        value
    }
}

/// "Parse a MIME type" (WHATWG MIME Sniffing §4.4), keeping the essence and the
/// first valid `charset` parameter. `None` is failure.
fn parse_mime(input: &str) -> Option<Mime> {
    let mut c = Cursor {
        chars: input.trim_matches(is_http_ws).chars().collect(),
        pos: 0,
    };
    let kind = c.collect(|ch| ch != '/');
    if kind.is_empty() || !kind.chars().all(is_token) || c.done() {
        return None;
    }
    c.bump();
    let subtype = c.collect(|ch| ch != ';');
    let subtype = subtype.trim_end_matches(is_http_ws);
    if subtype.is_empty() || !subtype.chars().all(is_token) {
        return None;
    }
    let essence = format!(
        "{}/{}",
        kind.to_ascii_lowercase(),
        subtype.to_ascii_lowercase()
    );
    let mut charset = None;
    while !c.done() {
        c.bump();
        c.collect(is_http_ws);
        let name = c.collect(|ch| ch != ';' && ch != '=').to_ascii_lowercase();
        if let Some(ch) = c.peek() {
            if ch == ';' {
                continue;
            }
            c.bump();
        }
        if c.done() {
            break;
        }
        let value = if c.peek() == Some('"') {
            let v = c.quoted();
            c.collect(|ch| ch != ';');
            v
        } else {
            let v = c.collect(|ch| ch != ';');
            let v = v.trim_end_matches(is_http_ws).to_owned();
            if v.is_empty() {
                continue;
            }
            v
        };
        if charset.is_none() && name == "charset" && value.chars().all(is_quoted_token) {
            charset = Some(value);
        }
    }
    Some(Mime { essence, charset })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;

    fn ct(value: &str) -> ContentType {
        content_type(&[Field::new("Content-Type", value)])
    }

    #[test]
    fn charset_rule_cases_of_negotiation_4() {
        assert_eq!(ct("text/html; charset=utf-8"), ContentType::Utf8);
        assert_eq!(ct("text/html"), ContentType::Utf8);
        assert_eq!(ct("TEXT/HTML; CHARSET=UTF8"), ContentType::Utf8);
        assert_eq!(ct("text/html; charset=\"UTF-8\""), ContentType::Utf8);
        assert_eq!(ct("text/html; charset=\" utf-8 \""), ContentType::Utf8);
        assert_eq!(ct("text/html; charset = utf-8"), ContentType::Utf8);
        assert_eq!(ct("text/html; x=\"abc"), ContentType::Utf8);
        assert_eq!(ct("text/html; charset=x-unicode20utf8"), ContentType::Utf8);
        assert_eq!(ct("text/html; charset=iso-8859-1"), ContentType::NotUtf8);
        assert_eq!(ct("text/html; charset=\"\""), ContentType::NotUtf8);
        assert_eq!(ct("text/html; charset=utf-16"), ContentType::NotUtf8);
        // A quoted string ends at its quote: the comma after it is outside.
        assert_eq!(ct("text/html; x=\"a\", text/plain"), ContentType::NotUtf8);
        // The bytes after a quoted value up to `;` are discarded, also a
        // `charset=` in them.
        assert_eq!(ct("text/html; x=\"a\"Xcharset=latin1"), ContentType::Utf8);
        // Two media types: the list does not parse as one (step 1), or the
        // comma is outside a quoted string (item 1). Both pass A through.
        assert_eq!(ct("text/html, text/html"), ContentType::NotHtml);
        assert_eq!(ct("text/html; a=b, text/plain"), ContentType::NotUtf8);
        assert_eq!(ct("text/html; x=\"a,b\""), ContentType::Utf8);
        assert_eq!(ct("text/html; x=\"a\\\",b\""), ContentType::Utf8);
        assert_eq!(ct("application/json"), ContentType::NotHtml);
        assert_eq!(ct("text/htmlx"), ContentType::NotHtml);
        assert_eq!(ct("text"), ContentType::NotHtml);
        assert_eq!(ct("/html"), ContentType::NotHtml);
        assert_eq!(ct("text/"), ContentType::NotHtml);
        assert_eq!(content_type(&[]), ContentType::NotHtml);
    }

    #[test]
    fn charset_rule_first_valid_parameter_counts() {
        // An empty unquoted value does not count; the next one does.
        assert_eq!(ct("text/html; charset=; charset=utf-8"), ContentType::Utf8);
        assert_eq!(
            ct("text/html; charset=utf-8; charset=latin1"),
            ContentType::Utf8
        );
        assert_eq!(
            ct("text/html; charset=latin1; charset=utf-8"),
            ContentType::NotUtf8
        );
        // A value with a code point above U+00FF is not valid: the next one counts.
        assert_eq!(
            ct("text/html; charset=\u{100}; charset=utf-8"),
            ContentType::Utf8
        );
        // Backslash escapes in a quoted value.
        assert_eq!(ct("text/html; charset=\"ut\\f-8\""), ContentType::Utf8);
        // A parameter without "=".
        assert_eq!(ct("text/html; charset; charset=utf-8"), ContentType::Utf8);
        assert_eq!(ct("text/html; charset"), ContentType::Utf8);
        assert_eq!(ct("text/html;charset=utf-8  "), ContentType::Utf8);
    }

    #[test]
    fn charset_rule_two_lines_is_not_utf8() {
        let two = [
            Field::new("Content-Type", "text/html; charset=utf-8"),
            Field::new("content-type", "text/html; charset=utf-8"),
        ];
        assert_eq!(content_type(&two), ContentType::NotUtf8);
    }

    #[test]
    fn clean_removes_rq_fields_vary_members_and_framing() {
        let a = [
            Field::new("Rq-Version", "1"),
            Field::new("RQ-X", "1"),
            Field::new("rq-", "1"),
            Field::new("Rqx", "1"),
            Field::new("Vary", "Accept-Encoding, Rq-Nonce"),
            Field::new("vary", "RQ-Have, rq-nonce"),
            Field::new("Vary", "Cookie,,  Origin"),
            Field::new("Content-Length", "3"),
            Field::new("Transfer-Encoding", "chunked"),
            Field::new("Connection", "close"),
        ];
        assert_eq!(
            clean(&a),
            vec![
                Field::new("Rqx", "1"),
                Field::new("Vary", "Accept-Encoding"),
                Field::new("Vary", "Cookie,,  Origin"),
            ]
        );
    }

    #[test]
    fn content_coding_other_than_identity() {
        let f = |v: &str| has_content_coding(&[Field::new("Content-Encoding", v)]);
        assert!(f("gzip"));
        assert!(f("identity, br"));
        assert!(!f("identity"));
        assert!(!f("IDENTITY"));
        assert!(!f(" , "));
        assert!(!has_content_coding(&[]));
    }
}
