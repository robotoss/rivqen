// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Port of `tools/rqp-markup/rust/src/scan.rs` (commit `92c708e1`; DL-161).
//! Keep the rules equal to the candidate: a change of the markup rules
//! changes both copies (WP-06 S1, risk R-2).
//!
//! The tokens of a document, as [`crate::rqp::split_rqp`] reads them (`markup.md`
//! section 2.3).
//!
//! A server needs the tokens for two rules that are not markup rules:
//!
//! - the walk "Document head" of `manifest-patch.md` section 1.7 (where the
//!   manifest goes, H-72, H-79a): tokens and gaps from offset 0;
//! - the patch content rule of `manifest-patch.md` section 4 item 1 and the
//!   attribute rule of section 4.2: every start tag of the content of an
//!   `html` block, its HTML context or foreign region, and every raw
//!   attribute.
//!
//! [`scan`] gives these tokens. It does not apply the rules.
//!
//! # One tokenizer
//!
//! The tokens come from the tokenizer of [`crate::rqp::split_rqp`], with the same
//! state changes of M-04: after a start tag in HTML context, the name decides
//! the new state (RCDATA, RAWTEXT, script data or PLAINTEXT); in a foreign
//! region no start tag changes the state. The attributes of a tag come from
//! the same attribute states that `analyze` uses. So `analyze` and this
//! module cannot read a tag, a comment or a raw text in two ways.
//!
//! The scan keeps the token stack of M-05 as `markup.md` defines it, to know
//! the foreign region. For a document that `analyze` accepts, the foreign
//! region of each token is the one of `analyze`. (`analyze` also closes table
//! parts by M-09; this changes only HTML elements outside block content, so
//! it never changes a foreign region of a valid document.)
//!
//! # Coverage
//!
//! The tokens and the gaps cover the input exactly once, in order: each
//! [`Token::span`] starts where the one before it ends, the first starts at
//! offset 0, and the last ends at the input length. A gap is never empty.
//!
//! # Block content
//!
//! Scan the **whole document**, then use the start tags whose span is inside
//! the content `[start, end)` of a block. These are the tokens of the content
//! in the analysis of the document, as `manifest-patch.md` section 4 item 1
//! and section 4.2 require: the content of a `title` block is RCDATA and the
//! content of a `json` block is script data, so both have no start tags.
//! Do not scan the content bytes alone: a scan starts in the data state, so
//! it reads the RCDATA of a `title` block as markup.
//!
//! # Resource limits
//!
//! The work is linear in the input size. The memory is linear in the input
//! size: the token stack holds at most one element per start tag, and the
//! name table one entry per distinct tag name. [`Tag::attributes`] holds no
//! memory that grows with the input; each call reads the tag again (linear
//! in the tag length). No input panics. The scan does not check M-01 or
//! M-02: call [`crate::rqp::split_rqp`] first.
//!
//! # Example
//!
//! ```
//! use rivqen_template::rqp::scan::{scan, Token};
//!
//! let input = b"<p a=1 A=2>x</p>";
//! let mut names = Vec::new();
//! for token in scan(input) {
//!     match token? {
//!         Token::StartTag(tag) => {
//!             for attribute in tag.attributes() {
//!                 names.push(attribute?.name().to_vec());
//!             }
//!         }
//!         _ => {}
//!     }
//! }
//! // Both attributes, also the duplicate that the tokenizer drops (M-03).
//! assert_eq!(names, [b"a".to_vec(), b"A".to_vec()]);
//! # Ok::<(), rivqen_template::rqp::MarkupError>(())
//! ```

use std::iter::FusedIterator;

use crate::rqp::MarkupError;
use crate::rqp::analysis::Internal as _;
use crate::rqp::names::{Known, NameId, NameTable, flag, raw_kind};
use crate::rqp::tokenizer::{self, RawAttr, TagStep, TagWalk, Tokenizer};

pub use crate::rqp::tokenizer::Span;

/// The tokens and gaps of `input`, from offset 0, in the data state, with an
/// empty token stack (HTML context).
///
/// A UTF-8 byte order mark at offset 0 is part of the first gap. (The
/// tokenizer of [`crate::rqp::split_rqp`] starts after it; the bytes `EF BB BF` are
/// never a token, so the tokens are the same.)
#[must_use]
pub fn scan(input: &[u8]) -> Scanner<'_> {
    Scanner {
        input,
        tokens: Tokenizer::new(input, 0),
        stack: TokenStack::new(),
        cursor: 0,
        pending: None,
        done: false,
    }
}

/// One token or one gap of a [`scan`].
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token<'a> {
    /// The bytes between two tokens, or before the first or after the last
    /// token. They are character data (also the raw text of RCDATA,
    /// RAWTEXT, script data and PLAINTEXT, and a `<` that does not start a
    /// tag), the bytes `</>` (which give no token), and the bytes of a tag
    /// that the end of the input cuts (which give no token). This is the
    /// gap of `manifest-patch.md` section 1.7. Never empty.
    Gap(Span),
    /// A start tag (from `<` to the byte after `>`).
    StartTag(Tag<'a>),
    /// An end tag. Its attributes and its self-closing flag have no effect
    /// (M-03).
    EndTag(Tag<'a>),
    /// A comment, a bogus comment or a processing instruction (M-03). It
    /// ends at the end of the input when nothing ends it before.
    Comment(Span),
    /// A DOCTYPE token. It ends at the first `>` or at the end of the input.
    Doctype(Span),
    /// The exact `<![CDATA[` and the bytes up to the first `>` (or the end
    /// of the input). In HTML context the WHATWG tokenizer reads it as a
    /// bogus comment, in a foreign region as a CDATA section (M-03, M-04).
    /// `well_formed` is the M-12 condition: then both readings end at the
    /// end of `span`. `analyze` rejects a document where it is false.
    Cdata {
        /// From `<` to the byte after the first `>`, or to the end.
        span: Span,
        /// The `>` that ends `span` is the end of a `]]>` (or there is no
        /// `>`).
        well_formed: bool,
    },
}

impl Token<'_> {
    /// The bytes of the input that this token or gap covers.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Gap(span)
            | Self::Comment(span)
            | Self::Doctype(span)
            | Self::Cdata { span, .. } => *span,
            Self::StartTag(tag) | Self::EndTag(tag) => tag.span,
        }
    }
}

/// A start tag or an end tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tag<'a> {
    input: &'a [u8],
    span: Span,
    name_span: Span,
    name: &'a [u8],
    self_closing: bool,
    foreign: bool,
}

impl<'a> Tag<'a> {
    /// From the `<` to the byte after the `>`.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }

    /// The raw tag name, as in the input. The tokenizer converts ASCII upper
    /// case to lower case (and U+0000 to U+FFFD), so compare names ASCII
    /// case-insensitively: `name().eq_ignore_ascii_case(b"head")`.
    #[must_use]
    pub fn name(&self) -> &'a [u8] {
        self.name
    }

    /// The span of [`Tag::name`].
    #[must_use]
    pub fn name_span(&self) -> Span {
        self.name_span
    }

    /// The tag ends with `/>`.
    #[must_use]
    pub fn self_closing(&self) -> bool {
        self.self_closing
    }

    /// The tag is in a foreign region (M-05 item 4): the token stack before
    /// this tag holds a region root. A region root itself (an `svg` or
    /// `math` start tag in HTML context) is in HTML context; the tags after
    /// it, up to the end tag that pops it, are in its foreign region.
    #[must_use]
    pub fn in_foreign_region(&self) -> bool {
        self.foreign
    }

    /// Every attribute of the tag in source order, also each duplicate that
    /// the tokenizer drops (M-03). The values are raw (`markup.md` section
    /// 2.1): character references are not decoded. Each call reads the tag
    /// again with the attribute states of the tokenizer.
    #[must_use]
    pub fn attributes(&self) -> Attributes<'a> {
        Attributes {
            input: self.input,
            walk: Some(TagWalk::new(self.input, self.name_span.end)),
        }
    }
}

/// One raw attribute of a tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attribute<'a> {
    name: &'a [u8],
    name_span: Span,
    value: &'a [u8],
    value_span: Option<Span>,
}

impl<'a> Attribute<'a> {
    /// The raw attribute name, as in the input. Compare it ASCII
    /// case-insensitively, as for [`Tag::name`].
    #[must_use]
    pub fn name(&self) -> &'a [u8] {
        self.name
    }

    /// The span of [`Attribute::name`].
    #[must_use]
    pub fn name_span(&self) -> Span {
        self.name_span
    }

    /// The raw value: the bytes between the quotes, or the bytes of an
    /// unquoted value. Empty when the attribute has no value.
    #[must_use]
    pub fn value(&self) -> &'a [u8] {
        self.value
    }

    /// The span of [`Attribute::value`]; `None` when the attribute has no
    /// value (no `=`, or `=` directly before the `>` that ends the tag).
    #[must_use]
    pub fn value_span(&self) -> Option<Span> {
        self.value_span
    }
}

/// The attributes of one [`Tag`], from [`Tag::attributes`].
///
/// An item is an error only for an internal defect of this crate (an offset
/// outside the input). The caller must then fail closed.
#[derive(Clone, Debug)]
pub struct Attributes<'a> {
    input: &'a [u8],
    walk: Option<TagWalk<'a>>,
}

impl<'a> Iterator for Attributes<'a> {
    type Item = Result<Attribute<'a>, MarkupError>;

    fn next(&mut self) -> Option<Self::Item> {
        let walk = self.walk.as_mut()?;
        match walk.step() {
            TagStep::Attr(raw) => {
                let item = attribute(self.input, raw);
                if item.is_err() {
                    self.walk = None;
                }
                Some(item)
            }
            TagStep::End { .. } => {
                self.walk = None;
                None
            }
            // The tag was read to its `>` before: the end of the input here
            // is a defect.
            TagStep::Eof => {
                self.walk = None;
                Some(Err(MarkupError::internal()))
            }
        }
    }
}

impl FusedIterator for Attributes<'_> {}

fn bytes(input: &[u8], span: Span) -> Result<&[u8], MarkupError> {
    input
        .get(span.start..span.end)
        .ok_or_else(MarkupError::internal)
}

fn attribute(input: &[u8], raw: RawAttr) -> Result<Attribute<'_>, MarkupError> {
    let value = match raw.value {
        Some(span) => bytes(input, span)?,
        None => &[],
    };
    Ok(Attribute {
        name: bytes(input, raw.name)?,
        name_span: raw.name,
        value,
        value_span: raw.value,
    })
}

/// The token stack of M-05, only as far as the foreign region needs it.
#[derive(Debug)]
struct TokenStack {
    names: NameTable,
    /// The name of each open element and the stack index of the previous
    /// open element with the same name.
    elements: Vec<(NameId, Option<usize>)>,
    /// Stack index of the region root (M-05 item 4).
    root: Option<usize>,
}

impl TokenStack {
    fn new() -> Self {
        Self {
            names: NameTable::new(),
            elements: Vec::new(),
            root: None,
        }
    }

    fn push(&mut self, name: NameId) -> usize {
        let index = self.elements.len();
        let previous = self.names.replace_top(name, Some(index));
        self.elements.push((name, previous));
        index
    }

    /// M-05 items 1 and 2.
    fn start_tag(&mut self, raw_name: &[u8], self_closing: bool) {
        let name = self.names.intern(raw_name);
        if self.root.is_some() {
            if !self_closing {
                self.push(name);
            }
            return;
        }
        let is_root = name == Known::Svg.id() || name == Known::Math.id();
        if (is_root && self_closing) || self.names.has(name, flag::VOID) {
            return;
        }
        let index = self.push(name);
        if is_root {
            self.root = Some(index);
        }
    }

    /// M-05 item 3: pops the nearest element with the name and every element
    /// above it. An end tag without such an element is ignored. Every
    /// element is popped at most once, so the work stays linear.
    fn end_tag(&mut self, raw_name: &[u8]) {
        let Some(target) = self
            .names
            .lookup(raw_name)
            .and_then(|name| self.names.top(name))
        else {
            return;
        };
        while self.elements.len() > target {
            let Some((name, previous)) = self.elements.pop() else {
                return;
            };
            self.names.replace_top(name, previous);
            if self.root == Some(self.elements.len()) {
                self.root = None;
            }
        }
    }
}

/// The iterator of [`scan`].
///
/// An item is an error only for an internal defect of this crate (an offset
/// outside the input, or tokens out of order). The scan then ends; the
/// caller must fail closed.
#[derive(Debug)]
pub struct Scanner<'a> {
    input: &'a [u8],
    tokens: Tokenizer<'a>,
    stack: TokenStack,
    /// Offset after the last token or gap given out.
    cursor: usize,
    /// A token that follows the gap given out last.
    pending: Option<Token<'a>>,
    done: bool,
}

impl<'a> Scanner<'a> {
    fn tag(&self, tag: &tokenizer::Tag, foreign: bool) -> Result<Tag<'a>, MarkupError> {
        Ok(Tag {
            input: self.input,
            span: tag.span,
            name_span: tag.name,
            name: bytes(self.input, tag.name)?,
            self_closing: tag.self_closing,
            foreign,
        })
    }

    /// The next token of the tokenizer, after the M-04 state change and the
    /// M-05 step it gives. `None` at the end of the input.
    fn next_token(&mut self) -> Result<Option<Token<'a>>, MarkupError> {
        let token = self.tokens.next_token();
        if self.tokens.defect() {
            return Err(MarkupError::internal());
        }
        let foreign = self.stack.root.is_some();
        Ok(Some(match token {
            tokenizer::Token::StartTag(tag) => {
                let out = self.tag(&tag, foreign)?;
                self.stack.start_tag(out.name, tag.self_closing);
                // M-04: in HTML context only, also with the self-closing flag.
                if !foreign && let Some((kind, end_name)) = raw_kind(out.name) {
                    self.tokens.set_raw(kind, end_name);
                }
                Token::StartTag(out)
            }
            tokenizer::Token::EndTag(tag) => {
                let out = self.tag(&tag, foreign)?;
                self.stack.end_tag(out.name);
                Token::EndTag(out)
            }
            tokenizer::Token::Comment(span) => Token::Comment(span),
            tokenizer::Token::Doctype(span) => Token::Doctype(span),
            tokenizer::Token::Cdata { span, well_formed } => Token::Cdata { span, well_formed },
            tokenizer::Token::Eof => return Ok(None),
        }))
    }

    fn advance(&mut self) -> Result<Option<Token<'a>>, MarkupError> {
        if let Some(token) = self.pending.take() {
            return Ok(Some(token));
        }
        let Some(token) = self.next_token()? else {
            self.done = true;
            let len = self.input.len();
            if self.cursor < len {
                let gap = Span {
                    start: self.cursor,
                    end: len,
                };
                self.cursor = len;
                return Ok(Some(Token::Gap(gap)));
            }
            return Ok(None);
        };
        let span = token.span();
        // Tokens are in order, not empty and inside the input; else this is
        // a defect.
        if span.start < self.cursor || span.end <= span.start || span.end > self.input.len() {
            return Err(MarkupError::internal());
        }
        let gap_start = self.cursor;
        self.cursor = span.end;
        if span.start > gap_start {
            self.pending = Some(token);
            return Ok(Some(Token::Gap(Span {
                start: gap_start,
                end: span.start,
            })));
        }
        Ok(Some(token))
    }
}

impl<'a> Iterator for Scanner<'a> {
    type Item = Result<Token<'a>, MarkupError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done && self.pending.is_none() {
            return None;
        }
        match self.advance() {
            Ok(token) => token.map(Ok),
            Err(error) => {
                self.done = true;
                self.pending = None;
                Some(Err(error))
            }
        }
    }
}

impl FusedIterator for Scanner<'_> {}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;
    use crate::limits::TemplateLimits;
    use crate::rqp::rules::RuleLimits;
    use proptest::prelude::*;

    /// (kind, span, foreign) of the tokens without the gaps.
    type Seen = Vec<(&'static str, Span, bool)>;

    fn kind(token: &tokenizer::Token) -> Option<(&'static str, Span)> {
        Some(match token {
            tokenizer::Token::StartTag(tag) => ("start", tag.span),
            tokenizer::Token::EndTag(tag) => ("end", tag.span),
            tokenizer::Token::Comment(span) => ("comment", *span),
            tokenizer::Token::Doctype(span) => ("doctype", *span),
            tokenizer::Token::Cdata { span, .. } => ("cdata", *span),
            tokenizer::Token::Eof => return None,
        })
    }

    /// The tokens that the pass of `analyze` reads, with its foreign region,
    /// up to the token where it stops.
    fn analyze_tokens(input: &[u8]) -> (Seen, bool) {
        let start = if input.starts_with(crate::rqp::analysis::BOM) {
            3
        } else {
            0
        };
        let mut seen = Vec::new();
        let limits = RuleLimits::new(&TemplateLimits::default());
        let valid = crate::rqp::rules::run(input, start, limits, |token, foreign| {
            if let Some((k, span)) = kind(token) {
                seen.push((k, span, foreign));
            }
        })
        .is_ok();
        (seen, valid)
    }

    fn scan_tokens(input: &[u8]) -> Seen {
        scan(input)
            .filter_map(|token| match token.unwrap() {
                Token::Gap(_) => None,
                Token::StartTag(tag) => Some(("start", tag.span(), tag.in_foreign_region())),
                Token::EndTag(tag) => Some(("end", tag.span(), tag.in_foreign_region())),
                Token::Comment(span) => Some(("comment", span, false)),
                Token::Doctype(span) => Some(("doctype", span, false)),
                Token::Cdata { span, .. } => Some(("cdata", span, false)),
            })
            .collect()
    }

    /// The scan and the pass of `analyze` read the same tokens with the same
    /// foreign region: all of them for a valid document, and every token up
    /// to the one where `analyze` stops for an invalid one.
    fn assert_same_tokens(input: &[u8]) {
        let (mut seen, valid) = analyze_tokens(input);
        let mut scanned = scan_tokens(input);
        // Only tags carry a context in the scan.
        for token in &mut seen {
            if token.0 != "start" && token.0 != "end" {
                token.2 = false;
            }
        }
        if valid {
            assert_eq!(scanned, seen, "{:?}", String::from_utf8_lossy(input));
        } else {
            scanned.truncate(seen.len());
            assert_eq!(scanned, seen, "{:?}", String::from_utf8_lossy(input));
        }
    }

    #[test]
    fn scan_reads_the_tokens_of_analyze() {
        for input in [
            &b"<!DOCTYPE html><html><head><title>a<b></title></head><body></body></html>"[..],
            b"<div data-rq-block=\"a\"><svg><title><p></title><g/></svg><style><p></style></div>",
            b"<math><mi><script>x</script></mi></math><script></script><p>",
            b"<svg/><style><b></style><math/><script><b></script>",
            b"<svg><style><b></style></svg><textarea><b></textarea>",
            // M-09 closure: `analyze` pops the `caption`; the scan keeps it.
            b"<table><caption><tr><td><svg><g></g></svg></td></tr></table><xmp><b></xmp>",
            b"<p><noscript><svg><b></svg></noscript></p><b>",
            b"\xEF\xBB\xBF<p>x</p>",
        ] {
            assert_same_tokens(input);
        }
    }

    #[test]
    fn region_root_end_ends_the_region() {
        let foreign: Vec<bool> = scan(b"<svg><g><style></g></svg><style><g></style>")
            .filter_map(|t| match t.unwrap() {
                Token::StartTag(tag) => Some(tag.in_foreign_region()),
                _ => None,
            })
            .collect();
        assert_eq!(foreign, [false, true, true, false]);
    }

    /// An end tag in a foreign region that matches an element below the
    /// region root pops the root (M-05 item 3): the region ends, and the
    /// next `style` is RAWTEXT again.
    #[test]
    fn end_tag_below_the_root_ends_the_region() {
        let input = b"<b><svg><g></b><style><i></style>";
        let tags: Vec<(String, bool)> = scan(input)
            .filter_map(|t| match t.unwrap() {
                Token::StartTag(tag) | Token::EndTag(tag) => Some((
                    String::from_utf8_lossy(tag.name()).into_owned(),
                    tag.in_foreign_region(),
                )),
                _ => None,
            })
            .collect();
        let expected = [
            ("b", false),
            ("svg", false),
            ("g", true),
            ("b", true),
            ("style", false),
            ("style", false),
        ];
        let expected: Vec<(String, bool)> = expected
            .iter()
            .map(|(n, f)| ((*n).to_owned(), *f))
            .collect();
        assert_eq!(tags, expected);
    }

    #[test]
    fn a_no_match_end_tag_keeps_the_region() {
        let foreign: Vec<bool> = scan(b"<svg></x><g>")
            .filter_map(|t| match t.unwrap() {
                Token::StartTag(tag) => Some(tag.in_foreign_region()),
                _ => None,
            })
            .collect();
        assert_eq!(foreign, [false, true]);
    }

    #[test]
    fn a_defect_span_fails_closed() {
        assert!(
            bytes(b"abc", Span { start: 2, end: 4 })
                .unwrap_err()
                .is_internal()
        );
        let raw = RawAttr {
            name: Span { start: 0, end: 1 },
            value: Some(Span { start: 2, end: 9 }),
        };
        assert!(attribute(b"abc", raw).unwrap_err().is_internal());
        let raw = RawAttr {
            name: Span { start: 0, end: 9 },
            value: None,
        };
        assert!(attribute(b"abc", raw).unwrap_err().is_internal());
    }

    /// Pieces that reach the M-04 state changes and the foreign region.
    const PIECES: &[&str] = &[
        "<div data-rq-block=\"a\">",
        "</div>",
        "<title data-rq-block=\"t\">",
        "</title>",
        "<script type=\"application/json\" data-rq-block=\"j\">",
        "</script>",
        "<svg>",
        "</svg>",
        "<svg/>",
        "<math>",
        "</math>",
        "<math/>",
        "<g>",
        "</g>",
        "<mi>",
        "</mi>",
        "<title>",
        "<style>",
        "</style>",
        "<script>",
        "<textarea>",
        "</textarea>",
        "<xmp>",
        "</xmp>",
        "<noscript>",
        "</noscript>",
        "<plaintext>",
        "<table>",
        "<caption>",
        "</caption>",
        "<tr>",
        "<td>",
        "</td>",
        "</table>",
        "<b>",
        "</b>",
        "<p>",
        "</p>",
        "<br>",
        "<!-- c -->",
        "<![CDATA[x]]>",
        "</>",
        "<",
        "x",
        " ",
        "\u{feff}",
        "<a b=\"<style>\" c='>'>",
    ];

    fn soup() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(prop::sample::select(PIECES), 0..40)
            .prop_map(|pieces| pieces.concat().into_bytes())
    }

    fn pick(items: &'static [&'static str]) -> impl Strategy<Value = String> + Clone {
        prop::sample::select(items).prop_map(str::to_owned)
    }

    /// Balanced foreign content: shapes, text-only elements and groups.
    fn foreign_tree() -> impl Strategy<Value = String> {
        let leaf = pick(&[
            "x",
            "<path/>",
            "<circle></circle>",
            "<title>a &lt; b</title>",
            "<style>a{}</style>",
            "<desc>a < b</desc>",
            "<mi>x</mi>",
            "<![CDATA[x]]>",
        ]);
        leaf.prop_recursive(3, 24, 4, |inner| {
            (
                pick(&["g", "symbol", "text", "mrow"]),
                prop::collection::vec(inner, 0..4),
            )
                .prop_map(|(name, children)| format!("<{name}>{}</{name}>", children.concat()))
        })
    }

    /// Balanced HTML with regions and raw text whose bytes look like tags.
    fn html_tree() -> impl Strategy<Value = String> {
        let raw = (
            pick(&["style", "script", "xmp", "textarea", "title", "iframe"]),
            prop::collection::vec(
                pick(&["<b>", "</svg>", "<svg>", "x", "</g>", "<math>"]),
                0..3,
            ),
        )
            .prop_map(|(name, body)| format!("<{name}>{}</{name}>", body.concat()));
        let region = (
            pick(&["svg", "math"]),
            prop::collection::vec(foreign_tree(), 0..4),
        )
            .prop_map(|(root, children)| format!("<{root}>{}</{root}>", children.concat()));
        let leaf = prop_oneof![
            raw,
            region,
            pick(&[
                "x",
                "<br>",
                "<svg/>",
                "<math/>",
                "<!-- c -->",
                "</>",
                "a < b"
            ]),
        ];
        leaf.prop_recursive(4, 48, 4, |inner| {
            (
                pick(&["div", "span", "section", "b"]),
                prop::collection::vec(inner, 0..4),
            )
                .prop_map(|(name, children)| format!("<{name}>{}</{name}>", children.concat()))
        })
    }

    /// Mostly valid documents, with or without a block around the content.
    fn balanced() -> impl Strategy<Value = Vec<u8>> {
        (
            prop::collection::vec(html_tree(), 0..4),
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(|(trees, block, head)| {
                let body = trees.concat();
                let body = if block {
                    format!("<div data-rq-block=\"a\">{body}</div>")
                } else {
                    body
                };
                let head = if head {
                    "<!DOCTYPE html><html><head><title>t</title></head>"
                } else {
                    ""
                };
                format!("{head}{body}").into_bytes()
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(3000))]

        #[test]
        fn scan_and_analyze_read_the_same_tokens(input in soup()) {
            assert_same_tokens(&input);
        }

        #[test]
        fn scan_and_analyze_read_the_same_tokens_of_balanced_documents(input in balanced()) {
            assert_same_tokens(&input);
        }

        #[test]
        fn scan_and_analyze_read_the_same_tokens_of_any_bytes(
            input in prop::collection::vec(any::<u8>(), 0..300)
        ) {
            assert_same_tokens(&input);
        }
    }

    /// Guard against a generator that never reaches the cases that matter:
    /// valid documents with a foreign region, and with a raw text after it.
    #[test]
    fn generator_reaches_valid_documents_with_foreign_regions() {
        use proptest::strategy::ValueTree as _;
        let mut runner = proptest::test_runner::TestRunner::deterministic();
        let (mut foreign, mut raw_after) = (0, 0);
        for _ in 0..3000 {
            let input = balanced().new_tree(&mut runner).unwrap().current();
            let (seen, valid) = analyze_tokens(&input);
            let Some(first) = seen.iter().position(|t| t.2) else {
                continue;
            };
            if valid {
                foreign += 1;
                let text = String::from_utf8_lossy(&input);
                let after = &text[seen[first].1.start..];
                if ["<style>", "<script>", "<xmp>", "<textarea>", "<title>"]
                    .iter()
                    .any(|t| after.contains(t))
                {
                    raw_after += 1;
                }
            }
        }
        assert!(
            foreign > 500,
            "only {foreign} valid documents with a region"
        );
        assert!(raw_after > 400, "only {raw_after} with a raw text after it");
    }
}
