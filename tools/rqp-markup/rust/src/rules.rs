// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! One pass over the tokens: the token stack (M-05), the document rules
//! (M-06…M-13) and the block rules (M-14…M-26) of `markup.md` section 2.
//!
//! The pass returns at the first rule that fails. Each token costs O(1)
//! amortized work: the nearest element with a name is found through the
//! per-name pointer of [`NameTable`], and every element is popped at most
//! once.

use std::collections::HashSet;

use crate::names::{Known, NameId, NameTable, RawKind, flag, raw_kind};
use crate::tokenizer::{Span, Tag, Token, Tokenizer};
use crate::{ErrorCode, Format, MAX_BLOCK_BYTES, MAX_BLOCKS, MarkupError, json};

/// A block found by the pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RawBlock {
    pub(crate) id: Span,
    pub(crate) format: Format,
    pub(crate) content: Span,
}

#[derive(Clone, Copy, Debug)]
struct Elem {
    name: NameId,
    /// Pushed in HTML context (not in a foreign region).
    html: bool,
    /// The previous element with the same name (its stack index).
    prev_same: Option<usize>,
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

/// M-13: a script `type` raw value that is reserved.
fn is_reserved_type(value: &[u8]) -> bool {
    value.contains(&b'&')
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
    /// Stack indices of the elements pushed in HTML context whose name is in
    /// `TABLE_FAMILY` or is `template` (M-09, M-19).
    table_marks: Vec<usize>,
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
        stack: Vec::new(),
        table_marks: Vec::new(),
        foreign_root: None,
        text_only: None,
        block: None,
        blocks: Vec::new(),
        ids: HashSet::new(),
        noscript: None,
    };
    let mut tokens = Tokenizer::new(input, start);
    loop {
        match tokens.next_token() {
            Token::StartTag(tag) => {
                if let Some((kind, end_name)) = pass.start_tag(&tag)? {
                    tokens.set_raw(kind, end_name);
                }
            }
            Token::EndTag(tag) => pass.end_tag(&tag)?,
            Token::Comment(_) => pass.not_character()?,
            Token::Doctype(_) => pass.doctype()?,
            Token::Cdata { well_formed, .. } => {
                pass.not_character()?;
                if !well_formed {
                    return Err(structure("M-12"));
                }
            }
            Token::Eof => return pass.finish(),
        }
    }
}

impl<'a> Pass<'a> {
    fn bytes(&self, span: Span) -> &'a [u8] {
        self.input.get(span.start..span.end).unwrap_or_default()
    }

    fn is_open(&self, name: Known) -> bool {
        self.names.top(name.id()).is_some()
    }

    /// An element with this name was opened in the content of the block
    /// whose element is at `block_elem`, and it is still open.
    fn open_in_content(&self, name: Known, block_elem: usize) -> bool {
        self.names.top(name.id()).is_some_and(|i| i > block_elem)
    }

    /// M-09: the nearest element with a name in `TABLE_FAMILY`; the search
    /// stops at `template`.
    fn table_context(&self) -> Option<NameId> {
        let index = *self.table_marks.last()?;
        let name = self.stack.get(index)?.name;
        if self.names.has(name, flag::TEMPLATE) {
            None
        } else {
            Some(name)
        }
    }

    fn push(&mut self, name: NameId, html: bool) -> usize {
        let index = self.stack.len();
        let prev_same = self.names.replace_top(name, Some(index));
        self.stack.push(Elem {
            name,
            html,
            prev_same,
        });
        if html && self.names.has(name, flag::TABLE_FAMILY | flag::TEMPLATE) {
            self.table_marks.push(index);
        }
        index
    }

    fn pop(&mut self) -> Option<Elem> {
        let elem = self.stack.pop()?;
        let index = self.stack.len();
        self.names.replace_top(elem.name, elem.prev_same);
        if self.table_marks.last() == Some(&index) {
            self.table_marks.pop();
        }
        if self.foreign_root == Some(index) {
            self.foreign_root = None;
        }
        Some(elem)
    }

    /// M-07.3, M-07.4: a token that is not a character token while a
    /// foreign text-only element is open.
    fn not_character(&self) -> Result<(), MarkupError> {
        if self.text_only.is_some() {
            Err(structure("M-07"))
        } else {
            Ok(())
        }
    }

    fn doctype(&self) -> Result<(), MarkupError> {
        self.not_character()?;
        if self.is_open(Known::Select) {
            return Err(structure("M-08"));
        }
        if self.block.is_some_and(|b| b.content == Content::Markup) {
            return Err(structure("M-22"));
        }
        Ok(())
    }

    fn start_tag(&mut self, tag: &Tag) -> Result<RawSwitch, MarkupError> {
        let raw_name = self.bytes(tag.name);
        let name = self.names.intern(raw_name);
        let foreign = self.foreign_root.is_some();

        self.not_character()?;
        if self.is_open(Known::Select)
            && !is_any(name, &[Known::Option, Known::Optgroup, Known::Hr])
        {
            return Err(structure("M-08"));
        }
        if name == Known::Frameset.id() {
            return Err(structure("M-10"));
        }
        if name == Known::Script.id()
            && tag
                .type_attr
                .is_some_and(|value| is_reserved_type(self.bytes(value)))
        {
            return Err(MarkupError::new(ErrorCode::Reserved, "M-13"));
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
                self.push(name, false);
                if self.names.has(name, flag::INTEGRATION | flag::TEXT) {
                    self.text_only = Some(name);
                }
            }
            return Ok(None);
        }

        // M-05 step 1, HTML context.
        self.table_part(name)?;
        let is_root = name == Known::Svg.id() || name == Known::Math.id();
        if (is_root && tag.self_closing) || self.names.has(name, flag::VOID) {
            return Ok(None);
        }
        let index = self.push(name, true);
        if is_root {
            self.foreign_root = Some(index);
        }
        if name == Known::Noscript.id() {
            self.noscript = Some(tag.span.end);
        }
        // M-04: also with the self-closing flag.
        Ok(raw_kind(raw_name))
    }

    /// M-09 for a start tag in HTML context.
    fn table_part(&self, name: NameId) -> Result<(), MarkupError> {
        use Known::{Caption, Col, Colgroup, Table, Tbody, Td, Tfoot, Th, Thead, Tr};
        let allowed: &[Known] = if is_any(name, &[Caption, Colgroup, Tbody, Thead, Tfoot]) {
            &[Table]
        } else if name == Col.id() {
            &[Table, Colgroup]
        } else if name == Tr.id() {
            &[Table, Tbody, Thead, Tfoot]
        } else if is_any(name, &[Td, Th]) {
            &[Table, Tbody, Thead, Tfoot, Tr]
        } else {
            return Ok(());
        };
        match self.table_context() {
            Some(context) if !is_any(context, allowed) => Err(structure("M-09")),
            _ => Ok(()),
        }
    }

    /// M-15, M-16, M-18, M-19, M-21 and the block count of M-26 for a block
    /// start tag outside block content.
    fn block_start(
        &mut self,
        tag: &Tag,
        name: NameId,
        attr: Span,
        foreign: bool,
    ) -> Result<RawSwitch, MarkupError> {
        let id = self.bytes(attr);
        if !is_valid_id(id) {
            return Err(MarkupError::new(ErrorCode::InvalidId, "M-15"));
        }
        let kind = if foreign {
            None
        } else if self.names.has(name, flag::BLOCK_HTML) {
            let content = if name == Known::Title.id() {
                Content::Raw
            } else {
                Content::Markup
            };
            Some((Format::Html, content))
        } else if name == Known::Script.id()
            && tag
                .type_attr
                .is_some_and(|value| self.bytes(value) == b"application/json")
        {
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
            return Err(MarkupError::new(ErrorCode::Limit, "M-26"));
        }
        let elem = self.push(name, true);
        self.block = Some(OpenBlock {
            id: attr,
            format,
            content,
            start: tag.span.end,
            elem,
            name,
        });
        Ok(raw_kind(self.bytes(tag.name)))
    }

    /// M-22 and M-24 for a start tag in the content of a markup block.
    fn content_start(&self, tag: &Tag, name: NameId, block: OpenBlock) -> Result<(), MarkupError> {
        use Known::{A, Button, Col, Dd, Dl, Dt, H1, H2, H3, H4, H5, H6, Li, Menu, Nobr, Ol, P};
        use Known::{Rb, Rp, Rt, Rtc, Ruby, Span, Table, Ul};
        if self.names.has(name, flag::FORBIDDEN_CONTENT)
            || (tag.self_closing && !self.names.has(name, flag::VOID))
        {
            return Err(structure("M-22"));
        }
        let elem = block.elem;
        let in_content = |names: &[Known]| names.iter().any(|&k| self.open_in_content(k, elem));
        let open = |names: &[Known]| names.iter().any(|&k| self.is_open(k));
        let no_ruby = !in_content(&[Ruby]);
        let implied = [
            // a
            self.names.has(name, flag::PCLOSE) && is_any(block.name, &[P, Span]),
            // b
            self.names.has(name, flag::HEADING)
                && (self.names.has(block.name, flag::HEADING)
                    || in_content(&[H1, H2, H3, H4, H5, H6])),
            // c
            is_any(name, &[Li, Dd, Dt]) && !in_content(&[Ul, Ol, Menu, Dl]),
            // d, e, f
            is_any(name, &[Button, A, Nobr]) && self.names.top(name).is_some(),
            // g
            (self.names.has(name, flag::TABLE_PART) || name == Col.id()) && !in_content(&[Table]),
            // h
            is_any(name, &[Rb, Rtc]) && (no_ruby || open(&[Rb, Rp, Rt, Rtc])),
            // i
            is_any(name, &[Rp, Rt]) && (no_ruby || open(&[Rb, Rp, Rt])),
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
            }))?;
        }
        let name = self.names.lookup(self.bytes(tag.name));
        if let Some(text_only) = self.text_only {
            if name != Some(text_only) {
                return Err(structure("M-07"));
            }
            self.text_only = None;
        }
        if self.is_open(Known::Select)
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
        let Some(target) = target else {
            return Ok(());
        };
        let table_family = name.is_some_and(|n| self.names.has(n, flag::TABLE_FAMILY));
        while let Some(elem) = self.pop() {
            if self.stack.len() == target {
                break;
            }
            // M-06: the popped element is crossed.
            if elem.html
                && self.names.has(elem.name, flag::GUARDED)
                && !(table_family && self.names.has(elem.name, flag::TABLE_PART))
            {
                return Err(structure("M-06"));
            }
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
        if block.content == Content::Markup {
            if name.is_some_and(|n| self.names.has(n, flag::VOID)) {
                return Err(structure("M-22"));
            }
            let current = self.stack.last().map(|e| e.name);
            if current.is_none() || current != name {
                return Err(structure("M-23"));
            }
        }
        // For a raw block the tokenizer returns only the appropriate end tag,
        // and the block element is the current node.
        self.pop();
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
            return Err(MarkupError::new(ErrorCode::Limit, "M-26"));
        }
        if block.format == Format::Json && !json::is_json_text(self.bytes(content)) {
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
            }))?;
        }
        if self.block.is_some() {
            return Err(structure("M-20"));
        }
        if self.is_open(Known::Select) {
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

/// M-11: tokenize the content of a `noscript` element followed by
/// `</noscript>` from the data state, in HTML context.
fn check_noscript(content: &[u8]) -> Result<(), MarkupError> {
    let mut text = Vec::with_capacity(content.len().saturating_add(11));
    text.extend_from_slice(content);
    text.extend_from_slice(b"</noscript>");
    let mut tokens = Tokenizer::new(&text, 0);
    loop {
        match tokens.next_token() {
            Token::StartTag(tag) => {
                let name = text.get(tag.name.start..tag.name.end).unwrap_or_default();
                if tag.block_attr.is_some()
                    || NOSCRIPT_FORBIDDEN
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(name))
                {
                    return Err(structure("M-11"));
                }
                if let Some((kind, end_name)) = raw_kind(name) {
                    tokens.set_raw(kind, end_name);
                }
            }
            Token::EndTag(tag) => {
                let name = text.get(tag.name.start..tag.name.end).unwrap_or_default();
                if name.eq_ignore_ascii_case(b"noscript") {
                    return if tag.span.start == content.len() {
                        Ok(())
                    } else {
                        Err(structure("M-11"))
                    };
                }
            }
            Token::Eof => return Err(structure("M-11")),
            Token::Comment(_) | Token::Doctype(_) | Token::Cdata { .. } => {}
        }
    }
}
