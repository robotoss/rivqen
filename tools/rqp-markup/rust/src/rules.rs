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

use crate::names::{
    Known, NameId, NameTable, RawKind, TABLE_PART_NAMES, VOID_NAMES, flag, raw_kind,
};
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
/// in any ASCII case (item 1), `&` (item 2) or a byte >= 0x80 (item 3, H-28).
fn is_reserved_type(value: &[u8]) -> bool {
    value.iter().any(|&b| b == b'&' || !b.is_ascii())
        || value
            .windows(b"rivqen-manifest".len())
            .any(|w| w.eq_ignore_ascii_case(b"rivqen-manifest"))
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
    ids: HashSet<&'a [u8]>,
    /// Offset after a `noscript` start tag in HTML context whose RAWTEXT
    /// content is not checked yet (M-11).
    noscript: Option<usize>,
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
    };
    let mut tokens = Tokenizer::new(input, start);
    loop {
        let token = tokens.next_token();
        if tokens.defect() {
            return Err(MarkupError::internal());
        }
        match token {
            Token::StartTag(tag) => {
                if let Some((kind, end_name)) = pass.start_tag(&tag)? {
                    tokens.set_raw(kind, end_name);
                }
            }
            Token::EndTag(tag) => pass.end_tag(&tag)?,
            Token::Comment(_) => pass.not_character()?,
            Token::Doctype(_) => pass.doctype()?,
            // E5 (H-21): a CDATA section that satisfies M-12 is allowed
            // also where M-07.3 and M-07.4 allow character tokens only.
            Token::Cdata { well_formed, .. } => {
                if !well_formed {
                    return Err(structure("M-12"));
                }
            }
            Token::Eof => return pass.finish(),
        }
    }
}

impl<'a> Pass<'a> {
    fn bytes(&self, span: Span) -> Result<&'a [u8], MarkupError> {
        slice(self.input, span)
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

    fn start_tag(&mut self, tag: &Tag) -> Result<RawSwitch, MarkupError> {
        let raw_name = self.bytes(tag.name)?;
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
        if name == Known::Script.id() {
            if let Some(value) = tag.type_attr {
                if is_reserved_type(self.bytes(value)?) {
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
            self.content_start(tag, name, block)?;
        } else if let Some(attr) = tag.block_attr {
            return self.block_start(tag, name, attr, foreign);
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

    /// M-09 with conditions 1 and 2 for a start tag in HTML context. It runs
    /// before the implied parts of M-26 and before the push.
    fn table_part(&self, name: NameId) -> Result<(), MarkupError> {
        use Known::{Caption, Col, Colgroup, Table, Tbody, Td, Tfoot, Th, Thead, Tr};
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
        let allowed: &[Known] = if is_any(name, &[Caption, Colgroup, Tbody, Thead, Tfoot]) {
            &[Table]
        } else if name == Col.id() {
            &[Table, Colgroup]
        } else if name == Tr.id() {
            &[Table, Tbody, Thead, Tfoot]
        } else if is_any(name, &[Td, Th]) {
            &[Table, Tbody, Thead, Tfoot, Tr]
        } else {
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
        tag: &Tag,
        name: NameId,
        attr: Span,
        foreign: bool,
    ) -> Result<RawSwitch, MarkupError> {
        let id = self.bytes(attr)?;
        if !is_valid_id(id) {
            return Err(MarkupError::new(ErrorCode::InvalidId, "M-15"));
        }
        let json_type = match tag.type_attr {
            Some(value) => self.bytes(value)? == b"application/json",
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
        if !self.ids.insert(id) {
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
        Ok(raw_kind(self.bytes(tag.name)?))
    }

    /// M-22 and M-24 for a start tag in the content of a markup block.
    fn content_start(&self, tag: &Tag, name: NameId, block: OpenBlock) -> Result<(), MarkupError> {
        use Known::{A, Button, Col, Colgroup, Dd, Dl, Dt, H1, H2, H3, H4, H5, H6, Li, Menu};
        use Known::{Nobr, Ol, P, Rb, Rp, Rt, Rtc, Ruby, Span, Table, Tbody, Tfoot, Thead, Tr, Ul};
        if self.names.has(name, flag::FORBIDDEN_CONTENT)
            || (tag.self_closing && !self.names.has(name, flag::VOID))
        {
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
            // g
            part && !in_content(&[Table]),
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

    fn end_tag(&mut self, tag: &Tag) -> Result<(), MarkupError> {
        if let Some(start) = self.noscript.take() {
            check_noscript(self.bytes(Span {
                start,
                end: tag.span.start,
            })?)?;
        }
        let name = self.names.lookup(self.bytes(tag.name)?);
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
            // context, or any element pushed in a foreign region.
            if !elem.html || self.names.has(elem.name, flag::SPECIAL) {
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
        if block.content == Content::Markup {
            if name.is_some_and(|n| self.names.has(n, flag::VOID)) {
                return Err(structure("M-22"));
            }
            if current.is_none() || current != name {
                return Err(structure("M-23"));
            }
        }
        // For a raw block the tokenizer returns only the appropriate end tag,
        // and the block element is the current node. In both cases the end
        // tag pops exactly the current node.
        let current = current.ok_or_else(MarkupError::internal)?;
        let index = self
            .stack
            .len()
            .checked_sub(1)
            .ok_or_else(MarkupError::internal)?;
        self.close(current, index)?;
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
        if block.format == Format::Json && !json::is_json_text(self.bytes(content)?) {
            return Err(MarkupError::new(ErrorCode::BadJson, "M-25"));
        }
        self.blocks.push(RawBlock {
            id: block.id,
            format: block.format,
            content,
        });
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<RawBlock>, MarkupError> {
        if let Some(start) = self.noscript.take() {
            check_noscript(self.bytes(Span {
                start,
                end: self.input.len(),
            })?)?;
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

/// Start tag names that M-11 forbids in `noscript` content.
const NOSCRIPT_FORBIDDEN: &[&[u8]] = &[
    b"frameset",
    b"math",
    b"noscript",
    b"plaintext",
    b"select",
    b"svg",
    b"template",
];

/// A tag name as the tokenizer emits it (M-03): ASCII lower case, and
/// U+0000 as U+FFFD.
fn normalized(raw: &[u8]) -> impl Iterator<Item = u8> + '_ {
    raw.iter().flat_map(|&byte| {
        let (bytes, len) = if byte == 0 {
            ([0xEF, 0xBF, 0xBD], 3)
        } else {
            ([byte.to_ascii_lowercase(), 0, 0], 1)
        };
        bytes.into_iter().take(len)
    })
}

/// M-11 (and M-13 for C, E8): tokenize the content C of a `noscript` element
/// followed by `</noscript>` from the data state, in HTML context. When the
/// `noscript` element has no end tag, C is the rest of the input (E3).
///
/// Item 5 (H-27) keeps a separate stack of the elements of C, as name spans
/// of the tokenized text. It grows only with the start tags of C (at least
/// 3 bytes each), so it is bounded by the input size (M-01). C is not
/// counted in D (M-26 item 7). Item 6 (H-38) counts the `table` elements on
/// that stack.
fn check_noscript(content: &[u8]) -> Result<(), MarkupError> {
    let mut text = Vec::with_capacity(content.len().saturating_add(11));
    text.extend_from_slice(content);
    text.extend_from_slice(b"</noscript>");
    let mut open: Vec<Span> = Vec::new();
    // Item 6: the number of `table` elements on `open`.
    let mut tables: usize = 0;
    let mut tokens = Tokenizer::new(&text, 0);
    loop {
        let token = tokens.next_token();
        if tokens.defect() {
            return Err(MarkupError::internal());
        }
        match token {
            Token::StartTag(tag) => {
                let name = slice(&text, tag.name)?;
                // E8 (H-22), H-28: M-13 applies to C too. A reader with
                // scripting disabled parses C as markup and would see a
                // manifest.
                if name.eq_ignore_ascii_case(b"script") {
                    if let Some(value) = tag.type_attr {
                        if is_reserved_type(slice(&text, value)?) {
                            return Err(MarkupError::new(ErrorCode::Reserved, "M-13"));
                        }
                    }
                }
                if tag.block_attr.is_some()
                    || NOSCRIPT_FORBIDDEN
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(name))
                {
                    return Err(structure("M-11"));
                }
                // Item 6 (H-38): with scripting disabled, a table part outside
                // a table of C closes the cell or caption around `noscript`.
                if tables == 0
                    && (name.eq_ignore_ascii_case(b"col")
                        || TABLE_PART_NAMES
                            .iter()
                            .any(|n| n.eq_ignore_ascii_case(name)))
                {
                    return Err(structure("M-11"));
                }
                // Item 5: a start tag that M-05 item 1 pushes opens an
                // element of C. `svg` and `math` are forbidden above, so
                // only the VOID names are not pushed.
                if !VOID_NAMES.iter().any(|v| v.eq_ignore_ascii_case(name)) {
                    open.push(tag.name);
                    if name.eq_ignore_ascii_case(b"table") {
                        tables = tables.checked_add(1).ok_or_else(MarkupError::internal)?;
                    }
                }
                if let Some((kind, end_name)) = raw_kind(name) {
                    tokens.set_raw(kind, end_name);
                }
            }
            Token::EndTag(tag) => {
                let name = slice(&text, tag.name)?;
                if name.eq_ignore_ascii_case(b"noscript") {
                    // Item 1, and item 5: no element of C is open.
                    return if tag.span.start == content.len() && open.is_empty() {
                        Ok(())
                    } else {
                        Err(structure("M-11"))
                    };
                }
                // Item 5: the end tag closes the last open element of C.
                let Some(last) = open.pop() else {
                    return Err(structure("M-11"));
                };
                if !normalized(slice(&text, last)?).eq(normalized(name)) {
                    return Err(structure("M-11"));
                }
                if name.eq_ignore_ascii_case(b"table") {
                    tables = tables.checked_sub(1).ok_or_else(MarkupError::internal)?;
                }
            }
            Token::Eof => return Err(structure("M-11")),
            Token::Comment(_) | Token::Doctype(_) | Token::Cdata { .. } => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn normalized_names_compare_as_the_tokenizer_emits_them() {
        let same = |a: &[u8], b: &[u8]| normalized(a).eq(normalized(b));
        assert!(same(b"DiV", b"div"));
        assert!(same(b"a\0", "a\u{FFFD}".as_bytes()));
        assert!(!same(b"a\0", b"a"));
        assert!(!same(b"div", b"dif"));
        assert!(!same("\u{C4}".as_bytes(), "\u{E4}".as_bytes()));
    }

    #[test]
    fn a_span_outside_the_input_fails_closed() {
        let error = slice(b"abc", Span { start: 2, end: 4 }).unwrap_err();
        assert!(error.is_internal());
        assert_eq!(slice(b"abc", Span { start: 1, end: 3 }).unwrap(), b"bc");
    }
}
