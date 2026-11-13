// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Patch content rule and attribute rule (WP-06 S2 contract; H-142, R-1).
//!
//! Normative text: docs/engineering/protocol/manifest-patch.md section 4,
//! item 1 (H-48, H-73, H-78, H-96), section 4.2 (H-59, H-77, H-83, H-83a,
//! DL-036, DL-048, DL-051, DL-052) and section 2.4, step 9.
//!
//! This is the XSS boundary of a patch (RSK-07): a gap lets a
//! server-controlled block run script. The rule fails closed.
//!
//! [`check_patch_content`] reads the tokens of the content of each patched
//! `html` block of the patched document with the [`scan`](crate::rqp::scan)
//! (the tokenizer of the analysis: M-04 state changes in HTML context, none
//! in a foreign region). For each start tag whose `<` is in the content
//! range of a patched `html` block it checks:
//!
//! 1. a start tag in HTML context whose name is in [`PATCH_FORBIDDEN`];
//! 2. a start tag in a foreign region whose name is not in
//!    [`FOREIGN_ALLOWED`];
//! 3. each attribute, also a duplicate that the tokenizer drops (M-03), with
//!    [`check_attribute`] on the raw value.
//!
//! The self-closing flag does not change the result; end tags do not
//! matter; names are compared in lower case, as the tokenizer gives them.
//! `json` blocks and the blocks without an operation are not read.
//!
//! Every violation is `PATCH_CONTENT_REJECTED`. A violation holds the block
//! index and the kind only, never content bytes, names or values.
//!
//! # Fail closed
//!
//! The rule rejects with [`ContentViolation::Markup`] when it cannot read
//! the content: a patched index without a block, a content range outside
//! the document, two patched `html` ranges that overlap, a scan stop (M-01,
//! the token stack limit, an internal defect) before the end of the last
//! patched `html` block, and an attribute walk that reports a defect.
//!
//! # Resource limits
//!
//! One scan pass from offset 0 to the end of the last patched `html` block;
//! each start tag of the patched content reads its attributes once; each
//! attribute value is read a constant number of times (the `srcset` split
//! included). So the work is linear in the length of the document. Memory:
//! the list of the patched `html` ranges (at most `patched.len()` entries)
//! and two name buffers that hold a lower-case copy of a name with an ASCII
//! upper-case letter (at most the longest such name). The attribute rule
//! allocates nothing.
//!
//! # Character references
//!
//! Item 2 and item 4.3 read numeric character references on the raw bytes
//! (markup M-13), never on the copy of item 3: in `java&#x73<LF>cript:x`
//! the reference is `&#x73` (`s`), not `&#x73c` (the trap of the audit).

use core::fmt;

use rivqen_model::error::{ErrorCode, HasErrorCode};

use crate::limits::TemplateLimits;
use rivqen_proto::types::BlockFormat;

use crate::rqp::analysis::Internal as _;
use crate::rqp::scan::{Tag, Token, scan};
use crate::rqp::{MarkupError, RqpDocument};

/// Start tag names that a patch cannot bring in HTML context (section 4,
/// item 1.1; H-78, H-96). Contract.
pub const PATCH_FORBIDDEN: [&str; 11] = [
    "base", "basefont", "bgsound", "embed", "iframe", "link", "meta", "noscript", "object",
    "script", "style",
];

/// Start tag names that a patch can bring in a foreign region (section 4,
/// `FOREIGN_ALLOWED`; H-73). Every other name rejects. Contract.
pub const FOREIGN_ALLOWED: [&str; 70] = [
    "svg",
    "g",
    "path",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "rect",
    "defs",
    "symbol",
    "use",
    "title",
    "desc",
    "lineargradient",
    "radialgradient",
    "stop",
    "clippath",
    "mask",
    "pattern",
    "marker",
    "text",
    "tspan",
    "filter",
    "feblend",
    "fecolormatrix",
    "fecomponenttransfer",
    "fecomposite",
    "fedisplacementmap",
    "fedropshadow",
    "feflood",
    "fefunca",
    "fefuncb",
    "fefuncg",
    "fefuncr",
    "fegaussianblur",
    "femerge",
    "femergenode",
    "femorphology",
    "feoffset",
    "feturbulence",
    "math",
    "mrow",
    "mi",
    "mo",
    "mn",
    "ms",
    "mtext",
    "mspace",
    "msub",
    "msup",
    "msubsup",
    "mfrac",
    "msqrt",
    "mroot",
    "mstyle",
    "munder",
    "mover",
    "munderover",
    "mmultiscripts",
    "mprescripts",
    "none",
    "mtable",
    "mtr",
    "mtd",
    "mpadded",
    "mphantom",
    "merror",
    "semantics",
    "annotation",
];

/// The kind of a violation of the patch content rule. Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContentViolation {
    /// Item 1.1: a start tag in HTML context with a name in
    /// [`PATCH_FORBIDDEN`].
    ForbiddenElement,
    /// Item 1.2: a start tag in a foreign region with a name that is not in
    /// [`FOREIGN_ALLOWED`].
    ForeignElement,
    /// Section 4.2, item 1: an attribute name that starts with `on`.
    EventHandler,
    /// Section 4.2, item 2: a character reference at the scheme decision
    /// byte that can continue or complete a scheme (H-77).
    CharacterReference,
    /// Section 4.2, item 4.1 or 4.2: a `javascript:`, `vbscript:` or
    /// `data:` value (H-59, H-83).
    Scheme,
    /// Section 4.2, item 4.3: a `srcset` with a character reference that
    /// decodes to white space, `,`, `(` or `)`, or with a candidate URL that
    /// items 2 to 4.2 reject (H-83).
    Srcset,
    /// The scan of the patched document stopped (M-01 or the token stack
    /// limit) before the end of a patched block: the rule cannot read the
    /// content, so it rejects (fail closed).
    Markup(MarkupError),
}

impl ContentViolation {
    /// A static reason without content.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ForbiddenElement => "the content has an element that a patch cannot bring",
            Self::ForeignElement => "the content has a foreign element outside the allowed set",
            Self::EventHandler => "the content has an event-handler attribute",
            Self::CharacterReference => {
                "the content has an attribute with a character reference at the scheme"
            }
            Self::Scheme => "the content has an attribute with a forbidden URL scheme",
            Self::Srcset => "the content has a srcset that the rule rejects",
            Self::Markup(_) => "the scan of the patched document stopped",
        }
    }
}

impl fmt::Display for ContentViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for ContentViolation {}

impl HasErrorCode for ContentViolation {
    fn code(&self) -> ErrorCode {
        ErrorCode::PatchContentRejected
    }
}

/// A violation in one patched block. Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentError {
    /// The index of the block in the blocks of the analysis.
    pub block: usize,
    /// The first violation in the block, in document order.
    pub violation: ContentViolation,
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "block {}: {}", self.block, self.violation)
    }
}

impl core::error::Error for ContentError {}

impl HasErrorCode for ContentError {
    fn code(&self) -> ErrorCode {
        ErrorCode::PatchContentRejected
    }
}

/// The patch content rule (section 2.4, step 9) on the patched document
/// `document`. `analysis` is the markup analysis of `document` (section 2.4,
/// step 8) and `patched` the indices of the blocks with an operation, in
/// any order. An index that names no block, or a block whose content range
/// is not in `document`, rejects with [`ContentViolation::Markup`]. The
/// first violation in document order wins. Contract.
///
/// `analysis` MUST be the result of [`split_rqp`](crate::rqp::split_rqp)
/// on exactly these `document` bytes: with the block ranges of another
/// document (for example of the stored original) the rule reads other
/// bytes than the new content. A session calls
/// [`check_patch`](crate::rqp::patch::check_patch), which keeps the pair
/// together; this function is public for the tests and the fuzz target.
///
/// The function first checks every index of `patched` (also of a `json`
/// block): a missing block, a range with `start > end` or `end` after the
/// end of `document`, a range that does not start after a `>` and end at a
/// `<` (the start tag and the end tag of the block), and two patched `html`
/// blocks whose ranges overlap
/// reject with [`ContentViolation::Markup`] (an internal defect of the
/// caller; a valid analysis has none of them). When no patched block is an
/// `html` block, the document is not read. Else the [`scan`] reads
/// `document` from offset 0 up to the end of the last patched `html`
/// block; a scan stop (an error item, or the end of the items) before that
/// end rejects with [`ContentViolation::Markup`] and the first patched
/// `html` block that the scan did not read to its end.
///
/// # Errors
///
/// [`ContentError`] for the first violation.
pub fn check_patch_content(
    document: &[u8],
    analysis: &RqpDocument,
    patched: &[usize],
    limits: &TemplateLimits,
) -> Result<(), ContentError> {
    let ranges = html_ranges(document, analysis, patched)?;
    let Some(last_end) = ranges.last().map(|range| range.end) else {
        return Ok(());
    };
    let mut names = NameBuffers::default();
    let mut tokens = scan(document, limits);
    // Offset after the last token or gap that the scan gave.
    let mut cursor = 0;
    // Index into `ranges` of the first range that does not end before the
    // current token.
    let mut next = 0;
    while cursor < last_end {
        let token = match tokens.next() {
            Some(Ok(token)) => token,
            Some(Err(error)) => return Err(unread(&ranges, cursor, error)),
            None => return Err(unread(&ranges, cursor, MarkupError::internal())),
        };
        let span = token.span();
        cursor = span.end;
        let Token::StartTag(tag) = token else {
            continue;
        };
        while ranges
            .get(next)
            .is_some_and(|range| range.end <= span.start)
        {
            next = next.saturating_add(1);
        }
        if let Some(range) = ranges.get(next)
            && range.start <= span.start
        {
            check_tag(&tag, &mut names).map_err(|violation| ContentError {
                block: range.block,
                violation,
            })?;
        }
    }
    Ok(())
}

/// The attribute rule of section 4.2 for one attribute of one start tag.
/// `element` is the tag name in lower case, `foreign` is `true` for a start
/// tag in a foreign region, `name` is the attribute name in lower case and
/// `raw_value` the raw value (no character reference decoding; empty for an
/// attribute without a value). Contract.
///
/// The items in order: item 1 ([`ContentViolation::EventHandler`]); for the
/// attribute `srcset` item 4.3 ([`ContentViolation::Srcset`] for every
/// failure in it); for every other attribute item 2
/// ([`ContentViolation::CharacterReference`]) and items 3 and 4
/// ([`ContentViolation::Scheme`]). The work is linear in the length of
/// `raw_value`; nothing is allocated.
///
/// # Errors
///
/// The [`ContentViolation`] of the first item that rejects.
pub fn check_attribute(
    element: &[u8],
    foreign: bool,
    name: &[u8],
    raw_value: &[u8],
) -> Result<(), ContentViolation> {
    if name.starts_with(b"on") {
        return Err(ContentViolation::EventHandler);
    }
    if name == b"srcset" {
        return if srcset_rejects(element, foreign, raw_value) {
            Err(ContentViolation::Srcset)
        } else {
            Ok(())
        };
    }
    check_value(element, foreign, name, raw_value)
}

// ---------------------------------------------------------------------------
// Scope: the patched `html` blocks.

/// The content range of one patched `html` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Range {
    start: usize,
    end: usize,
    block: usize,
}

/// An internal defect of the caller in `patched` or `analysis`.
fn defect(block: usize) -> ContentError {
    ContentError {
        block,
        violation: ContentViolation::Markup(MarkupError::internal()),
    }
}

/// The content ranges of the patched `html` blocks in document order,
/// without duplicates. Checks every patched index (see
/// [`check_patch_content`]).
fn html_ranges(
    document: &[u8],
    analysis: &RqpDocument,
    patched: &[usize],
) -> Result<Vec<Range>, ContentError> {
    let mut ranges = Vec::new();
    for &block in patched {
        let entry = analysis.blocks.get(block).ok_or_else(|| defect(block))?;
        if entry.start > entry.end || entry.end > document.len() {
            return Err(defect(block));
        }
        // `analysis` must be the analysis of `document` (S2 security
        // review, finding S2-2). The content of a block starts after the
        // `>` of its start tag and ends at the `<` of its end tag (M-27);
        // ranges of another document that do not have these bytes reject.
        let after_start_tag = entry
            .start
            .checked_sub(1)
            .and_then(|at| document.get(at))
            .is_some_and(|&byte| byte == b'>');
        if !after_start_tag || document.get(entry.end) != Some(&b'<') {
            return Err(defect(block));
        }
        if entry.descriptor.format == BlockFormat::Html {
            ranges.push(Range {
                start: entry.start,
                end: entry.end,
                block,
            });
        }
    }
    ranges.sort_unstable_by_key(|range| (range.start, range.block));
    ranges.dedup_by_key(|range| range.block);
    let overlap = ranges.windows(2).find_map(|pair| match pair {
        [first, second] if second.start < first.end => Some(second.block),
        _ => None,
    });
    match overlap {
        Some(block) => Err(defect(block)),
        None => Ok(ranges),
    }
}

/// The scan stopped at `cursor`, before the end of the last range: the
/// first range that it did not read to its end rejects.
fn unread(ranges: &[Range], cursor: usize, error: MarkupError) -> ContentError {
    let block = ranges
        .iter()
        .find(|range| range.end > cursor)
        .or(ranges.last())
        .map_or(0, |range| range.block);
    ContentError {
        block,
        violation: ContentViolation::Markup(error),
    }
}

// ---------------------------------------------------------------------------
// Section 4, item 1: one start tag of the content.

/// Buffers for the lower-case names, reused for every tag. They grow to the
/// longest name with an ASCII upper-case letter (at most the document).
#[derive(Default)]
struct NameBuffers {
    element: Vec<u8>,
    attribute: Vec<u8>,
}

/// `raw` in ASCII lower case, as the tokenizer gives the name. A name
/// without an upper-case letter is not copied. (The tokenizer also makes
/// U+0000 into U+FFFD; a name with either never equals a name of a list or
/// starts with `on` at that byte, so the raw byte gives the same result.)
fn lower<'b>(raw: &'b [u8], buffer: &'b mut Vec<u8>) -> &'b [u8] {
    if !raw.iter().any(u8::is_ascii_uppercase) {
        return raw;
    }
    buffer.clear();
    buffer.extend(raw.iter().map(u8::to_ascii_lowercase));
    buffer
}

fn in_list(list: &[&str], name: &[u8]) -> bool {
    list.iter().any(|entry| entry.as_bytes() == name)
}

/// Items 1.1, 1.2 and 1.3 for one start tag, in document order: the name,
/// then each attribute in source order (also each duplicate).
fn check_tag(tag: &Tag<'_>, names: &mut NameBuffers) -> Result<(), ContentViolation> {
    let foreign = tag.in_foreign_region();
    let element = lower(tag.name(), &mut names.element);
    if foreign {
        if !in_list(&FOREIGN_ALLOWED, element) {
            return Err(ContentViolation::ForeignElement);
        }
    } else if in_list(&PATCH_FORBIDDEN, element) {
        return Err(ContentViolation::ForbiddenElement);
    }
    for attribute in tag.attributes() {
        let attribute = attribute.map_err(ContentViolation::Markup)?;
        let name = lower(attribute.name(), &mut names.attribute);
        check_attribute(element, foreign, name, attribute.value())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Section 4.2, items 2 to 4.2: one value (or one srcset candidate URL).

/// Items 2 to 4.2 on one raw value.
fn check_value(
    element: &[u8],
    foreign: bool,
    name: &[u8],
    raw: &[u8],
) -> Result<(), ContentViolation> {
    if reference_at_decision_byte(raw) {
        return Err(ContentViolation::CharacterReference);
    }
    if scheme_rejects(element, foreign, name, raw) {
        return Err(ContentViolation::Scheme);
    }
    Ok(())
}

/// The bytes `09`, `0A`, `0D`: the URL parser removes them everywhere.
const fn is_tab_or_newline(byte: u8) -> bool {
    matches!(byte, 0x09 | 0x0A | 0x0D)
}

/// A byte that item 2 skips after the start: tab or newline, or a scheme
/// character (ASCII letter, digit, `+`, `-`, `.`).
const fn skipped_after_start(byte: u8) -> bool {
    is_tab_or_newline(byte) || byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
}

/// The largest value of a numeric character reference: a larger number
/// stops at this value (markup M-13).
const REFERENCE_CAP: u32 = 0x11_0000;

/// The value of a numeric character reference at the start of `after`
/// (the bytes after a `&`), read on the raw bytes as markup M-13 says:
/// `#` and decimal digits, or `#x` / `#X` and hexadecimal digits, all the
/// digits (also leading zeros), `;` optional, capped at `0x110000`. `None`
/// when no digit follows.
fn numeric_reference(after: &[u8]) -> Option<u32> {
    let rest = after.strip_prefix(b"#")?;
    let (digits, radix) = match rest.split_first() {
        Some((b'x' | b'X', hex)) => (hex, 16),
        _ => (rest, 10),
    };
    let mut value: Option<u32> = None;
    for &byte in digits {
        let Some(digit) = char::from(byte).to_digit(radix) else {
            break;
        };
        let next = value
            .unwrap_or(0)
            .saturating_mul(radix)
            .saturating_add(digit);
        value = Some(next.min(REFERENCE_CAP));
    }
    value
}

/// Item 2: a character reference at the decision byte that the URL parser
/// can read as part of a scheme (H-77). On the raw bytes.
fn reference_at_decision_byte(raw: &[u8]) -> bool {
    let Some(first) = raw.iter().position(|&byte| byte > 0x20) else {
        return false;
    };
    let rest = raw.get(first..).unwrap_or_default();
    let Some(decision) = rest.iter().position(|&byte| !skipped_after_start(byte)) else {
        return false;
    };
    let Some((&b'&', after)) = rest.get(decision..).and_then(<[u8]>::split_first) else {
        return false;
    };
    match numeric_reference(after) {
        // Item 2.1.
        Some(value) => continues_scheme(value),
        // Item 2.2.
        None => SCHEME_NAMED.iter().any(|named| after.starts_with(named)),
    }
}

/// Item 2.1: a value that the URL parser strips or removes (`00`–`20`),
/// a scheme character or `:`.
fn continues_scheme(value: u32) -> bool {
    u8::try_from(value).is_ok_and(|byte| {
        byte <= 0x20 || byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'+' | b'-' | b'.')
    })
}

/// Item 2.2: the named references (the bytes after the `&`,
/// case-sensitive) that decode to a character of item 2.1.
const SCHEME_NAMED: [&[u8]; 6] = [
    b"Tab;",
    b"NewLine;",
    b"colon;",
    b"fjlig;",
    b"period;",
    b"plus;",
];

/// The copy of item 3, without allocation: the raw value without `09`,
/// `0A`, `0D`, and without the bytes `00`–`20` at the start. (Removing the
/// tab and newline bytes first and then the leading bytes gives the same
/// bytes: the leading run is the same run without them.)
fn copy(raw: &[u8]) -> impl Iterator<Item = u8> + '_ {
    raw.iter()
        .copied()
        .skip_while(|&byte| byte <= 0x20)
        .filter(|&byte| !is_tab_or_newline(byte))
}

/// The copy starts with `prefix` (lower case), ASCII case-insensitively.
fn copy_starts_with(raw: &[u8], prefix: &[u8]) -> bool {
    let mut bytes = copy(raw);
    prefix.iter().all(|&expected| {
        bytes
            .next()
            .is_some_and(|byte| byte.to_ascii_lowercase() == expected)
    })
}

/// Item 4.2: the raster types of the allow-list (DL-052), in lower case.
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

/// The length of the longest raster type (`image/vnd.microsoft.icon`).
const RASTER_TYPE_MAX: usize = 24;

/// Item 4.2: the element and the attribute where a `data:` raster image
/// can pass, in HTML context only (H-83). The pairs `img srcset` and
/// `source srcset` of item 4.2 come here as `src`: item 4.3 checks each
/// candidate URL of a `srcset` as the value of `src` of the same element,
/// and [`check_attribute`] never gives a whole `srcset` value to item 4.2.
fn data_allowed_here(element: &[u8], foreign: bool, name: &[u8]) -> bool {
    !foreign
        && matches!(
            (element, name),
            (b"img" | b"source", b"src") | (b"video", b"poster")
        )
}

/// Item 4.2: the type part of the copy (after `data:` up to the first `;`
/// or `,`, or to the end) is exactly one raster type, ASCII
/// case-insensitively. Reads at most `RASTER_TYPE_MAX + 1` bytes of it.
fn type_part_is_raster(raw: &[u8]) -> bool {
    let mut part = [0u8; RASTER_TYPE_MAX];
    let mut len = 0usize;
    for byte in copy(raw).skip(b"data:".len()) {
        if matches!(byte, b';' | b',') {
            break;
        }
        let Some(slot) = part.get_mut(len) else {
            return false;
        };
        *slot = byte.to_ascii_lowercase();
        len = len.saturating_add(1);
    }
    part.get(..len)
        .is_some_and(|part| RASTER_TYPES.contains(&part))
}

/// Items 3 and 4.1, 4.2 on the copy.
fn scheme_rejects(element: &[u8], foreign: bool, name: &[u8], raw: &[u8]) -> bool {
    if copy_starts_with(raw, b"javascript:") || copy_starts_with(raw, b"vbscript:") {
        return true;
    }
    if copy_starts_with(raw, b"data:") {
        return !(data_allowed_here(element, foreign, name) && type_part_is_raster(raw));
    }
    false
}

// ---------------------------------------------------------------------------
// Section 4.2, item 4.3: srcset.

/// ASCII white space of the WHATWG text: `09`, `0A`, `0C`, `0D`, `20`.
const fn is_ascii_white_space(byte: u8) -> bool {
    matches!(byte, 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

/// Item 4.3: the named references (the bytes after the `&`,
/// case-sensitive) that decode to white space, `,`, `(` or `)`.
const SEPARATOR_NAMED: [&[u8]; 5] = [b"Tab;", b"NewLine;", b"comma;", b"lpar;", b"rpar;"];

/// Item 4.3, first part: the raw value has a character reference that
/// decodes to white space, `,`, `(` or `)`. Each `&` reads only the digits
/// after it, and digits are not `&`, so the work is linear.
fn has_separator_reference(raw: &[u8]) -> bool {
    raw.iter().enumerate().any(|(index, &byte)| {
        if byte != b'&' {
            return false;
        }
        let after = raw.get(index.saturating_add(1)..).unwrap_or_default();
        match numeric_reference(after) {
            Some(value) => u8::try_from(value)
                .is_ok_and(|byte| is_ascii_white_space(byte) || matches!(byte, b',' | b'(' | b')')),
            None => SEPARATOR_NAMED.iter().any(|named| after.starts_with(named)),
        }
    })
}

/// The candidate URLs of a `srcset` value, split as the WHATWG algorithm
/// "parse a srcset attribute" splits it (HTML section 4.8.4.3.10): skip
/// white space and commas; the URL is the next run of bytes that are not
/// white space, without its final commas; when the run did not end with a
/// comma, the descriptors after it end at a comma outside parentheses (or
/// at the end). Every byte is read once.
struct SrcsetUrls<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for SrcsetUrls<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        let start = self
            .rest
            .iter()
            .position(|&byte| !is_ascii_white_space(byte) && byte != b',')?;
        let rest = self.rest.get(start..)?;
        let url_len = rest
            .iter()
            .position(|&byte| is_ascii_white_space(byte))
            .unwrap_or(rest.len());
        let (url, after) = rest.split_at_checked(url_len)?;
        let trimmed_len = url
            .iter()
            .rposition(|&byte| byte != b',')
            .map_or(0, |last| last.saturating_add(1));
        let (url, commas) = url.split_at_checked(trimmed_len)?;
        self.rest = if commas.is_empty() {
            skip_descriptors(after)
        } else {
            after
        };
        Some(url)
    }
}

/// The bytes after the descriptors of one candidate: after the first comma
/// outside parentheses, or empty.
fn skip_descriptors(bytes: &[u8]) -> &[u8] {
    let mut in_parens = false;
    for (index, &byte) in bytes.iter().enumerate() {
        match (in_parens, byte) {
            (false, b',') => return bytes.get(index.saturating_add(1)..).unwrap_or_default(),
            (false, b'(') => in_parens = true,
            (true, b')') => in_parens = false,
            _ => {}
        }
    }
    &[]
}

/// Item 4.3: a separator reference, or a candidate URL that items 2 to 4.2
/// reject as the value of `src` of the same element.
fn srcset_rejects(element: &[u8], foreign: bool, raw: &[u8]) -> bool {
    has_separator_reference(raw)
        || (SrcsetUrls { rest: raw }).any(|url| check_value(element, foreign, b"src", url).is_err())
}
