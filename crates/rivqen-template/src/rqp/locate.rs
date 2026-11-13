// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Manifest locator and head walk (WP-06 S2 contract; H-142).
//!
//! Normative text: docs/engineering/protocol/manifest-patch.md section 1.7,
//! client items 1 to 5, and "Document head" (H-72, H-79a).
//!
//! [`locate_manifest`] finds the manifest element of a full document F: the
//! first `script` start tag of the [`scan`](crate::rqp::scan) whose first
//! `type` attribute has the raw value `application/rivqen-manifest+json`
//! (ASCII case-insensitive, exactly 32 bytes, item 1). The manifest text M
//! is the bytes from the byte after its `>` to the `<` of the next
//! `</script` (item 4). The original document O is F without the bytes of
//! the element (item 5). The `<` of the start tag MUST be at the insertion
//! point P of the head walk on O (item 3, client rule C2); a walk on F is not
//! sufficient.
//!
//! The locator does not check that F has only one manifest element (item
//! 2): a second one is in O, and the markup analysis of O rejects it (M-13,
//! [`crate::rqp::full`]). The locator reads F only up to the first match, so
//! a document whose first match is not at P fails without a full scan.
//!
//! # One pass
//!
//! [`locate_manifest`] does not copy O. It runs the head walk on O in the
//! one scan of F:
//!
//! 1. Before the manifest start tag, the tokens of F are the tokens of O
//!    (the same bytes, read from the data state).
//! 2. The element starts in the data state, and the tokenizer is in the
//!    data state after the `>` of its end tag. The end tag pops the element
//!    that the start tag pushed, so the token stack is the one before the
//!    start tag. So the tokens of F after the element are the tokens of O
//!    after the offset of the start tag (DL-172). The walk takes them with
//!    their offsets less the length of the element.
//! 3. A gap of O that holds the offset of the start tag is two gaps of F.
//!    The walk tests each gap byte by byte, so the result is the same.
//! 4. The byte order mark rule depends on the first 3 bytes of O. When the
//!    start tag is at offset 0, 1 or 2, the walk reads the bytes before it
//!    again with the flag of O.
//!
//! The walk after the element stops at the first token that ends item 1 of
//! "Document head", so it reads only the next token for an accepted
//! document. The test `locate_prop_any_bytes` compares the result with the
//! walk on a copy of O. One difference is allowed: the scan of F pushes the
//! manifest `script` on the token stack, so when the stack is full before
//! the start tag, the scan of F stops (`Markup`) where O has no stop. When
//! the walk has not ended with a wrong position or no head, the open
//! elements before the start tag are `max_open_elements + 2` `html`
//! elements (and at most one `head`), the markup analysis of O rejects them
//! (M-26), and so [`crate::rqp::full::read_full_document`] rejects F in both
//! readings (test
//! `locate_position_full_stack_before_the_element_is_markup_and_o_is_invalid`).
//!
//! Every error is `MANIFEST_INVALID`. An error holds no document byte.

use core::fmt;
use core::ops::Range;

use rivqen_model::error::{ErrorCode, HasErrorCode};

use crate::limits::TemplateLimits;
use crate::rqp::MarkupError;
use crate::rqp::analysis::Internal as _;
use crate::rqp::scan::{Scanner, Span, Tag, Token, scan};

/// The place of the manifest element in a full document. Contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestLocation {
    element: Range<usize>,
    text: Range<usize>,
}

impl ManifestLocation {
    /// The bytes of the element: from the `<` of the start tag to the byte
    /// after the `>` of the end tag.
    #[must_use]
    pub fn element(&self) -> Range<usize> {
        self.element.clone()
    }

    /// The bytes of the manifest text M (item 4).
    #[must_use]
    pub fn text(&self) -> Range<usize> {
        self.text.clone()
    }

    /// M in `full`; `None` when `full` is not the document of this location.
    #[must_use]
    pub fn manifest_text<'a>(&self, full: &'a [u8]) -> Option<&'a [u8]> {
        full.get(self.text.clone())
    }

    /// The original document O: `full` without the bytes of the element
    /// (item 5); `None` when `full` is not the document of this location.
    #[must_use]
    pub fn original(&self, full: &[u8]) -> Option<Vec<u8>> {
        let before = full.get(..self.element.start)?;
        let after = full.get(self.element.end..)?;
        let mut out = Vec::with_capacity(before.len().saturating_add(after.len()));
        out.extend_from_slice(before);
        out.extend_from_slice(after);
        Some(out)
    }
}

/// Why a full document has no valid manifest element (section 1.7). The
/// code of each variant is `MANIFEST_INVALID`. Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LocateError {
    /// No manifest start tag in the document (item 2).
    NoManifest,
    /// No `</script` after the manifest start tag (item 4).
    Unterminated,
    /// O has no head start tag of the document and no implied head (item 3,
    /// "Document head" item 1).
    NoHead,
    /// The manifest start tag is not at the insertion point P of O (item 3).
    WrongPosition,
    /// The scan stopped before it found the manifest start tag (M-01, or
    /// the token stack limit).
    Markup(MarkupError),
}

impl LocateError {
    /// A static reason without document bytes.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoManifest => "the document has no manifest element",
            Self::Unterminated => "the manifest element has no end tag",
            Self::NoHead => "the document has no head start tag and no implied head",
            Self::WrongPosition => "the manifest element is not at the insertion point",
            Self::Markup(_) => "the scan of the document stopped",
        }
    }
}

impl fmt::Display for LocateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for LocateError {}

impl HasErrorCode for LocateError {
    fn code(&self) -> ErrorCode {
        ErrorCode::ManifestInvalid
    }
}

/// Find the manifest element of the full document `full` (section 1.7,
/// client items 1, 3, 4 and 5). Contract.
///
/// # Errors
///
/// [`LocateError`] for the first failed item.
pub fn locate_manifest(
    full: &[u8],
    limits: &TemplateLimits,
) -> Result<ManifestLocation, LocateError> {
    let mut tokens = scan(full, limits);
    // The tokens of F before the manifest start tag are the tokens of O
    // before the same offset, so the walk on O runs in the same pass. When
    // the start tag is at offset 3 or later, O and F have the same first 3
    // bytes, so the byte order mark rule is the same.
    let prefix_walk = HeadWalk::new(full.starts_with(BOM));
    let mut stop: Option<Option<usize>> = None;

    // Item 1: the first manifest start tag.
    let start = loop {
        let token = next_token(&mut tokens)?.ok_or(LocateError::NoManifest)?;
        if let Token::StartTag(tag) = &token
            && is_manifest_start_tag(tag).map_err(LocateError::Markup)?
        {
            break tag.span();
        }
        if stop.is_none()
            && let Walk::Stop(point) = prefix_walk
                .step(&token, full, 0)
                .map_err(LocateError::Markup)?
        {
            stop = Some(point);
        }
    };

    // Item 4: M ends at the `<` of the next `script` end tag of the scan.
    let end = loop {
        let token = next_token(&mut tokens)?.ok_or(LocateError::Unterminated)?;
        if let Token::EndTag(tag) = &token
            && tag.name().eq_ignore_ascii_case(SCRIPT)
        {
            break tag.span();
        }
    };
    let location = ManifestLocation {
        element: start.start..end.end,
        text: start.end..end.start,
    };

    // Item 5: O is F without the bytes `start.start..end.end`.
    let removed = end.end.checked_sub(start.start).ok_or_else(internal)?;
    let before = full.get(..start.start).ok_or_else(internal)?;
    let after = full.get(end.end..).ok_or_else(internal)?;
    let walk = HeadWalk::new(before.iter().chain(after).take(BOM.len()).eq(BOM));
    if start.start < BOM.len() {
        // The byte order mark rule of the at most 2 bytes before the start
        // tag depends on the bytes after the element: walk them again with
        // the flag of O.
        stop = None;
        for token in scan(before, limits) {
            let token = token.map_err(LocateError::Markup)?;
            if let Walk::Stop(point) = walk.step(&token, before, 0).map_err(LocateError::Markup)? {
                stop = Some(point);
                break;
            }
        }
    }

    // Item 3: the walk on O.
    let point = match stop {
        Some(point) => point,
        None => walk_after_element(walk, tokens, full, removed)?,
    };
    match point {
        None => Err(LocateError::NoHead),
        Some(point) if point == start.start => Ok(location),
        Some(_) => Err(LocateError::WrongPosition),
    }
}

/// The rest of the walk on O when it did not stop before the element. The
/// tokens of F after the element are the tokens of O after the offset of the
/// start tag: the element starts and ends in the data state, and its end tag
/// pops the element that its start tag pushed (DL-172). `removed` is the
/// length of the element.
fn walk_after_element(
    walk: HeadWalk,
    tokens: Scanner<'_>,
    full: &[u8],
    removed: usize,
) -> Result<Option<usize>, LocateError> {
    for token in tokens {
        let token = token.map_err(LocateError::Markup)?;
        if let Walk::Stop(point) = walk
            .step(&token, full, removed)
            .map_err(LocateError::Markup)?
        {
            return Ok(point);
        }
    }
    // Item 1: the end of the input.
    Ok(None)
}

/// The insertion point P of the head walk ("Document head", items 1 and 2)
/// on `document`: the byte after the `>` of the head start tag of the
/// document, or the `<` of the implied-head start tag. `Ok(None)` when the
/// document has neither. The server inserts the manifest at P; the client
/// computes P on O. Contract.
///
/// # Errors
///
/// [`MarkupError`] when the scan stops before the walk ends (M-01).
pub fn insertion_point(
    document: &[u8],
    limits: &TemplateLimits,
) -> Result<Option<usize>, MarkupError> {
    let walk = HeadWalk::new(document.starts_with(BOM));
    for token in scan(document, limits) {
        if let Walk::Stop(point) = walk.step(&token?, document, 0)? {
            return Ok(point);
        }
    }
    // Item 1: the end of the input.
    Ok(None)
}

/// The type of the manifest element (item 1): 32 bytes.
const MANIFEST_TYPE: &[u8] = b"application/rivqen-manifest+json";

/// The UTF-8 byte order mark. At offset 0 it counts as white space in the
/// head walk.
const BOM: &[u8] = b"\xEF\xBB\xBF";

const SCRIPT: &[u8] = b"script";

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

/// The end tags that are not ignored before the head (DL-034).
const NOT_IGNORED_END: [&[u8]; 4] = [b"head", b"body", b"html", b"br"];

/// An internal defect of this crate (an offset outside the input): fail
/// closed.
fn internal() -> LocateError {
    LocateError::Markup(MarkupError::internal())
}

fn is_one_of(name: &[u8], names: &[&[u8]]) -> bool {
    names.iter().any(|known| name.eq_ignore_ascii_case(known))
}

/// White space of the head walk: `09 0A 0C 0D 20`.
const fn is_white_space(byte: u8) -> bool {
    matches!(byte, 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

/// The next token of the scan; a scan stop is [`LocateError::Markup`].
fn next_token<'a>(tokens: &mut Scanner<'a>) -> Result<Option<Token<'a>>, LocateError> {
    tokens.next().transpose().map_err(LocateError::Markup)
}

/// Item 1: a `script` start tag whose first `type` attribute has the raw
/// value [`MANIFEST_TYPE`], ASCII case-insensitive. The lengths must be
/// equal, and a byte `80` or larger never matches an ASCII byte. A second
/// `type` attribute does not count.
fn is_manifest_start_tag(tag: &Tag<'_>) -> Result<bool, MarkupError> {
    if !tag.name().eq_ignore_ascii_case(SCRIPT) {
        return Ok(false);
    }
    for attribute in tag.attributes() {
        let attribute = attribute?;
        if attribute.name().eq_ignore_ascii_case(b"type") {
            return Ok(attribute.value().eq_ignore_ascii_case(MANIFEST_TYPE));
        }
    }
    Ok(false)
}

/// One step of the head walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Walk {
    /// The walk goes on with the next token.
    Continue,
    /// The walk ends: the insertion point, or `None` (no head start tag of
    /// the document and no implied head).
    Stop(Option<usize>),
}

/// The walk "Document head", item 1, one token at a time.
///
/// The walked document can differ from the scanned input by a removed
/// range: `shift` is the number of bytes to subtract from an offset of the
/// input to get the offset in the walked document (the length of the
/// manifest element when the walk reads O in the scan of F after the
/// element).
#[derive(Debug, Clone, Copy)]
struct HeadWalk {
    /// The walked document starts with a byte order mark.
    bom: bool,
}

impl HeadWalk {
    const fn new(bom: bool) -> Self {
        Self { bom }
    }

    fn step(self, token: &Token<'_>, input: &[u8], shift: usize) -> Result<Walk, MarkupError> {
        Ok(match token {
            Token::Gap(span) => {
                let bytes = input
                    .get(span.start..span.end)
                    .ok_or_else(MarkupError::internal)?;
                return self.gap(bytes, *span, shift);
            }
            // A `<![CDATA[` in HTML context is a bogus comment (M-03, M-04);
            // the walk runs only in HTML context.
            Token::Comment(_) | Token::Doctype(_) | Token::Cdata { .. } => Walk::Continue,
            Token::StartTag(tag) => {
                let name = tag.name();
                let span = tag.span();
                if name.eq_ignore_ascii_case(b"html") {
                    Walk::Continue
                } else if name.eq_ignore_ascii_case(b"head") {
                    Walk::Stop(Some(shifted(span.end, shift)?))
                } else if is_one_of(name, &IMPLIED_HEAD) {
                    Walk::Stop(Some(shifted(span.start, shift)?))
                } else {
                    Walk::Stop(None)
                }
            }
            Token::EndTag(tag) => {
                if is_one_of(tag.name(), &NOT_IGNORED_END) {
                    Walk::Stop(None)
                } else {
                    Walk::Continue
                }
            }
        })
    }

    /// A gap of white space continues the walk. The bytes at offsets 0 to 2
    /// of a walked document that starts with a byte order mark count as
    /// white space. `bytes` are the bytes of `span` in the input.
    fn gap(self, bytes: &[u8], span: Span, shift: usize) -> Result<Walk, MarkupError> {
        let start = shifted(span.start, shift)?;
        let white = bytes.iter().enumerate().all(|(index, &byte)| {
            is_white_space(byte)
                || (self.bom && start.checked_add(index).is_some_and(|at| at < BOM.len()))
        });
        Ok(if white {
            Walk::Continue
        } else {
            Walk::Stop(None)
        })
    }
}

/// An offset of the input as an offset of the walked document.
fn shifted(offset: usize, shift: usize) -> Result<usize, MarkupError> {
    offset.checked_sub(shift).ok_or_else(MarkupError::internal)
}
