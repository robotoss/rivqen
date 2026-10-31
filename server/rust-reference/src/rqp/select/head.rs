// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The walk "Document head" of manifest and patch §1.7 (H-72, H-79a): the
//! insertion point of the manifest in the original document.

use rqp_markup::MarkupError;
use rqp_markup::scan::{Token, scan};

/// The implied-head start tags (H-79a).
const IMPLIED_HEAD: [&[u8]; 9] = [
    b"base",
    b"basefont",
    b"bgsound",
    b"link",
    b"meta",
    b"noframes",
    b"script",
    b"style",
    b"title",
];

/// End tags that are not ignored before the head (DL-034).
const NOT_IGNORED_END: [&[u8]; 4] = [b"head", b"body", b"html", b"br"];

const BOM: &[u8] = b"\xEF\xBB\xBF";

fn is_one_of(name: &[u8], set: &[&[u8]]) -> bool {
    set.iter().any(|n| name.eq_ignore_ascii_case(n))
}

/// A gap of white space: the bytes `09 0A 0C 0D 20`, and a byte order mark at
/// offset 0.
fn is_white_gap(input: &[u8], start: usize, end: usize) -> bool {
    let Some(mut bytes) = input.get(start..end) else {
        return false;
    };
    if start == 0 {
        if let Some(rest) = bytes.strip_prefix(BOM) {
            bytes = rest;
        }
    }
    bytes
        .iter()
        .all(|b| matches!(b, b'\t' | b'\n' | b'\x0C' | b'\r' | b' '))
}

/// The insertion point of the manifest: after the `>` of the head start tag of
/// the document, or at the `<` of the start tag that implies the head. `None`
/// when the document has neither (item 1: any other token, a gap with another
/// byte, or the end of the input).
///
/// # Errors
/// A [`MarkupError`] of the scan: an internal defect (fail closed).
pub fn insertion_point(input: &[u8]) -> Result<Option<usize>, MarkupError> {
    for token in scan(input) {
        match token? {
            Token::Gap(span) => {
                if !is_white_gap(input, span.start, span.end) {
                    return Ok(None);
                }
            }
            Token::Comment(_) | Token::Doctype(_) | Token::Cdata { .. } => {}
            Token::StartTag(tag) => {
                let name = tag.name();
                if name.eq_ignore_ascii_case(b"head") {
                    return Ok(Some(tag.span().end));
                }
                if is_one_of(name, &IMPLIED_HEAD) {
                    return Ok(Some(tag.span().start));
                }
                if !name.eq_ignore_ascii_case(b"html") {
                    return Ok(None);
                }
            }
            Token::EndTag(tag) => {
                if is_one_of(tag.name(), &NOT_IGNORED_END) {
                    return Ok(None);
                }
            }
            // A token kind of a later version of the scan: fail closed.
            _ => return Ok(None),
        }
    }
    Ok(None)
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

    fn at(input: &str) -> Option<usize> {
        insertion_point(input.as_bytes()).unwrap()
    }

    #[test]
    fn table_of_manifest_patch_1_7() {
        assert_eq!(at("<!DOCTYPE html>\n<html>\n<head>\n<title>"), Some(29));
        assert_eq!(
            at("<!-- c --><!DOCTYPE html><html lang=\"en\"><head>"),
            Some(47)
        );
        assert_eq!(at("</p><head>x"), Some(10));
        assert_eq!(at("<head lang=\"en\">"), Some(16));
        assert_eq!(at("<HEAD/>"), Some(7));
        assert_eq!(at("<html>\n<title>x</title></head>"), Some(7));
        assert_eq!(at("<meta charset=\"utf-8\"><head>"), Some(0));
        assert_eq!(at("<LINK/>"), Some(0));
        assert_eq!(at("Shop<html><head>"), None);
        assert_eq!(at("<noscript></noscript><head>"), None);
        assert_eq!(at("<template></template><head>"), None);
        assert_eq!(at("<body><head>"), None);
        assert_eq!(at("</br><head>"), None);
        assert_eq!(at("</body><head>"), None);
        assert_eq!(at("</html><head>"), None);
        assert_eq!(at("</head><head>"), None);
        assert_eq!(at("</><head>"), None);
        assert_eq!(at("<!-- <head> -->"), None);
        assert_eq!(at(""), None);
        assert_eq!(at("<html>"), None);
        assert_eq!(at("<![CDATA[x]]><head>"), Some(19));
        assert_eq!(at("<?pi x?><head>"), Some(14));
        assert_eq!(at("\t\n\x0C\r <head>"), Some(11));
        assert_eq!(at("<hea"), None);
    }

    #[test]
    fn byte_order_mark_counts_as_white_space_only_at_offset_0() {
        assert_eq!(at("\u{FEFF}<head>"), Some(9));
        assert_eq!(at("\u{FEFF}\n<head>"), Some(10));
        assert_eq!(at(" \u{FEFF}<head>"), None);
        assert_eq!(at("<html>\u{FEFF}<head>"), None);
    }
}
