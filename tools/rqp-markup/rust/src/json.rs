// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! JSON text check for `json` blocks (`markup.md` M-25, RFC 8259 §2).
//!
//! The check does not build values. It runs in one pass, without recursion,
//! and keeps at most [`MAX_DEPTH`] container kinds.

/// Maximum number of arrays and objects open at the same time (M-25).
pub const MAX_DEPTH: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Container {
    Array,
    Object,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    /// Consumes `byte` if it is next.
    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// RFC 8259 white space: space, tab, LF, CR.
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.bump();
        }
    }

    fn eat_digits(&mut self) -> bool {
        let start = self.pos;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.bump();
        }
        self.pos > start
    }

    fn literal(&mut self, text: &[u8]) -> bool {
        let end = self.pos.saturating_add(text.len());
        if self.bytes.get(self.pos..end) == Some(text) {
            self.pos = end;
            true
        } else {
            false
        }
    }

    /// `number = [ minus ] int [ frac ] [ exp ]`; any size.
    fn number(&mut self) -> bool {
        // Optional minus sign; `eat` consumes it when present.
        self.eat(b'-');
        let int = if self.eat(b'0') {
            true
        } else if self.peek().is_some_and(|b| matches!(b, b'1'..=b'9')) {
            self.eat_digits()
        } else {
            false
        };
        if !int {
            return false;
        }
        if self.eat(b'.') && !self.eat_digits() {
            return false;
        }
        if self.eat(b'e') || self.eat(b'E') {
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if !self.eat_digits() {
                return false;
            }
        }
        true
    }

    /// Four hex digits of a `\u` escape.
    fn hex4(&mut self) -> Option<u16> {
        let mut unit: u16 = 0;
        for _ in 0..4 {
            let digit = char::from(self.peek()?).to_digit(16)?;
            unit = unit
                .checked_mul(16)?
                .checked_add(u16::try_from(digit).ok()?)?;
            self.bump();
        }
        Some(unit)
    }

    /// A string; the opening quote is next.
    fn string(&mut self) -> bool {
        if !self.eat(b'"') {
            return false;
        }
        loop {
            let Some(byte) = self.peek() else {
                return false;
            };
            self.bump();
            match byte {
                b'"' => return true,
                b'\\' => {
                    if !self.escape() {
                        return false;
                    }
                }
                // Control characters must be escaped.
                0x00..=0x1F => return false,
                // The input is valid UTF-8 (M-02), so every other byte is
                // part of an allowed character.
                _ => {}
            }
        }
    }

    /// An escape after `\`.
    fn escape(&mut self) -> bool {
        let Some(byte) = self.peek() else {
            return false;
        };
        self.bump();
        match byte {
            b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => true,
            b'u' => match self.hex4() {
                // A high surrogate needs a low surrogate escape next.
                Some(0xD800..=0xDBFF) => {
                    self.eat(b'\\')
                        && self.eat(b'u')
                        && matches!(self.hex4(), Some(0xDC00..=0xDFFF))
                }
                // A lone low surrogate.
                Some(0xDC00..=0xDFFF) | None => false,
                Some(_) => true,
            },
            _ => false,
        }
    }
}

/// True when `bytes` is one JSON text by RFC 8259 §2 with a nesting depth of
/// at most [`MAX_DEPTH`].
#[must_use]
pub fn is_json_text(bytes: &[u8]) -> bool {
    let mut r = Reader { bytes, pos: 0 };
    let mut open: Vec<Container> = Vec::new();
    r.skip_ws();
    'value: loop {
        // A value is next.
        match r.peek() {
            Some(b'[' | b'{') => {
                let kind = if r.eat(b'[') {
                    Container::Array
                } else {
                    r.bump();
                    Container::Object
                };
                if open.len() >= MAX_DEPTH {
                    return false;
                }
                open.push(kind);
                r.skip_ws();
                let close = if kind == Container::Array { b']' } else { b'}' };
                if r.eat(close) {
                    open.pop();
                } else {
                    if kind == Container::Object && !member_name(&mut r) {
                        return false;
                    }
                    continue 'value;
                }
            }
            Some(b'"') => {
                if !r.string() {
                    return false;
                }
            }
            Some(b't') => {
                if !r.literal(b"true") {
                    return false;
                }
            }
            Some(b'f') => {
                if !r.literal(b"false") {
                    return false;
                }
            }
            Some(b'n') => {
                if !r.literal(b"null") {
                    return false;
                }
            }
            _ => {
                if !r.number() {
                    return false;
                }
            }
        }
        // After a value: close containers or read a separator.
        loop {
            r.skip_ws();
            let Some(&kind) = open.last() else {
                return r.pos == bytes.len();
            };
            if r.eat(b',') {
                r.skip_ws();
                if kind == Container::Object && !member_name(&mut r) {
                    return false;
                }
                continue 'value;
            }
            let close = if kind == Container::Array { b']' } else { b'}' };
            if !r.eat(close) {
                return false;
            }
            open.pop();
        }
    }
}

/// `string name-separator`, then white space; a value is next.
fn member_name(r: &mut Reader<'_>) -> bool {
    if !r.string() {
        return false;
    }
    r.skip_ws();
    if !r.eat(b':') {
        return false;
    }
    r.skip_ws();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> bool {
        is_json_text(s.as_bytes())
    }

    #[test]
    fn valid_texts() {
        for text in [
            "0",
            "-0",
            "1.5e+10",
            "-12.25E-3",
            "1e400",
            "123456789012345678901234567890",
            "true",
            "false",
            "null",
            "\"\"",
            "\"a\\\"\\\\\\/\\b\\f\\n\\r\\t\\u00e9\"",
            "\"\\ud83d\\ude00\"",
            "\"\\uD83D\\uDE00\"",
            "\"caf\u{e9} \u{1F600} \u{7f}\"",
            "[]",
            "{}",
            " \t\r\n[ 1 , {\"a\" : [ ] } ] \r\n",
            "{\"a\":1,\"a\":2}",
            "[[],[[]],{}]",
        ] {
            assert!(ok(text), "{text:?}");
        }
    }

    #[test]
    fn invalid_texts() {
        for text in [
            "",
            " ",
            "\u{feff}[1]",
            "\u{a0}[1]",
            "[1,]",
            "{\"a\":1,}",
            "[1 2]",
            "{\"a\" 1}",
            "{\"a\":}",
            "{1:2}",
            "{\"a\":1",
            "[",
            "]",
            "[1]]",
            "01",
            "-",
            "+1",
            "1.",
            ".5",
            "1e",
            "1e+",
            "0x10",
            "NaN",
            "tru",
            "nul",
            "fals",
            "True",
            "\"a",
            "\"\\x\"",
            "\"\\u12\"",
            "\"\\u12G4\"",
            "\"\\ud800\"",
            "\"\\udc00\"",
            "\"\\ud800\\u0041\"",
            "\"\\ud800x\"",
            "\"\\ud800\\ud800\"",
            "\"a\tb\"",
            "\"\u{1}\"",
            "1 2",
            "[] []",
            "\"\\",
        ] {
            assert!(!ok(text), "{text:?}");
        }
    }

    #[test]
    fn depth_limit() {
        let arrays = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(ok(&arrays(64)));
        assert!(!ok(&arrays(65)));
        let objects = |n: usize| format!("{}1{}", "{\"k\":".repeat(n), "}".repeat(n));
        assert!(ok(&objects(64)));
        assert!(!ok(&objects(65)));
        // Depth counts containers open at the same time, not all containers.
        let wide = format!("[{}]", vec!["[[]]"; 200].join(","));
        assert!(ok(&wide));
    }
}
