// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The patch content rule of manifest and patch §4 item 1 and the attribute rule
//! of §4.2 (H-56, H-59, H-73, H-77, H-78, H-83, H-83a, H-96, DL-048, DL-051,
//! DL-052), on the start tags of the content of `html` blocks.
//!
//! The start tags are the tokens of the whole document inside the content span
//! of a block (`rqp_markup::scan`; markup §2.3): the content is read in the
//! state that the analysis has there. Names compare ASCII case-insensitively
//! with the lower-case names of the rules (the tokenizer lowercases ASCII).

use rqp_markup::MarkupError;
use rqp_markup::scan::{Token, scan};

/// `PATCH_FORBIDDEN` (§4 item 1.1; H-78, H-96).
const PATCH_FORBIDDEN: [&[u8]; 11] = [
    b"base",
    b"basefont",
    b"bgsound",
    b"embed",
    b"iframe",
    b"link",
    b"meta",
    b"noscript",
    b"object",
    b"script",
    b"style",
];

/// `FOREIGN_ALLOWED` (§4; H-73).
const FOREIGN_ALLOWED: [&[u8]; 70] = [
    b"svg",
    b"g",
    b"path",
    b"circle",
    b"ellipse",
    b"line",
    b"polyline",
    b"polygon",
    b"rect",
    b"defs",
    b"symbol",
    b"use",
    b"title",
    b"desc",
    b"lineargradient",
    b"radialgradient",
    b"stop",
    b"clippath",
    b"mask",
    b"pattern",
    b"marker",
    b"text",
    b"tspan",
    b"filter",
    b"feblend",
    b"fecolormatrix",
    b"fecomponenttransfer",
    b"fecomposite",
    b"fedisplacementmap",
    b"fedropshadow",
    b"feflood",
    b"fefunca",
    b"fefuncb",
    b"fefuncg",
    b"fefuncr",
    b"fegaussianblur",
    b"femerge",
    b"femergenode",
    b"femorphology",
    b"feoffset",
    b"feturbulence",
    b"math",
    b"mrow",
    b"mi",
    b"mo",
    b"mn",
    b"ms",
    b"mtext",
    b"mspace",
    b"msub",
    b"msup",
    b"msubsup",
    b"mfrac",
    b"msqrt",
    b"mroot",
    b"mstyle",
    b"munder",
    b"mover",
    b"munderover",
    b"mmultiscripts",
    b"mprescripts",
    b"none",
    b"mtable",
    b"mtr",
    b"mtd",
    b"mpadded",
    b"mphantom",
    b"merror",
    b"semantics",
    b"annotation",
];

/// The raster types of a `data:` image (§4.2 item 4.2; H-83a, DL-052).
const RASTER_TYPES: [&[u8]; 10] = [
    b"image/png",
    b"image/apng",
    b"image/jpeg",
    b"image/jpg",
    b"image/gif",
    b"image/webp",
    b"image/avif",
    b"image/bmp",
    b"image/x-icon",
    b"image/vnd.microsoft.icon",
];

/// Named references at the decision byte (§4.2 item 2.2; H-77).
const DECISION_NAMES: [&[u8]; 6] = [
    b"Tab;",
    b"NewLine;",
    b"colon;",
    b"fjlig;",
    b"period;",
    b"plus;",
];

/// Named references that decode to ASCII white space, `,`, `(` or `)` (§4.2
/// item 4.3; H-83).
const SRCSET_NAMES: [&[u8]; 5] = [b"Tab;", b"NewLine;", b"comma;", b"lpar;", b"rpar;"];

fn is_one_of(name: &[u8], set: &[&[u8]]) -> bool {
    set.iter().any(|n| name.eq_ignore_ascii_case(n))
}

/// True when every start tag inside the content spans `[start, end)` of
/// `blocks` passes the patch content rule. `blocks` are in document order and
/// do not overlap.
///
/// # Errors
/// A [`MarkupError`] of the scan: an internal defect (the caller fails closed).
pub fn blocks_pass(input: &[u8], blocks: &[(usize, usize)]) -> Result<bool, MarkupError> {
    let mut remaining = blocks.iter().peekable();
    if remaining.peek().is_none() {
        return Ok(true);
    }
    for token in scan(input) {
        let token = token?;
        let span = token.span();
        // Drop the blocks that end before this token starts.
        while remaining.peek().is_some_and(|&&(_, end)| end <= span.start) {
            remaining.next();
        }
        let Some(&&(start, end)) = remaining.peek() else {
            return Ok(true);
        };
        let Token::StartTag(tag) = token else {
            continue;
        };
        if start <= span.start && span.end <= end && !tag_passes(&tag)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// §4 item 1 for one start tag.
fn tag_passes(tag: &rqp_markup::scan::Tag<'_>) -> Result<bool, MarkupError> {
    let name = tag.name();
    let html = !tag.in_foreign_region();
    if html && is_one_of(name, &PATCH_FORBIDDEN) {
        return Ok(false);
    }
    if !html && !is_one_of(name, &FOREIGN_ALLOWED) {
        return Ok(false);
    }
    for attribute in tag.attributes() {
        let attribute = attribute?;
        if attribute_rejects(name, html, attribute.name(), attribute.value()) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The attribute rule of §4.2 for one raw attribute of the start tag `element`
/// (`html`: the tag is in HTML context).
#[must_use]
pub fn attribute_rejects(element: &[u8], html: bool, name: &[u8], value: &[u8]) -> bool {
    // Item 1: an inline event handler.
    if name.get(..2).is_some_and(|p| p.eq_ignore_ascii_case(b"on")) {
        return true;
    }
    if name.eq_ignore_ascii_case(b"srcset") {
        return srcset_rejects(element, html, value);
    }
    url_rejects(element, html, name, value)
}

/// Items 2 to 4.2 for one value.
fn url_rejects(element: &[u8], html: bool, name: &[u8], value: &[u8]) -> bool {
    if decision_reference_rejects(value) {
        return true;
    }
    let copy = normalized_copy(value);
    let starts = |prefix: &[u8]| {
        copy.get(..prefix.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(prefix))
    };
    if starts(b"javascript:") || starts(b"vbscript:") {
        return true;
    }
    if starts(b"data:") {
        return !data_image_allowed(element, html, name, copy.get(5..).unwrap_or_default());
    }
    false
}

/// Item 2: the character reference at the scheme decision byte (H-77), read on
/// the raw bytes.
fn decision_reference_rejects(value: &[u8]) -> bool {
    let mut rest = value;
    while let Some((&b, tail)) = rest.split_first() {
        if b > 0x20 {
            break;
        }
        rest = tail;
    }
    while let Some((&b, tail)) = rest.split_first() {
        if !(matches!(b, b'\t' | b'\n' | b'\r' | b'+' | b'-' | b'.') || b.is_ascii_alphanumeric()) {
            break;
        }
        rest = tail;
    }
    let Some(after) = rest.strip_prefix(b"&") else {
        return false;
    };
    match numeric_reference(after) {
        Some(v) => {
            v <= 0x20
                || matches!(v, 0x3A | 0x2B | 0x2D | 0x2E)
                || (0x30..=0x39).contains(&v)
                || (0x41..=0x5A).contains(&v)
                || (0x61..=0x7A).contains(&v)
        }
        None => DECISION_NAMES.iter().any(|n| after.starts_with(n)),
    }
}

/// The value of a numeric character reference that starts after a `&` (markup
/// M-13): `#` and decimal digits, or `#x`/`#X` and hex digits, all digits,
/// leading zeros too, `;` optional. A value above `10FFFF` is `110000`. `None`
/// when no reference starts here.
fn numeric_reference(after_amp: &[u8]) -> Option<u32> {
    let rest = after_amp.strip_prefix(b"#")?;
    let (digits, radix) = match rest.split_first() {
        Some((b'x' | b'X', hex)) => (hex, 16),
        _ => (rest, 10),
    };
    let mut value: u32 = 0;
    let mut count = 0_usize;
    for &b in digits {
        let Some(d) = char::from(b).to_digit(radix) else {
            break;
        };
        value = value
            .checked_mul(radix)
            .and_then(|v| v.checked_add(d))
            .map_or(0x11_0000, |v| v.min(0x11_0000));
        count = count.saturating_add(1);
    }
    (count > 0).then_some(value)
}

/// Item 3: the copy without tab, LF and CR, then without the bytes `00` to `20`
/// at the start.
fn normalized_copy(value: &[u8]) -> Vec<u8> {
    value
        .iter()
        .copied()
        .filter(|b| !matches!(b, b'\t' | b'\n' | b'\r'))
        .skip_while(|&b| b <= 0x20)
        .collect()
}

/// Item 4.2: the exception for a `data:` raster image (H-83, H-83a). `after` is
/// the copy after `data:`.
fn data_image_allowed(element: &[u8], html: bool, name: &[u8], after: &[u8]) -> bool {
    let image = (element.eq_ignore_ascii_case(b"img") || element.eq_ignore_ascii_case(b"source"))
        && (name.eq_ignore_ascii_case(b"src") || name.eq_ignore_ascii_case(b"srcset"));
    let poster = element.eq_ignore_ascii_case(b"video") && name.eq_ignore_ascii_case(b"poster");
    let place = image || poster;
    let end = after
        .iter()
        .position(|&b| b == b';' || b == b',')
        .unwrap_or(after.len());
    let kind = after.get(..end).unwrap_or_default();
    html && place && is_one_of(kind, &RASTER_TYPES)
}

fn is_ascii_ws(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
}

/// Item 4.3: `srcset` (H-83). The value is split into image candidates as
/// "parse a srcset attribute" (WHATWG HTML §4.8.4.3.10) splits it, and each
/// candidate URL is checked as a `src` value of the same element.
fn srcset_rejects(element: &[u8], html: bool, value: &[u8]) -> bool {
    // A reference that decodes to white space, `,`, `(` or `)`.
    let mut rest = value;
    while let Some(i) = rest.iter().position(|&b| b == b'&') {
        let after = rest.get(i.saturating_add(1)..).unwrap_or_default();
        let bad = match numeric_reference(after) {
            Some(v) => matches!(v, 0x09 | 0x0A | 0x0C | 0x0D | 0x20 | 0x2C | 0x28 | 0x29),
            None => SRCSET_NAMES.iter().any(|n| after.starts_with(n)),
        };
        if bad {
            return true;
        }
        rest = after;
    }
    let mut pos = 0_usize;
    let at = |p: usize| value.get(p).copied();
    loop {
        while at(pos).is_some_and(|b| is_ascii_ws(b) || b == b',') {
            pos = pos.saturating_add(1);
        }
        if at(pos).is_none() {
            return false;
        }
        let start = pos;
        while at(pos).is_some_and(|b| !is_ascii_ws(b)) {
            pos = pos.saturating_add(1);
        }
        let mut url = value.get(start..pos).unwrap_or_default();
        let ends_with_comma = url.last() == Some(&b',');
        while let Some(stripped) = url.strip_suffix(b",") {
            url = stripped;
        }
        if url_rejects(element, html, b"src", url) {
            return true;
        }
        if !ends_with_comma {
            pos = skip_descriptors(value, pos);
        }
    }
}

/// The descriptor tokenizer of "parse a srcset attribute": the position after
/// the comma that ends the descriptors (a comma inside parentheses does not),
/// or the end of the value.
fn skip_descriptors(value: &[u8], mut pos: usize) -> usize {
    let mut in_parens = false;
    while let Some(&b) = value.get(pos) {
        pos = pos.saturating_add(1);
        if in_parens {
            if b == b')' {
                in_parens = false;
            }
        } else if b == b'(' {
            in_parens = true;
        } else if b == b',' {
            return pos;
        }
    }
    pos
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

    fn rejects(element: &str, name: &str, value: &str) -> bool {
        attribute_rejects(element.as_bytes(), true, name.as_bytes(), value.as_bytes())
    }

    fn content_passes(content: &str) -> bool {
        let doc = format!(
            "<!DOCTYPE html><html><head></head><body><div data-rq-block=\"b\">{content}</div></body></html>"
        );
        let start = doc.find("\">").unwrap() + 2;
        let end = doc.rfind("</div>").unwrap();
        blocks_pass(doc.as_bytes(), &[(start, end)]).unwrap()
    }

    #[test]
    fn table_of_manifest_patch_4_2() {
        for (element, name, value) in [
            ("a", "onclick", "x()"),
            ("a", "ONLOAD", "x"),
            ("a", "href", "javascript:x"),
            ("a", "href", " JavaScript:x"),
            ("a", "href", "vbscript:x"),
            ("a", "href", "data:text/html,x"),
            ("img", "src", " \tDa\nTa:x"),
            ("a", "href", "java&#x09;script:x"),
            ("a", "href", "&#106;avascript:x"),
            ("a", "href", "java&colon;x"),
            ("a", "href", "&Tab;javascript:x"),
            ("a", "href", "java&#x73\ncript:x"),
            ("img", "src", "data:image/svg+xml,x"),
            ("img", "src", "data:text/html,x"),
            ("a", "href", "data:image/png,x"),
            ("img", "src", "data:image/ svg+xml,x"),
            ("img", "src", "data:image/svg,x"),
            ("img", "src", "data:image/png ,x"),
            ("img", "src", "data:image/svg%2Bxml,x"),
            ("img", "src", "data:image/svg&#43;xml,x"),
            ("img", "src", "data:image/\x0Bsvg+xml,x"),
            ("img", "src", "data:image/\x0Csvg+xml,x"),
            ("img", "src", "data: image/png,x"),
            ("img", "srcset", "/a.png 1x, data:image/svg+xml,x 2x"),
            ("source", "srcset", "/a.png 1x, javascript:x 2x"),
            ("img", "srcset", "/a.png 1x,&#32;data:text/html,x 2x"),
            ("img", "srcset", "/a.png 1x&comma;javascript:x"),
            ("a", "title", "\u{1} VbScRiPt:x"),
            ("text", "y", "&#x6a;"),
            // Item 2: a space is skipped before the decision byte; references
            // to `:`, a digit and an upper-case letter reject.
            ("a", "href", " &#106;avascript:x"),
            ("a", "href", "java&#58;x"),
            ("a", "href", "&#49;x"),
            ("a", "href", "&#74;avascript:x"),
        ] {
            assert!(rejects(element, name, value), "{element} {name}={value:?}");
        }
        for (element, name, value) in [
            ("b", "title", "a &quot; b"),
            ("a", "href", "?a=1&b=2"),
            ("a", "href", "/p?a=1&amp;b=2"),
            ("b", "title", "Tom &amp; Jerry"),
            ("b", "alt", "&#169; 2026"),
            ("a", "href", "data&amp;:x"),
            ("a", "href", "&COLON;x"),
            ("a", "data-x", "javascript"),
            ("a", "href", "/javascript:x"),
            ("a", "href", "database:x"),
            ("img", "src", "data:image/png;base64,iVBO"),
            ("video", "poster", "data:image/jpeg;base64,x"),
            ("img", "src", "DATA:IMAGE/PNG,x"),
            ("img", "src", " \tDa\nTa:image/png,x"),
            ("img", "src", "data:image/jpeg;charset=x;base64,x"),
            ("img", "src", "data:image/gif,x"),
            ("source", "src", "data:image/webp,x"),
            ("img", "srcset", "/a.png 1x, data:image/png;base64,iVBO 2x"),
            ("img", "srcset", "/a.png (x,y) 1x, /b.png"),
            ("img", "srcset", ",,/a.png,,"),
            ("a", "href", ""),
            // Item 4.3: descriptors are not URLs, also in parentheses with a comma.
            ("img", "srcset", "/a.png javascript:x"),
            ("img", "srcset", "/a.png (x,javascript:x) 1x"),
        ] {
            assert!(!rejects(element, name, value), "{element} {name}={value:?}");
        }
    }

    #[test]
    fn data_image_needs_html_context() {
        assert!(attribute_rejects(
            b"img",
            false,
            b"src",
            b"data:image/png,x"
        ));
        assert!(!attribute_rejects(
            b"img",
            true,
            b"src",
            b"data:image/png,x"
        ));
    }

    #[test]
    fn srcset_of_other_elements_rejects_data() {
        assert!(rejects("div", "srcset", "data:image/png,x"));
        assert!(rejects("video", "srcset", "data:image/png,x"));
    }

    #[test]
    fn numeric_reference_reads_raw_bytes() {
        assert_eq!(numeric_reference(b"#x73\ncript"), Some(0x73));
        assert_eq!(numeric_reference(b"#0000106;"), Some(106));
        assert_eq!(numeric_reference(b"#X3a"), Some(0x3A));
        assert_eq!(numeric_reference(b"#99999999999999999999"), Some(0x11_0000));
        assert_eq!(numeric_reference(b"#xFFFFFFFFFFFF"), Some(0x11_0000));
        assert_eq!(numeric_reference(b"#x;"), None);
        assert_eq!(numeric_reference(b"#;"), None);
        assert_eq!(numeric_reference(b"amp;"), None);
    }

    #[test]
    fn patch_forbidden_and_foreign_allowed() {
        for content in [
            "<script>alert(1)</script>",
            "<style>p{color:red}</style>",
            "<iframe src=\"/v\"></iframe>",
            "<noscript><span>x</span></noscript>",
            "<LINK/>",
            "<template><script>x</script></template>",
            "<svg><a><text>t</text></a></svg>",
            "<svg><title>x</title><path onload=\"x\"/></svg>",
            "<svg><use xlink:href=\"javascript:x\"/></svg>",
            "<svg><desc>d</desc><use href=\"data:image/svg+xml,x\"/></svg>",
            "<a onclick=\"x\">y</a>",
        ] {
            assert!(!content_passes(content), "{content}");
        }
        for content in [
            "120.00",
            "<b>x</b>",
            "<template><span>x</span></template>",
            "<svg><title>t</title><path d=\"M0\"/><use href=\"#i\"/></svg>",
            "<math><mi>x</mi></math>",
            "<img src=\"data:image/png;base64,iVBO\" alt=\"\">",
        ] {
            assert!(content_passes(content), "{content}");
        }
    }

    #[test]
    fn only_tags_inside_the_blocks_count() {
        let doc = b"<head><script>x</script></head><body><p data-rq-block=\"a\">ok</p><script>y</script><p data-rq-block=\"b\"><b>z</b></p>";
        let a = (doc.windows(2).position(|w| w == b"ok").unwrap(), 0);
        let a = (a.0, a.0 + 2);
        let bstart = doc.windows(3).position(|w| w == b"<b>").unwrap();
        let b = (bstart, bstart + 8);
        assert!(blocks_pass(doc, &[a, b]).unwrap());
        assert!(blocks_pass(doc, &[]).unwrap());
        // A span that covers the second script fails.
        let script = doc.windows(8).rposition(|w| w == b"<script>").unwrap();
        assert!(!blocks_pass(doc, &[a, (script, script + 8)]).unwrap());
    }
}
