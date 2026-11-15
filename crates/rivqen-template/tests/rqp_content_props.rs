// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the patch content rule (WP-06 S2 T-02).
//!
//! - P-CONTENT-1 (`content_prop_no_missed_scheme`): a test-only model of
//!   the browser (character reference decoding of an attribute value, then
//!   the scheme steps of the URL Standard, and the WHATWG `srcset` split on
//!   the decoded value) finds a `javascript:`, `vbscript:` or `data:` URL
//!   (outside the H-83 exception) only in values that `check_attribute`
//!   rejects.
//! - P-CONTENT-2 (`content_prop_accepted_has_no_forbidden`): any bytes as
//!   block content never panic, and an accepted content has no start tag
//!   (by the scan) with a `PATCH_FORBIDDEN` name in HTML context, no
//!   foreign start tag outside `FOREIGN_ALLOWED` and no attribute name that
//!   starts with `on`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use rivqen_proto::types::BlockFormat;
use rivqen_template::limits::{MAX_LEGACY_NAME_BYTES, TemplateLimits};
use rivqen_template::rqp::content::{
    FOREIGN_ALLOWED, PATCH_FORBIDDEN, check_attribute, check_patch_content,
};
use rivqen_template::rqp::scan::{Token, scan};
use rivqen_template::rqp::{RqpBlock, RqpDocument, split_rqp};

// ---------------------------------------------------------------------------
// Test-only model of the browser (P-CONTENT-1).

/// Named references of the model: the ones that decode to a scheme
/// character, `:`, white space, `,`, `(` or `)` (entities.json; section 4.2
/// "Why item 2 is sound" and "Why the srcset split is sound"), and a few
/// that decode to other characters.
const NAMED: [(&str, &str); 16] = [
    ("Tab;", "\t"),
    ("NewLine;", "\n"),
    ("colon;", ":"),
    ("fjlig;", "fj"),
    ("period;", "."),
    ("plus;", "+"),
    ("comma;", ","),
    ("lpar;", "("),
    ("rpar;", ")"),
    ("amp;", "&"),
    ("quot;", "\""),
    ("lt;", "<"),
    ("gt;", ">"),
    ("sol;", "/"),
    ("excl;", "!"),
    ("nbsp;", "\u{A0}"),
];

/// The character of a numeric reference (WHATWG "numeric character
/// reference end state"): 0, a surrogate or a number over 10FFFF give
/// U+FFFD; 80 to 9F map to characters that are not ASCII (stand-in
/// U+FFFD; the exact Windows-1252 character does not change a scheme).
fn numeric_char(value: u64) -> char {
    if value == 0 || (0x80..=0x9F).contains(&value) {
        return '\u{FFFD}';
    }
    u32::try_from(value)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or('\u{FFFD}')
}

/// An attribute value as the tokenizer decodes it: CR and CR LF become LF
/// (input preprocessing), U+0000 becomes U+FFFD, numeric references in all
/// forms, the named references of [`NAMED`].
fn decode(raw: &str) -> Vec<char> {
    let raw = raw.replace("\r\n", "\n").replace('\r', "\n");
    let chars: Vec<char> = raw.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\0' {
            out.push('\u{FFFD}');
            i += 1;
            continue;
        }
        if c != '&' {
            out.push(c);
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'#') {
            let mut j = i + 2;
            let hex = matches!(chars.get(j), Some('x' | 'X'));
            if hex {
                j += 1;
            }
            let radix = if hex { 16 } else { 10 };
            let first = j;
            let mut value: u64 = 0;
            while let Some(d) = chars.get(j).and_then(|c| c.to_digit(radix)) {
                value = (value * u64::from(radix) + u64::from(d)).min(0x11_0000);
                j += 1;
            }
            if j > first {
                if chars.get(j) == Some(&';') {
                    j += 1;
                }
                out.push(numeric_char(value));
                i = j;
                continue;
            }
        } else {
            let rest: String = chars[i + 1..].iter().collect();
            if let Some((name, text)) = NAMED.iter().find(|(name, _)| rest.starts_with(name)) {
                out.extend(text.chars());
                i += 1 + name.chars().count();
                continue;
            }
        }
        out.push('&');
        i += 1;
    }
    out
}

/// The scheme steps of the URL Standard: remove leading C0 control or
/// space, remove all ASCII tab or newline, read the scheme (an ASCII letter,
/// then letters, digits, `+`, `-`, `.`, up to a `:`). Gives the scheme in
/// lower case and the rest after the `:`.
fn scheme(decoded: &[char]) -> Option<(String, Vec<char>)> {
    let input: Vec<char> = decoded
        .iter()
        .copied()
        .skip_while(|&c| c <= ' ')
        .filter(|&c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    if !input.first()?.is_ascii_alphabetic() {
        return None;
    }
    let end = input
        .iter()
        .position(|&c| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))?;
    if input[end] != ':' {
        return None;
    }
    let name: String = input[..end].iter().collect();
    Some((name.to_ascii_lowercase(), input[end + 1..].to_vec()))
}

const RASTER: [&str; 10] = [
    "image/png",
    "image/apng",
    "image/jpeg",
    "image/jpg",
    "image/gif",
    "image/webp",
    "image/avif",
    "image/bmp",
    "image/x-icon",
    "image/vnd.microsoft.icon",
];

/// The model finds a URL that the rule must not let through in this
/// decoded value used as `name` of `element`.
fn forbidden_url(element: &str, foreign: bool, name: &str, decoded: &[char]) -> bool {
    let Some((scheme, rest)) = scheme(decoded) else {
        return false;
    };
    match scheme.as_str() {
        "javascript" | "vbscript" => true,
        "data" => {
            let allowed_place = !foreign
                && matches!(
                    (element, name),
                    ("img" | "source", "src" | "srcset") | ("video", "poster")
                );
            let kind: String = rest
                .iter()
                .take_while(|&&c| c != ';' && c != ',')
                .collect::<String>()
                .to_ascii_lowercase();
            !(allowed_place && RASTER.contains(&kind.as_str()))
        }
        _ => false,
    }
}

fn white(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{C}' | '\r' | ' ')
}

/// The candidate URLs of the WHATWG algorithm "parse a srcset attribute"
/// on the decoded value, with the descriptor tokenizer states as written
/// (in descriptor, in parens, after descriptor). Every candidate URL, also
/// one whose descriptors the browser would reject later.
fn srcset_urls(decoded: &[char]) -> Vec<Vec<char>> {
    #[derive(PartialEq)]
    enum State {
        InDescriptor,
        InParens,
        AfterDescriptor,
    }
    let mut urls = Vec::new();
    let mut pos = 0;
    loop {
        while pos < decoded.len() && (white(decoded[pos]) || decoded[pos] == ',') {
            pos += 1;
        }
        if pos >= decoded.len() {
            return urls;
        }
        let start = pos;
        while pos < decoded.len() && !white(decoded[pos]) {
            pos += 1;
        }
        let mut url = decoded[start..pos].to_vec();
        if url.last() == Some(&',') {
            while url.last() == Some(&',') {
                url.pop();
            }
        } else {
            while pos < decoded.len() && white(decoded[pos]) {
                pos += 1;
            }
            let mut state = State::InDescriptor;
            loop {
                let c = decoded.get(pos).copied();
                match state {
                    State::InDescriptor => match c {
                        Some(c) if white(c) => state = State::AfterDescriptor,
                        Some(',') => {
                            pos += 1;
                            break;
                        }
                        Some('(') => state = State::InParens,
                        None => break,
                        Some(_) => {}
                    },
                    State::InParens => match c {
                        Some(')') => state = State::InDescriptor,
                        None => break,
                        Some(_) => {}
                    },
                    State::AfterDescriptor => match c {
                        Some(c) if white(c) => {}
                        None => break,
                        Some(_) => {
                            state = State::InDescriptor;
                            continue;
                        }
                    },
                }
                pos += 1;
            }
        }
        urls.push(url);
    }
}

/// The model of the browser for one attribute.
fn model_forbidden(element: &str, foreign: bool, name: &str, raw: &str) -> bool {
    let decoded = decode(raw);
    if name == "srcset" {
        srcset_urls(&decoded)
            .iter()
            .any(|url| forbidden_url(element, foreign, "src", url))
    } else {
        forbidden_url(element, foreign, name, &decoded)
    }
}

// ---------------------------------------------------------------------------
// Generators of attribute values.

const PREFIXES: &[&str] = &[
    "",
    "",
    " ",
    "\t",
    "\u{1}",
    "\u{0}",
    "&#32;",
    "&#x0;",
    "\n ",
    "\u{C}",
    "&Tab;",
    "/a.png 1x, ",
    "/a.png,",
    "/a.png, ",
    "/a.png (1x, y) ",
    "/a 1x,&#32;",
    "/a 1x&comma;",
];

const SCHEMES: &[&str] = &["javascript:", "vbscript:", "data:", "java:", "datas:"];

/// Noise between two characters of the scheme (one place in four).
const NOISE: &[&str] = &[
    "",
    "",
    "",
    "",
    "",
    "",
    "\t",
    "\n",
    "\r",
    "\r\n",
    "&Tab;",
    "&NewLine;",
    "&#9;",
    "&#x0A;",
    "&#13;",
    "\u{B}",
    " ",
    "\u{0}",
    "&#0;",
    "&",
];

const TAILS: &[&str] = &[
    "x",
    "alert(1)",
    "",
    "image/png,x",
    "image/png;base64,AAAA",
    "IMAGE/GIF,x",
    "image/svg+xml,x",
    "image/png ,x",
    "image/p&#110;g,x",
    "image/p\tng,x",
    "image/svg&plus;xml,x",
    ",x",
    " 2x",
    "image/gif 1x, /b.png",
    "x 1x, javascript:y",
    "x, data:text/html,y",
];

/// One character of a scheme in one of its raw forms.
fn form(c: char, choice: u8) -> String {
    let code = u32::from(c);
    match choice {
        0 | 1 => c.to_string(),
        2 => c.to_ascii_uppercase().to_string(),
        3 => format!("&#{code};"),
        4 => format!("&#{code}"),
        5 => format!("&#x{code:x};"),
        6 => format!("&#X{code:X};"),
        7 => format!("&#000{code};"),
        _ if c == ':' => "&colon;".to_owned(),
        _ if c == '.' => "&period;".to_owned(),
        _ => c.to_string(),
    }
}

/// A scheme in literal, upper-case and reference forms with noise between
/// its characters, after a prefix and before a tail.
fn obfuscated() -> impl Strategy<Value = String> {
    (
        prop::sample::select(PREFIXES),
        prop::sample::select(SCHEMES),
        prop::collection::vec((0..9u8, 0..NOISE.len() * 4), 12),
        prop::sample::select(TAILS),
    )
        .prop_map(|(prefix, scheme, choices, tail)| {
            let mut out = String::from(prefix);
            for (i, c) in scheme.chars().enumerate() {
                let (choice, noise) = choices[i % choices.len()];
                out.push_str(&form(c, choice));
                out.push_str(NOISE.get(noise).copied().unwrap_or_default());
            }
            out.push_str(tail);
            out
        })
}

/// Free concatenations of pieces.
const PIECES: &[&str] = &[
    "java",
    "script",
    "vb",
    "data",
    "JaVa",
    "SCRIPT",
    "DATA",
    ":",
    "&#58;",
    "&#x3a",
    "&colon;",
    "&#x73",
    "&#115;",
    "&#106;",
    "&#x6A",
    "&#0000106",
    "&Tab;",
    "&NewLine;",
    "&fjlig;",
    "&period;",
    "&plus;",
    "&#9;",
    "&#x0A",
    "&#13;",
    "&#32;",
    "&#0;",
    "&#x2b;",
    "&#45;",
    "&#46",
    "\t",
    "\n",
    "\r",
    " ",
    "\u{0}",
    "\u{1}",
    "\u{B}",
    "\u{C}",
    "&",
    "&amp;",
    "&#",
    "&#x",
    "a",
    "j",
    "s",
    "v",
    "x",
    "1",
    "+",
    "-",
    ".",
    "/",
    "?",
    "image/png",
    "image/svg+xml",
    "png",
    ";base64",
    ",",
    "(",
    ")",
    "&comma;",
    "&lpar;",
    "&rpar;",
    "&#44;",
    "&#x28",
    "\u{FFFD}",
    "é",
];

fn pieces() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(PIECES), 0..16).prop_map(|p| p.concat())
}

fn value() -> impl Strategy<Value = String> {
    prop_oneof![3 => obfuscated(), 1 => pieces()]
}

/// (element, foreign, attribute) of the property.
const PLACES: &[(&str, bool, &str)] = &[
    ("a", false, "href"),
    ("img", false, "src"),
    ("video", false, "poster"),
    ("source", false, "src"),
    ("img", false, "srcset"),
    ("source", false, "srcset"),
    ("video", false, "srcset"),
    ("div", false, "srcset"),
    ("use", true, "href"),
    ("image", true, "href"),
    ("img", true, "src"),
    ("p", false, "title"),
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

    /// P-CONTENT-1.
    #[test]
    fn content_prop_no_missed_scheme(
        raw in value(),
        place in prop::sample::select(PLACES),
    ) {
        let (element, foreign, name) = place;
        if model_forbidden(element, foreign, name, &raw) {
            prop_assert!(
                check_attribute(element.as_bytes(), foreign, name.as_bytes(), raw.as_bytes())
                    .is_err(),
                "missed: {element} {foreign} {name} {raw:?}"
            );
        }
    }

    /// The attribute rule on any bytes: no panic, and item 1 decides alone
    /// for a name that starts with `on`.
    #[test]
    fn content_prop_attribute_any_bytes(
        element in prop::collection::vec(any::<u8>(), 0..8),
        foreign in any::<bool>(),
        name in prop::collection::vec(any::<u8>(), 0..8),
        raw in prop::collection::vec(any::<u8>(), 0..64),
    ) {
        let result = check_attribute(&element, foreign, &name, &raw);
        if name.starts_with(b"on") {
            prop_assert!(result.is_err());
        }
    }
}

/// Guard against a generator that never reaches the cases that matter.
#[test]
fn content_prop_generator_reaches_forbidden_values() {
    use proptest::strategy::ValueTree as _;
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let (mut forbidden, mut accepted, mut with_reference) = (0, 0, 0);
    let (mut later_candidate, mut raster) = (0, 0);
    let total = 5_000;
    for _ in 0..total {
        let raw = value().new_tree(&mut runner).unwrap().current();
        if model_forbidden("a", false, "href", &raw) {
            forbidden += 1;
            if raw.contains('&') {
                with_reference += 1;
            }
        } else if check_attribute(b"a", false, b"href", raw.as_bytes()).is_ok() {
            accepted += 1;
        }
        // A forbidden URL in a later srcset candidate: the whole value
        // alone would not show it.
        if raw.starts_with('/') && model_forbidden("img", false, "srcset", &raw) {
            later_candidate += 1;
        }
        if model_forbidden("a", false, "href", &raw) && !model_forbidden("img", false, "src", &raw)
        {
            raster += 1;
        }
    }
    assert!(forbidden > total / 10, "only {forbidden} forbidden values");
    assert!(
        with_reference > total / 20,
        "only {with_reference} forbidden values with a reference"
    );
    assert!(accepted > total / 10, "only {accepted} accepted values");
    assert!(
        later_candidate > total / 50,
        "only {later_candidate} forbidden later srcset candidates"
    );
    assert!(raster > total / 100, "only {raster} data: raster images");
}

/// The model itself on the rows of the result table of section 4.2 and the
/// trap of the audit: a model that finds nothing proves nothing.
#[test]
fn content_prop_model_reads_the_table() {
    for raw in [
        "javascript:x",
        " JavaScript:x",
        "vbscript:x",
        "data:text/html,x",
        " \tDa\nTa:x",
        "java&#x09;script:x",
        "&#106;avascript:x",
        "&Tab;javascript:x",
        "java&#x73\ncript:x",
    ] {
        assert!(model_forbidden("a", false, "href", raw), "{raw:?}");
    }
    for raw in [
        "?a=1&b=2",
        "data&amp;:x",
        "&COLON;x",
        "/javascript:x",
        "database:x",
        "java&#x73cript:x",
        // `java:` is no scheme of item 4; the rule rejects it by item 2
        // (stricter than the model, which is the allowed direction).
        "java&colon;x",
    ] {
        assert!(!model_forbidden("a", false, "href", raw), "{raw:?}");
    }
    assert!(!model_forbidden(
        "img",
        false,
        "src",
        "data:image/png;base64,AAAA"
    ));
    assert!(model_forbidden("img", false, "src", "data:image/svg+xml,x"));
    assert!(model_forbidden(
        "img",
        false,
        "srcset",
        "/a.png 1x,&#32;data:text/html,x 2x"
    ));
    assert!(!model_forbidden(
        "img",
        false,
        "srcset",
        "/a.png 1x, data:image/png,x 2x"
    ));
    assert!(!model_forbidden(
        "img",
        false,
        "srcset",
        "/a.png (1x, javascript:x) 2x"
    ));
}

// ---------------------------------------------------------------------------
// P-CONTENT-2: block content.

const BLOCK_START: &[u8] = b"<div data-rq-block=\"a\">";
const BLOCK_END: &[u8] = b"</div>";

fn analysis(blocks: &[(usize, usize, BlockFormat)]) -> RqpDocument {
    let base = split_rqp(
        b"<div data-rq-block=\"a\">x</div><script type=\"application/json\" data-rq-block=\"j\">{}</script>",
        &TemplateLimits::default(),
    )
    .unwrap();
    RqpDocument {
        template_revision: base.template_revision,
        blocks: blocks
            .iter()
            .map(|&(start, end, format)| RqpBlock {
                descriptor: base.blocks[usize::from(format == BlockFormat::Json)]
                    .descriptor
                    .clone(),
                start,
                end,
            })
            .collect(),
    }
}

fn limits(open: usize) -> TemplateLimits {
    let default = TemplateLimits::default();
    TemplateLimits::new(
        default.max_document_bytes(),
        default.max_blocks(),
        default.max_block_bytes(),
        MAX_LEGACY_NAME_BYTES,
        open,
    )
}

const TAG_PIECES: &[&str] = &[
    "<script>",
    "<SCRIPT/>",
    "<scr\0ipt>",
    "</script>",
    "<style>",
    "</style>",
    "<iframe>",
    "</iframe>",
    "<noscript>",
    "</noscript>",
    "<link>",
    "<Meta>",
    "<base/>",
    "<p onclick=x>",
    "<p ONCLICK=\"x\">",
    "<p title=a title=b>",
    "<p\tonload=x>",
    "<img/onerror=x>",
    "<svg/onload=x>",
    "<a href=\"javascript:x\">",
    "<img src=\"data:image/png,x\">",
    "<svg>",
    "</svg>",
    "<svg/>",
    "<math>",
    "</math>",
    "<g>",
    "</g>",
    "<a>",
    "</a>",
    "<foreignObject>",
    "<template>",
    "</template>",
    "<textarea>",
    "</textarea>",
    "<title>",
    "</title>",
    "<!--",
    "-->",
    "<![CDATA[",
    "]]>",
    "<",
    ">",
    "/",
    "=",
    "\"",
    "'",
    " ",
    "x",
    "on",
    "<p ",
    "<b>",
    "</b>",
    "<div>",
    "</div>",
    "&#106;",
    "\0",
];

fn content() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        3 => prop::collection::vec(prop::sample::select(TAG_PIECES), 0..24)
            .prop_map(|p| p.concat().into_bytes()),
        1 => prop::collection::vec(any::<u8>(), 0..200),
    ]
}

/// The independent reading of the rule over the tokens of the scan: the
/// first start tag in `[start, end)` that the rule must reject, `Err(())`
/// when the scan stops before `end`.
fn oracle(text: &[u8], start: usize, end: usize, limits: &TemplateLimits) -> Result<bool, ()> {
    let mut cursor = 0;
    for token in scan(text, limits) {
        if cursor >= end {
            break;
        }
        let token = token.map_err(|_| ())?;
        cursor = token.span().end;
        let Token::StartTag(tag) = token else {
            continue;
        };
        let at = tag.span().start;
        if at < start || at >= end {
            continue;
        }
        let name = tag.name().to_ascii_lowercase();
        let name_is = |list: &[&str]| list.iter().any(|n| n.as_bytes() == name.as_slice());
        let foreign = tag.in_foreign_region();
        if (foreign && !name_is(&FOREIGN_ALLOWED)) || (!foreign && name_is(&PATCH_FORBIDDEN)) {
            return Ok(true);
        }
        for attribute in tag.attributes() {
            let attribute = attribute.map_err(|_| ())?;
            let attribute_name = attribute.name().to_ascii_lowercase();
            if attribute_name.starts_with(b"on")
                || check_attribute(&name, foreign, &attribute_name, attribute.value()).is_err()
            {
                return Ok(true);
            }
        }
    }
    if cursor < end { Err(()) } else { Ok(false) }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    /// P-CONTENT-2.
    #[test]
    fn content_prop_accepted_has_no_forbidden(
        content in content(),
        open in prop_oneof![Just(256usize), 0..12usize],
    ) {
        let mut text = BLOCK_START.to_vec();
        let start = text.len();
        text.extend_from_slice(&content);
        let end = text.len();
        text.extend_from_slice(BLOCK_END);
        let limits = limits(open);
        let analysis = analysis(&[(start, end, BlockFormat::Html)]);
        let result = check_patch_content(&text, &analysis, &[0], &limits);
        let expected = oracle(&text, start, end, &limits);
        match result {
            Ok(()) => prop_assert_eq!(expected, Ok(false), "{:?}", String::from_utf8_lossy(&content)),
            Err(error) => {
                prop_assert_eq!(error.block, 0);
                prop_assert!(expected != Ok(false), "{:?}", String::from_utf8_lossy(&content));
            }
        }
    }

    /// Any document, any ranges, any indices: no panic.
    #[test]
    fn content_prop_any_ranges_never_panic(
        text in prop::collection::vec(any::<u8>(), 0..200),
        blocks in prop::collection::vec((0..240usize, 0..240usize, any::<bool>()), 0..5),
        patched in prop::collection::vec(0..7usize, 0..6),
        open in 0..8usize,
    ) {
        let blocks: Vec<(usize, usize, BlockFormat)> = blocks
            .into_iter()
            .map(|(s, e, html)| (s, e, if html { BlockFormat::Html } else { BlockFormat::Json }))
            .collect();
        let analysis = analysis(&blocks);
        let result = check_patch_content(&text, &analysis, &patched, &limits(open));
        if patched.is_empty() {
            prop_assert!(result.is_ok());
        }
        if let Err(error) = result {
            prop_assert!(patched.contains(&error.block));
        }
    }
}
