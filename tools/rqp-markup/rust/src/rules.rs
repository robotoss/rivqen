// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! One pass over the tokens: the token stack (M-05), the document rules
//! (M-06…M-13) and the block rules (M-14…M-26) of `markup.md` section 2.
//!
//! The pass returns at the first rule that fails. Each token costs O(1)
//! amortized work: the nearest element with a name is found through the
//! per-name pointer of [`NameTable`], and every element is popped at most
//! once.
//!
//! The depth counter D of M-26 (H-31) is never larger than
//! [`MAX_OPEN_ELEMENTS`]: every unit of D is added by `Pass::raise` (a push
//! or an implied table part), which checks the limit. Every element on the
//! token stack holds at least one unit of D, so the stack never has more
//! than [`MAX_OPEN_ELEMENTS`] elements.

use std::collections::HashSet;

use crate::names::{Known, NameId, NameTable, RawKind, flag, raw_kind};
use crate::tokenizer::{Span, Tag, Token, Tokenizer};
use crate::{ErrorCode, Format, MAX_BLOCK_BYTES, MAX_BLOCKS, MAX_OPEN_ELEMENTS, MarkupError, json};

/// A block found by the pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RawBlock {
    pub(crate) id: Span,
    pub(crate) format: Format,
    pub(crate) content: Span,
}

/// The implied table parts of an element F (M-26 item 3), one bit each.
mod part {
    /// A row group that the tree builder inserts in a `table`.
    pub(super) const ROW_GROUP: u8 = 1;
    /// A row that the tree builder inserts in a `table` or a row group.
    pub(super) const ROW: u8 = 1 << 1;
    /// A column group that the tree builder inserts in a `table`.
    pub(super) const COLUMN_GROUP: u8 = 1 << 2;
    /// All parts.
    pub(super) const ALL: [u8; 3] = [ROW_GROUP, ROW, COLUMN_GROUP];
}

#[derive(Clone, Copy, Debug)]
struct Elem {
    name: NameId,
    /// Pushed in HTML context (not in a foreign region).
    html: bool,
    /// The previous element with the same name (its stack index).
    prev_same: Option<usize>,
    /// M-26 item 5: an end tag that crossed elements, popped a dirty
    /// element, or popped `html` or `body` left this element as the current
    /// node. Its own end tag then gives no units of D back.
    dirty: bool,
    /// M-06 item 4: the special-crossed mark. An end tag with a name in
    /// `FORMATTING` must not pop this element.
    marked: bool,
    /// M-26 item 3: the implied parts ([`part`] bits) of this element as F.
    /// Each part holds one unit of D.
    implied: u8,
}

impl Elem {
    /// The units of D that an exact pop of this element gives back (M-26
    /// item 4): its own unit and one for each implied part.
    fn units(&self) -> usize {
        part::ALL
            .iter()
            .filter(|&&bit| self.implied & bit != 0)
            .count()
            .saturating_add(1)
    }
}

/// How the content of the open block is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Content {
    /// `html` block other than `title`: tokens, M-22…M-24.
    Markup,
    /// `title` (RCDATA) or `json` (script data): the next token is the
    /// block end tag.
    Raw,
}

#[derive(Clone, Copy, Debug)]
struct OpenBlock {
    id: Span,
    format: Format,
    content: Content,
    /// Offset after the block start tag.
    start: usize,
    /// Stack index of the block element.
    elem: usize,
    name: NameId,
}

type RawSwitch = Option<(RawKind, &'static [u8])>;

fn structure(rule: &'static str) -> MarkupError {
    MarkupError::new(ErrorCode::Structure, rule)
}

fn limit() -> MarkupError {
    MarkupError::new(ErrorCode::Limit, "M-26")
}

/// The bytes of a span of `text`. A span outside `text` is a defect of this
/// candidate: the document fails closed.
fn slice(text: &[u8], span: Span) -> Result<&[u8], MarkupError> {
    text.get(span.start..span.end)
        .ok_or_else(MarkupError::internal)
}

/// M-15: `^[a-z0-9][a-z0-9_-]{0,63}$` on the raw value.
pub(crate) fn is_valid_id(id: &[u8]) -> bool {
    let Some((&first, rest)) = id.split_first() else {
        return false;
    };
    rest.len() <= 63
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && rest
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// M-13: a reserved script `type` raw value. It contains `rivqen-manifest`
/// in any ASCII case (item 1), a `&` that item 2 rejects (H-84) or a byte
/// >= 0x80 (item 3, H-28).
fn is_reserved_type(value: &[u8]) -> bool {
    value.iter().any(|b| !b.is_ascii())
        || value
            .windows(b"rivqen-manifest".len())
            .any(|w| w.eq_ignore_ascii_case(b"rivqen-manifest"))
        || value
            .iter()
            .enumerate()
            .filter(|&(_, &b)| b == b'&')
            .any(|(i, _)| {
                value
                    .get(i.saturating_add(1)..)
                    .is_some_and(rejects_reference)
            })
}

/// M-13 item 2 (H-84, DL-048 a) for the bytes after one `&`: an ASCII letter
/// (a named reference), or a numeric reference whose value is `00`..`20`, an
/// ASCII letter or digit, `/`, `+`, `-`, `.`, or `80` or more. `&#` and `&#x`
/// without a digit are not references. The value stops at `110000`.
fn rejects_reference(after: &[u8]) -> bool {
    const CAP: u32 = 0x0011_0000;
    let Some((&first, rest)) = after.split_first() else {
        return false;
    };
    if first.is_ascii_alphabetic() {
        return true;
    }
    if first != b'#' {
        return false;
    }
    let (radix, digits) = match rest.split_first() {
        Some((b'x' | b'X', hex)) => (16, hex),
        _ => (10, rest),
    };
    let mut seen = false;
    let mut value: u32 = 0;
    for digit in digits.iter().map_while(|&b| char::from(b).to_digit(radix)) {
        seen = true;
        value = value.saturating_mul(radix).saturating_add(digit).min(CAP);
    }
    seen && match u8::try_from(value) {
        Ok(byte) => {
            byte <= 0x20
                || byte >= 0x80
                || byte.is_ascii_alphanumeric()
                || matches!(byte, b'/' | b'+' | b'-' | b'.')
        }
        Err(_) => true,
    }
}

/// M-16 (H-86): the `type` raw value of a `json` block. Exact and
/// case-sensitive, without white space or parameters.
fn is_json_type(value: &[u8]) -> bool {
    value == b"application/json" || value == b"application/ld+json"
}

/// The end tag after the content C of a `noscript` element (M-11).
const NOSCRIPT_END: &[u8] = b"</noscript>";

/// The most implied closure steps for one start tag (M-09, H-85a): a row
/// group, a row and a cell.
const CLOSURE_STEPS: usize = 3;

/// M-09: the names that F may have for a table part start tag. `None` for
/// every other name (also `table`).
fn allowed_parents(name: NameId) -> Option<&'static [Known]> {
    use Known::{Caption, Col, Colgroup, Table, Tbody, Td, Tfoot, Th, Thead, Tr};
    if is_any(name, &[Caption, Colgroup, Tbody, Thead, Tfoot]) {
        Some(&[Table])
    } else if name == Col.id() {
        Some(&[Table, Colgroup])
    } else if name == Tr.id() {
        Some(&[Table, Tbody, Thead, Tfoot])
    } else if is_any(name, &[Td, Th]) {
        Some(&[Table, Tbody, Thead, Tfoot, Tr])
    } else {
        None
    }
}

fn is_any(name: NameId, names: &[Known]) -> bool {
    names.iter().any(|k| k.id() == name)
}

struct Pass<'a> {
    input: &'a [u8],
    names: NameTable,
    stack: Vec<Elem>,
    /// The depth counter D of M-26 (H-31). Never larger than
    /// [`MAX_OPEN_ELEMENTS`] and never smaller than the stack size.
    depth: usize,
    /// Stack indices of the elements pushed in HTML context whose name is in
    /// `TABLE_FAMILY` or is `template` (M-09, M-19).
    table_marks: Vec<usize>,
    /// M-10 item 2 (H-36): the number of `form` elements pushed in HTML
    /// context that are on the token stack. Every pop changes it, also a pop
    /// by a crossing. Never larger than the stack size.
    html_forms: usize,
    /// Stack index of the `svg` or `math` element that opened the current
    /// foreign region.
    foreign_root: Option<usize>,
    /// M-07.3 and M-07.4: an element in a foreign region whose content must
    /// be character tokens only, up to its end tag.
    text_only: Option<NameId>,
    block: Option<OpenBlock>,
    blocks: Vec<RawBlock>,
    ids: HashSet<Box<[u8]>>,
    /// Offset after a `noscript` start tag in HTML context whose RAWTEXT
    /// content is not checked yet (M-11).
    noscript: Option<usize>,
    /// The pass over a `noscript` content C is running (M-11 item 8): the
    /// implied closure of M-09 does not apply (H-85a).
    in_noscript: bool,
}

/// Runs the pass over `input` (already checked by M-01 and M-02). The
/// tokenizer starts at `start` (after a byte order mark).
pub(crate) fn run(input: &[u8], start: usize) -> Result<Vec<RawBlock>, MarkupError> {
    let mut pass = Pass {
        input,
        names: NameTable::new(),
        stack: Vec::with_capacity(MAX_OPEN_ELEMENTS),
        depth: 0,
        table_marks: Vec::new(),
        html_forms: 0,
        foreign_root: None,
        text_only: None,
        block: None,
        blocks: Vec::new(),
        ids: HashSet::new(),
        noscript: None,
        in_noscript: false,
    };
    let mut tokens = Tokenizer::new(input, start);
    loop {
        let token = tokens.next_token();
        if tokens.defect() {
            return Err(MarkupError::internal());
        }
        match token {
            Token::StartTag(tag) => {
                if let Some((kind, end_name)) = pass.start_tag(input, &tag)? {
                    tokens.set_raw(kind, end_name);
                }
            }
            Token::EndTag(tag) => pass.end_tag(input, &tag)?,
            Token::Eof => return pass.finish(),
            other => pass.other_token(&other)?,
        }
    }
}

impl Pass<'_> {
    /// A comment, a DOCTYPE or a CDATA section.
    fn other_token(&mut self, token: &Token) -> Result<(), MarkupError> {
        match token {
            Token::Comment(_) => self.not_character(),
            Token::Doctype(_) => self.doctype(),
            // E5 (H-21): a CDATA section that satisfies M-12 is allowed
            // also where M-07.3 and M-07.4 allow character tokens only.
            Token::Cdata { well_formed, .. } if !well_formed => Err(structure("M-12")),
            _ => Ok(()),
        }
    }

    fn is_open(&self, name: Known) -> bool {
        self.names.top(name.id()).is_some()
    }

    /// M-08 applies to a `select` element pushed in HTML context only. A
    /// `select` in a foreign region is a foreign element (M-07 applies).
    /// Inside an HTML `select` no `svg` or `math` can start (M-08), and a
    /// foreign region ends with all its elements, so the nearest `select` is
    /// the HTML one whenever one is open.
    fn html_select_open(&self) -> bool {
        self.names
            .top(Known::Select.id())
            .and_then(|index| self.stack.get(index))
            .is_some_and(|elem| elem.html)
    }

    /// An element with this name was opened in the content of the block
    /// whose element is at `block_elem`, and it is still open.
    fn open_in_content(&self, name: Known, block_elem: usize) -> bool {
        self.names.top(name.id()).is_some_and(|i| i > block_elem)
    }

    /// M-09: the stack index and the name of the nearest element with a name
    /// in `TABLE_FAMILY`; the search stops at `template`.
    fn table_context_at(&self) -> Option<(usize, NameId)> {
        let index = *self.table_marks.last()?;
        let name = self.stack.get(index)?.name;
        if self.names.has(name, flag::TEMPLATE) {
            None
        } else {
            Some((index, name))
        }
    }

    /// M-09: the name of F.
    fn table_context(&self) -> Option<NameId> {
        self.table_context_at().map(|(_, name)| name)
    }

    /// M-26: adds one unit to D. A step that makes D larger than
    /// [`MAX_OPEN_ELEMENTS`] fails. This is the only place that adds to D,
    /// so the limit holds in HTML context, in a foreign region, for a block
    /// element, in block content and for implied table parts.
    fn raise(&mut self) -> Result<(), MarkupError> {
        let depth = self
            .depth
            .checked_add(1)
            .ok_or_else(MarkupError::internal)?;
        if depth > MAX_OPEN_ELEMENTS {
            return Err(limit());
        }
        self.depth = depth;
        Ok(())
    }

    /// M-26 item 3: the implied parts of a start tag in HTML context. F gets
    /// each part once; each new part adds one unit to D. Called after M-09,
    /// so F is one of the allowed elements of the start tag.
    fn implied_parts(&mut self, name: NameId) -> Result<(), MarkupError> {
        use Known::{Col, Table, Tbody, Td, Tfoot, Th, Thead, Tr};
        let Some((index, context)) = self.table_context_at() else {
            return Ok(());
        };
        let in_table = context == Table.id();
        let cell = is_any(name, &[Td, Th]);
        let parts = if in_table && name == Tr.id() {
            part::ROW_GROUP
        } else if in_table && cell {
            part::ROW_GROUP | part::ROW
        } else if cell && is_any(context, &[Tbody, Thead, Tfoot]) {
            part::ROW
        } else if in_table && name == Col.id() {
            part::COLUMN_GROUP
        } else {
            0
        };
        for bit in part::ALL {
            let has = self
                .stack
                .get(index)
                .ok_or_else(MarkupError::internal)?
                .implied;
            if parts & bit != 0 && has & bit == 0 {
                self.raise()?;
                if let Some(f) = self.stack.get_mut(index) {
                    f.implied |= bit;
                }
            }
        }
        Ok(())
    }

    /// M-05 push, with its unit of D (M-26 item 2). This is the only place
    /// that pushes.
    fn push(&mut self, name: NameId, html: bool) -> Result<usize, MarkupError> {
        self.raise()?;
        let index = self.stack.len();
        // Every element holds a unit of D, so the stack is smaller than D
        // here. The check keeps the stack bounded also if a defect broke that.
        if index >= self.depth {
            return Err(MarkupError::internal());
        }
        let prev_same = self.names.replace_top(name, Some(index));
        self.stack.push(Elem {
            name,
            html,
            prev_same,
            dirty: false,
            marked: false,
            implied: 0,
        });
        if html && self.names.has(name, flag::TABLE_FAMILY | flag::TEMPLATE) {
            self.table_marks.push(index);
        }
        if html && name == Known::Form.id() {
            self.html_forms = self
                .html_forms
                .checked_add(1)
                .ok_or_else(MarkupError::internal)?;
        }
        Ok(index)
    }

    /// Pops the current node. This is the only place that pops. An empty
    /// stack, or a count of HTML forms that does not agree with the stack,
    /// is a defect of this candidate.
    fn pop(&mut self) -> Result<Elem, MarkupError> {
        let elem = self.stack.pop().ok_or_else(MarkupError::internal)?;
        let index = self.stack.len();
        if elem.html && elem.name == Known::Form.id() {
            self.html_forms = self
                .html_forms
                .checked_sub(1)
                .ok_or_else(MarkupError::internal)?;
        }
        self.names.replace_top(elem.name, elem.prev_same);
        if self.table_marks.last() == Some(&index) {
            self.table_marks.pop();
        }
        if self.foreign_root == Some(index) {
            self.foreign_root = None;
        }
        Ok(elem)
    }

    /// M-07.3, M-07.4: a token that is not a character token (and not a
    /// CDATA section, E5) while a foreign text-only element is open.
    fn not_character(&self) -> Result<(), MarkupError> {
        if self.text_only.is_some() {
            Err(structure("M-07"))
        } else {
            Ok(())
        }
    }

    fn doctype(&self) -> Result<(), MarkupError> {
        self.not_character()?;
        if self.html_select_open() {
            return Err(structure("M-08"));
        }
        if self.block.is_some_and(|b| b.content == Content::Markup) {
            return Err(structure("M-22"));
        }
        Ok(())
    }

    fn start_tag(&mut self, src: &[u8], tag: &Tag) -> Result<RawSwitch, MarkupError> {
        let raw_name = slice(src, tag.name)?;
        let name = self.names.intern(raw_name);
        let foreign = self.foreign_root.is_some();

        self.not_character()?;
        if self.html_select_open() && !is_any(name, &[Known::Option, Known::Optgroup, Known::Hr]) {
            return Err(structure("M-08"));
        }
        if name == Known::Frameset.id() {
            return Err(structure("M-10"));
        }
        // M-10 item 2 (H-36): the tree builder ignores a nested `form` start
        // tag (form element pointer). A `form` in a foreign region is a
        // foreign element.
        if !foreign && name == Known::Form.id() && self.html_forms > 0 {
            return Err(structure("M-10"));
        }
        // M-10 item 3 (H-43): `<template for>` is WHATWG content patching,
        // which replaces or removes nodes between `<?start>` and `<?end>`.
        // A `template` in a foreign region is a foreign element.
        if !foreign && name == Known::Template.id() && tag.has_for {
            return Err(structure("M-10"));
        }
        if name == Known::Script.id() {
            if let Some(value) = tag.type_attr {
                if is_reserved_type(slice(src, value)?) {
                    return Err(MarkupError::new(ErrorCode::Reserved, "M-13"));
                }
            }
        }
        if foreign {
            if self.names.has(name, flag::BREAKOUT) {
                return Err(structure("M-07"));
            }
            if self.names.has(name, flag::TEXT)
                && (tag.self_closing || name == Known::Plaintext.id())
            {
                return Err(structure("M-07"));
            }
        }

        if let Some(block) = self.block {
            // Only a markup block has tokens in its content.
            if tag.block_attr.is_some() {
                return Err(MarkupError::new(ErrorCode::Nested, "M-17"));
            }
            self.content_start(tag, name, block, foreign)?;
        } else if let Some(attr) = tag.block_attr {
            return self.block_start(src, tag, name, attr, foreign);
        }

        if foreign {
            // M-05 step 2.
            if !tag.self_closing {
                self.push(name, false)?;
                if self.names.has(name, flag::INTEGRATION | flag::TEXT) {
                    self.text_only = Some(name);
                }
            }
            return Ok(None);
        }

        // M-05 step 1, HTML context.
        self.implied_closure(name)?;
        self.table_part(name)?;
        self.implied_parts(name)?;
        let is_root = name == Known::Svg.id() || name == Known::Math.id();
        if (is_root && tag.self_closing) || self.names.has(name, flag::VOID) {
            return Ok(None);
        }
        let index = self.push(name, true)?;
        if is_root {
            self.foreign_root = Some(index);
        }
        if name == Known::Noscript.id() {
            self.noscript = Some(tag.span.end);
        }
        // M-04: also with the self-closing flag.
        Ok(raw_kind(raw_name))
    }

    /// M-09 implied closure (H-85a). Outside block content and outside the
    /// pass on C, a table part start tag in HTML context closes F while F
    /// does not allow the tag. Each step is the end tag of F. At most
    /// [`CLOSURE_STEPS`] steps; it never closes a `table`.
    fn implied_closure(&mut self, name: NameId) -> Result<(), MarkupError> {
        use Known::{Colgroup, Tbody, Tfoot, Thead, Tr};
        if self.block.is_some() || self.in_noscript {
            return Ok(());
        }
        let Some(allowed) = allowed_parents(name) else {
            return Ok(());
        };
        for _ in 0..CLOSURE_STEPS {
            let Some((index, context)) = self.table_context_at() else {
                return Ok(());
            };
            if is_any(context, allowed) {
                return Ok(());
            }
            // Step 2: as condition 2, F is the current node.
            if is_any(context, &[Tr, Tbody, Thead, Tfoot, Colgroup])
                && index.checked_add(1) != Some(self.stack.len())
            {
                return Err(structure("M-09"));
            }
            self.close(context, index)?;
        }
        Ok(())
    }

    /// M-09 with conditions 1 and 2 for a start tag in HTML context. It runs
    /// before the implied parts of M-26 and before the push.
    fn table_part(&self, name: NameId) -> Result<(), MarkupError> {
        use Known::{Col, Colgroup, Table, Tbody, Tfoot, Thead, Tr};
        let part = self.names.has(name, flag::TABLE_PART) || name == Col.id();
        if !part && name != Table.id() {
            return Ok(());
        }
        // Condition 1 (H-35): the nearest element in `TABLE_FAMILY` or
        // `template` is a `template`. Then "in column group" ignores every
        // later start tag, also the raw text ones.
        if name == Col.id()
            && self
                .table_marks
                .last()
                .and_then(|&index| self.stack.get(index))
                .is_some_and(|elem| self.names.has(elem.name, flag::TEMPLATE))
        {
            return Err(structure("M-09"));
        }
        let Some((index, context)) = self.table_context_at() else {
            return Ok(());
        };
        // Condition 2 (H-37): "clear the stack back to a table context" must
        // pop nothing, so F is the current node.
        if is_any(context, &[Table, Tbody, Thead, Tfoot, Tr, Colgroup])
            && index.checked_add(1) != Some(self.stack.len())
        {
            return Err(structure("M-09"));
        }
        let Some(allowed) = allowed_parents(name) else {
            // `table`: only condition 2.
            return Ok(());
        };
        if is_any(context, allowed) {
            Ok(())
        } else {
            Err(structure("M-09"))
        }
    }

    /// M-15, M-16, M-18, M-19, M-21 and the block count of M-26 for a block
    /// start tag outside block content. The push of the block element then
    /// adds its unit of D (M-26).
    fn block_start(
        &mut self,
        src: &[u8],
        tag: &Tag,
        name: NameId,
        attr: Span,
        foreign: bool,
    ) -> Result<RawSwitch, MarkupError> {
        let id = slice(src, attr)?;
        if !is_valid_id(id) {
            return Err(MarkupError::new(ErrorCode::InvalidId, "M-15"));
        }
        let json_type = match tag.type_attr {
            Some(value) => is_json_type(slice(src, value)?),
            None => false,
        };
        let kind = if foreign {
            None
        } else if self.names.has(name, flag::BLOCK_HTML) {
            let content = if name == Known::Title.id() {
                Content::Raw
            } else {
                Content::Markup
            };
            Some((Format::Html, content))
        } else if name == Known::Script.id() && json_type {
            Some((Format::Json, Content::Raw))
        } else {
            None
        };
        let Some((format, content)) = kind else {
            return Err(MarkupError::new(ErrorCode::ForbiddenElement, "M-16"));
        };
        if tag.self_closing {
            return Err(structure("M-18"));
        }
        if self.is_open(Known::Template) || self.is_open(Known::Select) {
            return Err(structure("M-19"));
        }
        if self
            .table_context()
            .is_some_and(|context| !is_any(context, &[Known::Td, Known::Th, Known::Caption]))
        {
            return Err(structure("M-19"));
        }
        if !self.ids.insert(id.into()) {
            return Err(MarkupError::new(ErrorCode::Duplicate, "M-21"));
        }
        if self.blocks.len() >= MAX_BLOCKS {
            return Err(limit());
        }
        let elem = self.push(name, true)?;
        self.block = Some(OpenBlock {
            id: attr,
            format,
            content,
            start: tag.span.end,
            elem,
            name,
        });
        Ok(raw_kind(slice(src, tag.name)?))
    }

    /// M-22 and M-24 for a start tag in the content of a markup block.
    /// `foreign` is true for a start tag in a foreign region of the content:
    /// only M-07 and M-23 apply there (H-73, H-78), not M-22 and M-24.
    fn content_start(
        &self,
        tag: &Tag,
        name: NameId,
        block: OpenBlock,
        foreign: bool,
    ) -> Result<(), MarkupError> {
        use Known::{A, Button, Col, Colgroup, Dd, Dl, Dt, H1, H2, H3, H4, H5, H6, Li, Menu};
        use Known::{Nobr, Ol, P, Rb, Rp, Rt, Rtc, Ruby, Span, Table, Tbody, Template, Tfoot};
        use Known::{Thead, Tr, Ul};
        if foreign {
            return Ok(());
        }
        let region_root = is_any(name, &[Known::Svg, Known::Math]);
        if self.names.has(name, flag::FORBIDDEN_CONTENT)
            || (tag.self_closing && !self.names.has(name, flag::VOID) && !region_root)
        {
            return Err(structure("M-22"));
        }
        // Item 6 (H-94, H-97): declarative shadow DOM, any attribute name
        // `shadowrootmode` on an HTML `template` start tag.
        if name == Template.id() && tag.has_shadowrootmode {
            return Err(structure("M-22"));
        }
        let elem = block.elem;
        let in_content = |names: &[Known]| names.iter().any(|&k| self.open_in_content(k, elem));
        let open = |names: &[Known]| names.iter().any(|&k| self.is_open(k));
        // The stack index of the nearest open element with one of the names.
        let nearest = |names: &[Known]| names.iter().filter_map(|&k| self.names.top(k.id())).max();
        let no_ruby = !in_content(&[Ruby]);
        let list_item = is_any(name, &[Li, Dd, Dt]);
        let part = self.names.has(name, flag::TABLE_PART) || name == Col.id();
        let implied = [
            // a
            self.names.has(name, flag::PCLOSE) && is_any(block.name, &[P, Span]),
            // b
            self.names.has(name, flag::HEADING)
                && (self.names.has(block.name, flag::HEADING)
                    || in_content(&[H1, H2, H3, H4, H5, H6])),
            // c
            list_item && !in_content(&[Ul, Ol, Menu, Dl]),
            // j: an item opened in the content is above the nearest list. When
            // the nearest list is outside the content, item c applies.
            list_item && nearest(&[Li, Dd, Dt]) > nearest(&[Ul, Ol, Menu, Dl]),
            // d, e, f
            is_any(name, &[Button, A, Nobr]) && self.names.top(name).is_some(),
            // g (H-94): also when F is none (a `template` is nearer)
            part && (!in_content(&[Table]) || self.table_context().is_none()),
            // h
            is_any(name, &[Rb, Rtc]) && (no_ruby || open(&[Rb, Rp, Rt, Rtc])),
            // i
            is_any(name, &[Rp, Rt]) && (no_ruby || open(&[Rb, Rp, Rt])),
            // k: table context (foster parenting; `table` closes the table)
            !part
                && self
                    .table_context()
                    .is_some_and(|f| is_any(f, &[Table, Tbody, Thead, Tfoot, Tr, Colgroup])),
        ];
        if implied.contains(&true) {
            return Err(structure("M-24"));
        }
        Ok(())
    }

    fn end_tag(&mut self, src: &[u8], tag: &Tag) -> Result<(), MarkupError> {
        if let Some(start) = self.noscript.take() {
            self.check_noscript(slice(
                src,
                Span {
                    start,
                    end: tag.span.start,
                },
            )?)?;
        }
        self.end_tag_rules(slice(src, tag.name)?, tag)
    }

    /// An end tag with the raw name `raw_name`: M-05 step 3 and the rules
    /// that depend on it.
    fn end_tag_rules(&mut self, raw_name: &[u8], tag: &Tag) -> Result<(), MarkupError> {
        let name = self.names.lookup(raw_name);
        if let Some(text_only) = self.text_only {
            if name != Some(text_only) {
                return Err(structure("M-07"));
            }
            self.text_only = None;
        }
        if self.html_select_open()
            && !name.is_some_and(|n| is_any(n, &[Known::Option, Known::Optgroup, Known::Select]))
        {
            return Err(structure("M-08"));
        }
        if let Some(block) = self.block {
            return self.content_end(tag, name, block);
        }

        // M-05 step 3.
        let target = name.and_then(|n| self.names.top(n));
        if let Some(root) = self.foreign_root {
            // M-07.2: only an element of the region can match.
            if target.is_none_or(|t| t < root) {
                return Err(structure("M-07"));
            }
        }
        match (name, target) {
            (Some(name), Some(target)) => self.close(name, target),
            // No match. M-06 item 3 (H-30): in HTML context (in a foreign
            // region M-07.2 has failed above) a stray `tbody`, `thead`,
            // `tfoot`, `tr` or `table` end tag is invalid. Other end tags
            // without a match are ignored.
            _ if name.is_some_and(|n| {
                is_any(
                    n,
                    &[
                        Known::Tbody,
                        Known::Thead,
                        Known::Tfoot,
                        Known::Tr,
                        Known::Table,
                    ],
                )
            }) =>
            {
                Err(structure("M-06"))
            }
            _ => Ok(()),
        }
    }

    /// M-05 step 3 for an end tag with `name` that matches the element at
    /// `target`: pops that element and every element above it (the crossed
    /// elements). Checks M-06 items 1, 2 and 4, then sets the marks of M-06
    /// item 4 and changes D (M-26 items 4 and 5).
    fn close(&mut self, name: NameId, target: usize) -> Result<(), MarkupError> {
        let formatting = self.names.has(name, flag::FORMATTING);
        let table_family = self.names.has(name, flag::TABLE_FAMILY);
        let mut crossed = false;
        let mut mark = false;
        // M-06 item 4 (H-75): the end tag of the region root pops every
        // element of its region in the tree builder too, so the foreign
        // elements that it crosses set no mark.
        let region_root_close = self.foreign_root == Some(target);
        let matched = loop {
            let elem = self.pop()?;
            // M-06 item 4: a formatting end tag pops no marked element. A
            // popped mark goes down to the new current node (item 4 (b)).
            if elem.marked {
                if formatting {
                    return Err(structure("M-06"));
                }
                mark = true;
            }
            if self.stack.len() == target {
                break elem;
            }
            // The element is crossed.
            crossed = true;
            // M-06 item 1.
            if elem.html
                && self.names.has(elem.name, flag::GUARDED)
                && !(table_family && self.names.has(elem.name, flag::TABLE_PART))
            {
                return Err(structure("M-06"));
            }
            // M-06 items 2 and 4 (a): a special element pushed in HTML
            // context, or any element pushed in a foreign region unless the
            // end tag closes the region root.
            if self.names.has(elem.name, flag::SPECIAL) && elem.html
                || !elem.html && !region_root_close
            {
                if formatting {
                    return Err(structure("M-06"));
                }
                mark = true;
            }
        };
        // M-26 items 4 and 5.
        let exact =
            !crossed && !matched.dirty && !is_any(matched.name, &[Known::Html, Known::Body]);
        if exact {
            self.depth = self
                .depth
                .checked_sub(matched.units())
                .ok_or_else(MarkupError::internal)?;
        }
        if let Some(current) = self.stack.last_mut() {
            current.marked |= mark;
            current.dirty |= !exact;
        }
        Ok(())
    }

    /// An end tag while a block is open: M-20, M-22.4, M-23.
    fn content_end(
        &mut self,
        tag: &Tag,
        name: Option<NameId>,
        block: OpenBlock,
    ) -> Result<(), MarkupError> {
        let current = self.stack.last().map(|e| e.name);
        let top = self
            .stack
            .len()
            .checked_sub(1)
            .ok_or_else(MarkupError::internal)?;
        let mut target = top;
        if block.content == Content::Markup {
            // M-22 item 4 is for the HTML context only.
            if self.foreign_root.is_none() && name.is_some_and(|n| self.names.has(n, flag::VOID)) {
                return Err(structure("M-22"));
            }
            // M-23 (H-75): the end tag of the region root can cross the
            // elements of its region.
            let root_close = self
                .foreign_root
                .is_some_and(|root| name.and_then(|n| self.names.top(n)) == Some(root));
            if root_close {
                target = self.foreign_root.ok_or_else(MarkupError::internal)?;
            } else if current.is_none() || current != name {
                return Err(structure("M-23"));
            }
        }
        // For a raw block the tokenizer returns only the appropriate end tag,
        // and the block element is the current node. In both cases the end
        // tag pops the current node, or the region root and the elements
        // above it.
        let closed = if target == top {
            current.ok_or_else(MarkupError::internal)?
        } else {
            name.ok_or_else(MarkupError::internal)?
        };
        self.close(closed, target)?;
        if self.stack.len() == block.elem {
            return self.close_block(block, tag.span.start);
        }
        Ok(())
    }

    /// The block end tag starts at `end`: M-26 (size), then M-25.
    fn close_block(&mut self, block: OpenBlock, end: usize) -> Result<(), MarkupError> {
        self.block = None;
        let content = Span {
            start: block.start,
            end,
        };
        if end.saturating_sub(block.start) > MAX_BLOCK_BYTES {
            return Err(limit());
        }
        if block.format == Format::Json && !json::is_json_text(slice(self.input, content)?) {
            return Err(MarkupError::new(ErrorCode::BadJson, "M-25"));
        }
        self.blocks.push(RawBlock {
            id: block.id,
            format: block.format,
            content,
        });
        Ok(())
    }

    /// M-11 item 8 (H-71, DL-044): the pass over the content C of the
    /// `noscript` element that is the current node, and over the end tag
    /// `noscript` after it. C is the rest of the input when the element has
    /// no end tag (E3). The pass runs on the state of the main pass: the
    /// tokens of C use the same rules, so M-05 to M-10, M-12, M-13 and the
    /// limit on D apply to them, and the error is the one of the rule that
    /// fails.
    ///
    /// Items 1 to 3 and 5 to 7 and 9 are checked by
    /// [`Pass::noscript_start_rules`] and [`Pass::noscript_end_rules`]. When
    /// the pass succeeds, C is balanced (item 5), so the token stack and the
    /// marks are those of before C. D and the dirty mark of the current node
    /// can differ, and the pass restores them (DL-044 c). The stack of C is
    /// the part of the token stack above the `noscript` element, so it is
    /// bounded by D (M-26).
    fn check_noscript(&mut self, content: &[u8]) -> Result<(), MarkupError> {
        let mut text = Vec::with_capacity(content.len().saturating_add(NOSCRIPT_END.len()));
        text.extend_from_slice(content);
        text.extend_from_slice(NOSCRIPT_END);
        let base = self.stack.len();
        let saved_depth = self.depth;
        let saved_flags = self.stack.last().map(|e| (e.dirty, e.marked));
        let mut tokens = Tokenizer::new(&text, 0);
        self.in_noscript = true;
        loop {
            let token = tokens.next_token();
            if tokens.defect() {
                return Err(MarkupError::internal());
            }
            match token {
                Token::StartTag(tag) => {
                    self.noscript_start_rules(&text, &tag, base)?;
                    if let Some((kind, end_name)) = self.start_tag(&text, &tag)? {
                        tokens.set_raw(kind, end_name);
                    }
                }
                Token::EndTag(tag) => {
                    let raw_name = slice(&text, tag.name)?;
                    if self.noscript_end_rules(raw_name, &tag, base, content.len())? {
                        break;
                    }
                    self.end_tag_rules(raw_name, &tag)?;
                }
                // Item 1: C ends inside a tag, a comment or a raw text.
                Token::Eof => return Err(structure("M-11")),
                other => self.other_token(&other)?,
            }
        }
        self.in_noscript = false;
        self.depth = saved_depth;
        if let (Some(elem), Some((dirty, marked))) = (self.stack.last_mut(), saved_flags) {
            elem.dirty = dirty;
            elem.marked = marked;
        }
        Ok(())
    }

    /// M-11 items 2, 3, 6 and 7 for a start tag of C. `base` is the size of
    /// the token stack with the `noscript` element, so the elements of C are
    /// at the indices from `base`.
    fn noscript_start_rules(
        &mut self,
        src: &[u8],
        tag: &Tag,
        base: usize,
    ) -> Result<(), MarkupError> {
        use Known::{Body, Col, Frameset, Head, Html, Math, Noscript, Plaintext, Select, Table};
        let name = self.names.intern(slice(src, tag.name)?);
        // Item 4 (E8, H-22, H-28) is M-13 of the pass in `start_tag`. Item 2; item 3 (`svg` is not in the set, item 9); item 7.
        if tag.block_attr.is_some()
            || is_any(
                name,
                &[
                    Frameset,
                    Math,
                    Noscript,
                    Plaintext,
                    Select,
                    Known::Template,
                    Html,
                    Body,
                    Head,
                ],
            )
        {
            return Err(structure("M-11"));
        }
        // Item 6 (H-38): a table opened in C and still open.
        let part = self.names.has(name, flag::TABLE_PART) || name == Col.id();
        if part && self.names.top(Table.id()).is_none_or(|t| t < base) {
            return Err(structure("M-11"));
        }
        Ok(())
    }

    /// M-11 items 1, 5 and 7 for an end tag of C or the end tag after it.
    /// Returns true for the end tag `noscript` that ends the pass.
    fn noscript_end_rules(
        &mut self,
        raw_name: &[u8],
        tag: &Tag,
        base: usize,
        content_len: usize,
    ) -> Result<bool, MarkupError> {
        let name = self.names.lookup(raw_name);
        if name == Some(Known::Noscript.id()) {
            // Item 1: the first end tag `noscript` starts at the end of C.
            // Item 5: no element of C is open.
            return if tag.span.start == content_len && self.stack.len() == base {
                Ok(true)
            } else {
                Err(structure("M-11"))
            };
        }
        if name.is_some_and(|n| is_any(n, &[Known::Html, Known::Body, Known::Head])) {
            return Err(structure("M-11"));
        }
        // Item 5: no stray end tag, and the end tag matches the current node.
        // Exception (H-75): the end tag of the region root.
        let Some(current) = self.stack.last().filter(|_| self.stack.len() > base) else {
            return Err(structure("M-11"));
        };
        let region_root_close = name == Some(Known::Svg.id())
            && self.foreign_root.is_some()
            && self.names.top(Known::Svg.id()) == self.foreign_root;
        if name == Some(current.name) || region_root_close {
            Ok(false)
        } else {
            Err(structure("M-11"))
        }
    }

    fn finish(mut self) -> Result<Vec<RawBlock>, MarkupError> {
        if let Some(start) = self.noscript.take() {
            let input = self.input;
            self.check_noscript(slice(
                input,
                Span {
                    start,
                    end: input.len(),
                },
            )?)?;
        }
        if self.block.is_some() {
            return Err(structure("M-20"));
        }
        if self.html_select_open() {
            return Err(structure("M-08"));
        }
        Ok(self.blocks)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_span_outside_the_input_fails_closed() {
        let error = slice(b"abc", Span { start: 2, end: 4 }).unwrap_err();
        assert!(error.is_internal());
        assert_eq!(slice(b"abc", Span { start: 1, end: 3 }).unwrap(), b"bc");
    }
}
