// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! A subset of the WHATWG HTML tokenizer (HTML Living Standard §13.2.5) with
//! exact byte spans (`markup.md` M-03, section 3.1).
//!
//! The tokenizer emits only the tokens that the RQP rules use: start tags,
//! end tags, comments (including bogus comments and processing
//! instructions), DOCTYPE tokens and `<![CDATA[` sections. Character tokens
//! are not emitted. Character references are not decoded: no rule depends
//! on decoded text, and a character reference never changes a token
//! boundary.
//!
//! The caller sets the raw text state after a start tag (M-04). There is no
//! tree builder.
//!
//! Input stream preprocessing (CR and CR LF become LF) does not change
//! offsets. It is modelled by treating CR as white space: CR LF then gives
//! the same token boundaries as one LF, and a lone CR the same as LF.
//!
//! Every loop moves the position forward or changes to a state that moves it
//! forward on the same byte, so the work is linear in the input size.

use crate::names::RawKind;

/// A byte range `[start, end)` of the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// A start tag or an end tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tag {
    /// From `<` to the byte after `>`.
    pub(crate) span: Span,
    /// The raw tag name (not normalized).
    pub(crate) name: Span,
    /// The tag ends with `/>`.
    pub(crate) self_closing: bool,
    /// Raw value of the first `data-rq-block` attribute (empty span when the
    /// attribute has no value).
    pub(crate) block_attr: Option<Span>,
    /// Raw value of the first `type` attribute.
    pub(crate) type_attr: Option<Span>,
}

/// A token that the rules use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Token {
    StartTag(Tag),
    EndTag(Tag),
    /// A comment, a bogus comment or a processing instruction.
    Comment(Span),
    Doctype(Span),
    /// `<![CDATA[` read in the data state. `well_formed` is the M-12
    /// condition: the first `>` is the end of a `]]>` that starts after
    /// `<![CDATA[`, so a CDATA section and a bogus comment end at the same
    /// byte (the end of `span`).
    Cdata {
        span: Span,
        well_formed: bool,
    },
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Data,
    RcDataOrRawText,
    ScriptData,
    PlainText,
}

/// The tokenizer state over one input.
#[derive(Debug)]
pub(crate) struct Tokenizer<'a> {
    input: &'a [u8],
    pos: usize,
    mode: Mode,
    /// Lower-case name of the appropriate end tag in raw text modes.
    end_name: &'static [u8],
}

/// White space of the tokenizer (CR included, see the module comment).
fn is_ws(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}

fn next(i: usize) -> usize {
    i.saturating_add(1)
}

impl<'a> Tokenizer<'a> {
    /// A tokenizer in the data state at `start`.
    pub(crate) fn new(input: &'a [u8], start: usize) -> Self {
        Self {
            input,
            pos: start,
            mode: Mode::Data,
            end_name: b"",
        }
    }

    /// Switches to a raw text state after a start tag (M-04). `end_name` is
    /// the lower-case name of the appropriate end tag.
    pub(crate) fn set_raw(&mut self, kind: RawKind, end_name: &'static [u8]) {
        self.mode = match kind {
            RawKind::RcData | RawKind::RawText => Mode::RcDataOrRawText,
            RawKind::Script => Mode::ScriptData,
            RawKind::PlainText => Mode::PlainText,
        };
        self.end_name = end_name;
    }

    fn at(&self, i: usize) -> Option<u8> {
        self.input.get(i).copied()
    }

    fn len(&self) -> usize {
        self.input.len()
    }

    fn find(&self, from: usize, byte: u8) -> Option<usize> {
        let rest = self.input.get(from..)?;
        let offset = rest.iter().position(|&b| b == byte)?;
        from.checked_add(offset)
    }

    /// Offset after the first `>` at or after `from`, or the input length.
    fn after_gt(&self, from: usize) -> usize {
        self.find(from, b'>').map_or(self.len(), next)
    }

    fn starts_with(&self, at: usize, text: &[u8]) -> bool {
        self.input
            .get(at..)
            .is_some_and(|rest| rest.starts_with(text))
    }

    fn eof(&mut self) -> Token {
        self.pos = self.len();
        Token::Eof
    }

    /// The next token. After [`Token::Eof`] every call returns `Eof`.
    pub(crate) fn next_token(&mut self) -> Token {
        match self.mode {
            Mode::Data => self.data(),
            Mode::RcDataOrRawText => {
                self.mode = Mode::Data;
                self.raw_text()
            }
            Mode::ScriptData => {
                self.mode = Mode::Data;
                self.script_data()
            }
            Mode::PlainText => self.eof(),
        }
    }

    /// Data state, tag open state, end tag open state.
    fn data(&mut self) -> Token {
        loop {
            let Some(lt) = self.find(self.pos, b'<') else {
                return self.eof();
            };
            let after = next(lt);
            match self.at(after) {
                Some(b'!') => return self.markup_declaration(lt, next(after)),
                Some(b'/') => {
                    let name = next(after);
                    match self.at(name) {
                        Some(c) if c.is_ascii_alphabetic() => {
                            return match self.tag(lt, name, name) {
                                Some(tag) => Token::EndTag(tag),
                                None => self.eof(),
                            };
                        }
                        // `</>` emits nothing.
                        Some(b'>') => self.pos = next(name),
                        None => return self.eof(),
                        Some(_) => return self.bogus_comment(lt, name),
                    }
                }
                Some(c) if c.is_ascii_alphabetic() => {
                    return match self.tag(lt, after, after) {
                        Some(tag) => Token::StartTag(tag),
                        None => self.eof(),
                    };
                }
                Some(b'?') => return self.bogus_comment(lt, after),
                // `<` is a character; reconsume the next byte in the data state.
                Some(_) => self.pos = after,
                None => return self.eof(),
            }
        }
    }

    /// Markup declaration open state; `at` is the byte after `<!`.
    fn markup_declaration(&mut self, lt: usize, at: usize) -> Token {
        if self.starts_with(at, b"--") {
            return self.comment(lt, at.saturating_add(2));
        }
        let keyword = at.saturating_add(7);
        if self
            .input
            .get(at..keyword)
            .is_some_and(|s| s.eq_ignore_ascii_case(b"doctype"))
        {
            // Every DOCTYPE state ends the token at the first `>`.
            let end = self.after_gt(keyword);
            self.pos = end;
            return Token::Doctype(Span { start: lt, end });
        }
        if self.starts_with(at, b"[CDATA[") {
            let end = self.after_gt(keyword);
            self.pos = end;
            let well_formed = match self.find(keyword, b'>') {
                // No `>`: a CDATA section and a bogus comment both end at EOF.
                None => true,
                Some(gt) => gt
                    .checked_sub(2)
                    .is_some_and(|close| close >= keyword && self.starts_with(close, b"]]>")),
            };
            return Token::Cdata {
                span: Span { start: lt, end },
                well_formed,
            };
        }
        self.bogus_comment(lt, at)
    }

    /// Bogus comment state (also `<?`): ends at the first `>`.
    fn bogus_comment(&mut self, lt: usize, from: usize) -> Token {
        let end = self.after_gt(from);
        self.pos = end;
        Token::Comment(Span { start: lt, end })
    }

    /// The comment states, from the comment start state at `from`.
    fn comment(&mut self, lt: usize, from: usize) -> Token {
        #[derive(Clone, Copy)]
        enum S {
            Start,
            StartDash,
            Body,
            Lt,
            LtBang,
            LtBangDash,
            LtBangDashDash,
            EndDash,
            End,
            EndBang,
        }
        let mut state = S::Start;
        let mut i = from;
        loop {
            // EOF in every comment state emits the comment.
            let Some(c) = self.at(i) else {
                return self.emit_comment(lt, self.len());
            };
            match state {
                S::Start => match c {
                    b'-' => {
                        state = S::StartDash;
                        i = next(i);
                    }
                    b'>' => return self.emit_comment(lt, next(i)),
                    _ => state = S::Body,
                },
                S::StartDash => match c {
                    b'-' => {
                        state = S::End;
                        i = next(i);
                    }
                    b'>' => return self.emit_comment(lt, next(i)),
                    _ => state = S::Body,
                },
                S::Body => {
                    match c {
                        b'<' => state = S::Lt,
                        b'-' => state = S::EndDash,
                        _ => {}
                    }
                    i = next(i);
                }
                S::Lt => match c {
                    b'!' => {
                        state = S::LtBang;
                        i = next(i);
                    }
                    b'<' => i = next(i),
                    _ => state = S::Body,
                },
                S::LtBang => {
                    if c == b'-' {
                        state = S::LtBangDash;
                        i = next(i);
                    } else {
                        state = S::Body;
                    }
                }
                S::LtBangDash => {
                    if c == b'-' {
                        state = S::LtBangDashDash;
                        i = next(i);
                    } else {
                        state = S::EndDash;
                    }
                }
                // `>`, EOF and anything else: reconsume in the comment end state.
                S::LtBangDashDash => state = S::End,
                S::EndDash => {
                    if c == b'-' {
                        state = S::End;
                        i = next(i);
                    } else {
                        state = S::Body;
                    }
                }
                S::End => match c {
                    b'>' => return self.emit_comment(lt, next(i)),
                    b'!' => {
                        state = S::EndBang;
                        i = next(i);
                    }
                    b'-' => i = next(i),
                    _ => state = S::Body,
                },
                S::EndBang => match c {
                    b'-' => {
                        state = S::EndDash;
                        i = next(i);
                    }
                    b'>' => return self.emit_comment(lt, next(i)),
                    _ => state = S::Body,
                },
            }
        }
    }

    fn emit_comment(&mut self, lt: usize, end: usize) -> Token {
        self.pos = end;
        Token::Comment(Span { start: lt, end })
    }

    /// Tag name, attribute and self-closing states. `name_start` is the
    /// first byte of the name; the tag name state starts at `resume` (the
    /// raw text end tags have read their name already). Returns `None` on
    /// EOF in a tag: the tag is not emitted.
    #[allow(clippy::too_many_lines)]
    fn tag(&mut self, lt: usize, name_start: usize, resume: usize) -> Option<Tag> {
        #[derive(Clone, Copy)]
        enum S {
            Name,
            BeforeAttrName,
            AttrName,
            AfterAttrName,
            BeforeValue,
            Quoted(u8),
            Unquoted,
            AfterQuoted,
            SelfClosing,
        }
        let mut state = S::Name;
        let mut i = resume;
        let mut name_end = resume;
        let mut attr_start = resume;
        let mut value_start = resume;
        let mut attr = Attr::Other;
        let mut block_attr: Option<Span> = None;
        let mut type_attr: Option<Span> = None;
        let mut self_closing = false;

        loop {
            let Some(c) = self.at(i) else {
                self.pos = self.len();
                return None;
            };
            match state {
                S::Name => match c {
                    b'/' => {
                        name_end = i;
                        state = S::SelfClosing;
                        i = next(i);
                    }
                    b'>' => {
                        name_end = i;
                        break;
                    }
                    _ if is_ws(c) => {
                        name_end = i;
                        state = S::BeforeAttrName;
                        i = next(i);
                    }
                    _ => i = next(i),
                },
                S::BeforeAttrName => match c {
                    b'/' | b'>' => state = S::AfterAttrName,
                    _ if is_ws(c) => i = next(i),
                    // `=` here starts an attribute whose name begins with `=`.
                    b'=' => {
                        attr_start = i;
                        state = S::AttrName;
                        i = next(i);
                    }
                    _ => {
                        attr_start = i;
                        state = S::AttrName;
                    }
                },
                S::AttrName => {
                    if c == b'/' || c == b'>' || c == b'=' || is_ws(c) {
                        // Leaving the attribute name state: the first
                        // attribute with a name wins (M-03).
                        let name = self.input.get(attr_start..i).unwrap_or_default();
                        attr = if block_attr.is_none()
                            && name.eq_ignore_ascii_case(b"data-rq-block")
                        {
                            block_attr = Some(Span { start: i, end: i });
                            Attr::Block
                        } else if type_attr.is_none() && name.eq_ignore_ascii_case(b"type") {
                            type_attr = Some(Span { start: i, end: i });
                            Attr::Type
                        } else {
                            Attr::Other
                        };
                        if c == b'=' {
                            state = S::BeforeValue;
                            i = next(i);
                        } else {
                            state = S::AfterAttrName;
                        }
                    } else {
                        i = next(i);
                    }
                }
                S::AfterAttrName => match c {
                    b'/' => {
                        state = S::SelfClosing;
                        i = next(i);
                    }
                    b'=' => {
                        state = S::BeforeValue;
                        i = next(i);
                    }
                    b'>' => break,
                    _ if is_ws(c) => i = next(i),
                    _ => {
                        attr_start = i;
                        state = S::AttrName;
                    }
                },
                S::BeforeValue => match c {
                    b'"' | b'\'' => {
                        value_start = next(i);
                        state = S::Quoted(c);
                        i = next(i);
                    }
                    // Missing value: the value is empty and the tag ends.
                    b'>' => break,
                    _ if is_ws(c) => i = next(i),
                    _ => {
                        value_start = i;
                        state = S::Unquoted;
                    }
                },
                S::Quoted(quote) => {
                    if c == quote {
                        let value = Span {
                            start: value_start,
                            end: i,
                        };
                        set_value(attr, value, &mut block_attr, &mut type_attr);
                        state = S::AfterQuoted;
                    }
                    i = next(i);
                }
                S::Unquoted => {
                    if c == b'>' || is_ws(c) {
                        let value = Span {
                            start: value_start,
                            end: i,
                        };
                        set_value(attr, value, &mut block_attr, &mut type_attr);
                        if c == b'>' {
                            break;
                        }
                        state = S::BeforeAttrName;
                    }
                    i = next(i);
                }
                S::AfterQuoted => match c {
                    b'/' => {
                        state = S::SelfClosing;
                        i = next(i);
                    }
                    b'>' => break,
                    _ if is_ws(c) => {
                        state = S::BeforeAttrName;
                        i = next(i);
                    }
                    _ => state = S::BeforeAttrName,
                },
                S::SelfClosing => {
                    if c == b'>' {
                        self_closing = true;
                        break;
                    }
                    state = S::BeforeAttrName;
                }
            }
        }
        // `i` is the offset of the `>` that ends the tag.
        let end = next(i);
        self.pos = end;
        Some(Tag {
            span: Span { start: lt, end },
            name: Span {
                start: name_start,
                end: name_end,
            },
            self_closing,
            block_attr,
            type_attr,
        })
    }

    /// Scans ASCII letters from `from`. Returns the offset after them and
    /// whether they are the appropriate end tag name `name` followed by
    /// white space, `/` or `>`.
    fn end_name_at(&self, from: usize, name: &[u8]) -> (usize, bool) {
        let rest = self.input.get(from..).unwrap_or_default();
        let letters = rest
            .iter()
            .position(|b| !b.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let end = from.saturating_add(letters);
        let matches = rest
            .get(..letters)
            .is_some_and(|s| s.eq_ignore_ascii_case(name))
            && self
                .at(end)
                .is_some_and(|c| c == b'/' || c == b'>' || is_ws(c));
        (end, matches)
    }

    /// The appropriate end tag found at `lt`: tokenize it as an end tag.
    fn raw_end_tag(&mut self, lt: usize, name_start: usize, name_end: usize) -> Token {
        match self.tag(lt, name_start, name_end) {
            Some(tag) => Token::EndTag(tag),
            None => self.eof(),
        }
    }

    /// RCDATA and RAWTEXT states with their end tag states: the next token
    /// is the appropriate end tag, or EOF.
    fn raw_text(&mut self) -> Token {
        let name = self.end_name;
        let mut i = self.pos;
        loop {
            let Some(lt) = self.find(i, b'<') else {
                return self.eof();
            };
            let slash = next(lt);
            if self.at(slash) != Some(b'/') {
                i = slash;
                continue;
            }
            let name_start = next(slash);
            let (name_end, matches) = self.end_name_at(name_start, name);
            if matches {
                return self.raw_end_tag(lt, name_start, name_end);
            }
            // Not appropriate: the bytes are text; reconsume after the letters.
            i = name_end;
        }
    }

    /// Script data state with the escaped and double-escaped states.
    #[allow(clippy::too_many_lines)]
    fn script_data(&mut self) -> Token {
        #[derive(Clone, Copy)]
        enum S {
            Data,
            Lt,
            EndOpen,
            EscStart,
            EscStartDash,
            Esc,
            EscDash,
            EscDashDash,
            EscLt,
            EscEndOpen,
            DblStart,
            Dbl,
            DblDash,
            DblDashDash,
            DblLt,
            DblEnd,
        }
        let name = self.end_name;
        let mut state = S::Data;
        let mut i = self.pos;
        loop {
            let Some(c) = self.at(i) else {
                return self.eof();
            };
            match state {
                S::Data => {
                    if c == b'<' {
                        state = S::Lt;
                    }
                    i = next(i);
                }
                S::Lt => match c {
                    b'/' => {
                        state = S::EndOpen;
                        i = next(i);
                    }
                    b'!' => {
                        state = S::EscStart;
                        i = next(i);
                    }
                    _ => state = S::Data,
                },
                S::EndOpen | S::EscEndOpen => {
                    let back = if matches!(state, S::EndOpen) {
                        S::Data
                    } else {
                        S::Esc
                    };
                    if c.is_ascii_alphabetic() {
                        let (end, matches) = self.end_name_at(i, name);
                        if matches {
                            return self.raw_end_tag(i.saturating_sub(2), i, end);
                        }
                        i = end;
                    }
                    state = back;
                }
                S::EscStart => {
                    if c == b'-' {
                        state = S::EscStartDash;
                        i = next(i);
                    } else {
                        state = S::Data;
                    }
                }
                S::EscStartDash => {
                    if c == b'-' {
                        state = S::EscDashDash;
                        i = next(i);
                    } else {
                        state = S::Data;
                    }
                }
                S::Esc => {
                    match c {
                        b'-' => state = S::EscDash,
                        b'<' => state = S::EscLt,
                        _ => {}
                    }
                    i = next(i);
                }
                S::EscDash => {
                    state = match c {
                        b'-' => S::EscDashDash,
                        b'<' => S::EscLt,
                        _ => S::Esc,
                    };
                    i = next(i);
                }
                S::EscDashDash => {
                    state = match c {
                        b'-' => S::EscDashDash,
                        b'<' => S::EscLt,
                        b'>' => S::Data,
                        _ => S::Esc,
                    };
                    i = next(i);
                }
                S::EscLt => match c {
                    b'/' => {
                        state = S::EscEndOpen;
                        i = next(i);
                    }
                    _ if c.is_ascii_alphabetic() => state = S::DblStart,
                    _ => state = S::Esc,
                },
                S::DblStart | S::DblEnd => {
                    // The temporary buffer is the run of letters.
                    let (end, buffer_is_script) = self.end_name_at(i, b"script");
                    let (when_script, when_other) = if matches!(state, S::DblStart) {
                        (S::Dbl, S::Esc)
                    } else {
                        (S::Esc, S::Dbl)
                    };
                    match self.at(end) {
                        Some(d) if d == b'/' || d == b'>' || is_ws(d) => {
                            state = if buffer_is_script {
                                when_script
                            } else {
                                when_other
                            };
                            i = next(end);
                        }
                        _ => {
                            // Reconsume the byte after the letters.
                            state = if matches!(state, S::DblStart) {
                                S::Esc
                            } else {
                                S::Dbl
                            };
                            i = end;
                        }
                    }
                }
                S::Dbl => {
                    match c {
                        b'-' => state = S::DblDash,
                        b'<' => state = S::DblLt,
                        _ => {}
                    }
                    i = next(i);
                }
                S::DblDash => {
                    state = match c {
                        b'-' => S::DblDashDash,
                        b'<' => S::DblLt,
                        _ => S::Dbl,
                    };
                    i = next(i);
                }
                S::DblDashDash => {
                    state = match c {
                        b'-' => S::DblDashDash,
                        b'<' => S::DblLt,
                        b'>' => S::Data,
                        _ => S::Dbl,
                    };
                    i = next(i);
                }
                S::DblLt => {
                    if c == b'/' {
                        state = S::DblEnd;
                        i = next(i);
                    } else {
                        state = S::Dbl;
                    }
                }
            }
        }
    }
}

/// The attribute whose value the tag state machine reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Attr {
    /// The first `data-rq-block` attribute.
    Block,
    /// The first `type` attribute.
    Type,
    /// Any other attribute, or a later duplicate.
    Other,
}

/// Stores the raw value of the attribute that is read now.
fn set_value(attr: Attr, value: Span, block: &mut Option<Span>, ty: &mut Option<Span>) {
    match attr {
        Attr::Block => *block = Some(value),
        Attr::Type => *ty = Some(value),
        Attr::Other => {}
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::format_push_string
)]
mod tests {
    use super::*;
    use crate::names::raw_kind;

    /// Tokens of `input` as short strings. Start tags switch the raw text
    /// state as M-04 does in HTML context.
    fn tokens(input: &str) -> Vec<String> {
        let mut tokenizer = Tokenizer::new(input.as_bytes(), 0);
        let text = |span: Span| &input[span.start..span.end];
        let mut out = Vec::new();
        loop {
            match tokenizer.next_token() {
                Token::StartTag(tag) => {
                    let name = text(tag.name);
                    let mut d = format!("<{name}>{}..{}", tag.span.start, tag.span.end);
                    if tag.self_closing {
                        d.push_str(" /");
                    }
                    if let Some(v) = tag.block_attr {
                        d.push_str(" block=");
                        d += &format!("{:?}", text(v));
                    }
                    if let Some(v) = tag.type_attr {
                        d.push_str(" type=");
                        d += &format!("{:?}", text(v));
                    }
                    out.push(d);
                    if let Some((kind, end_name)) = raw_kind(name.as_bytes()) {
                        tokenizer.set_raw(kind, end_name);
                    }
                }
                Token::EndTag(tag) => out.push(format!(
                    "</{}>{}..{}",
                    text(tag.name),
                    tag.span.start,
                    tag.span.end
                )),
                Token::Comment(span) => out.push(format!("comment {}..{}", span.start, span.end)),
                Token::Doctype(span) => out.push(format!("doctype {}..{}", span.start, span.end)),
                Token::Cdata { span, well_formed } => out.push(format!(
                    "cdata {}..{} {}",
                    span.start,
                    span.end,
                    if well_formed { "ok" } else { "bad" }
                )),
                Token::Eof => return out,
            }
        }
    }

    #[test]
    fn start_and_end_tags_have_exact_spans() {
        assert_eq!(tokens("ab<p>c</p>"), ["<p>2..5", "</p>6..10"]);
        assert_eq!(tokens("<DiV\tclass=x\n>"), ["<DiV>0..14"]);
        assert_eq!(tokens("<a\rb=c>"), ["<a>0..7"], "CR is white space");
        assert_eq!(tokens("<a\x0Cb>"), ["<a>0..5"], "FF is white space");
        assert_eq!(tokens("</p class=\"a>b\" x>"), ["</p>0..18"]);
        assert_eq!(tokens("<p\0q>"), ["<p\0q>0..5"]);
    }

    #[test]
    fn bom_offset_is_skipped_by_the_caller() {
        let input = "\u{feff}<p>";
        let mut tokenizer = Tokenizer::new(input.as_bytes(), 3);
        let Token::StartTag(tag) = tokenizer.next_token() else {
            unreachable!("a start tag");
        };
        assert_eq!(tag.span, Span { start: 3, end: 6 });
        assert_eq!(tokenizer.next_token(), Token::Eof);
        assert_eq!(tokenizer.next_token(), Token::Eof);
    }

    #[test]
    fn attribute_values_are_raw_and_quoted_gt_does_not_end_a_tag() {
        assert_eq!(
            tokens("<p title=\"a>b\" data-rq-block=\"x&amp;\">"),
            ["<p>0..38 block=\"x&amp;\""]
        );
        assert_eq!(
            tokens("<p data-rq-block='s>q'>"),
            ["<p>0..23 block=\"s>q\""]
        );
        assert_eq!(
            tokens("<p data-rq-block='a\"b'>"),
            ["<p>0..23 block=\"a\\\"b\""]
        );
        assert_eq!(
            tokens("<p data-rq-block=uq id=y>"),
            ["<p>0..25 block=\"uq\""]
        );
        assert_eq!(tokens("<p data-rq-block=uq>"), ["<p>0..20 block=\"uq\""]);
        assert_eq!(
            tokens("<p data-rq-block=u'q\">"),
            ["<p>0..22 block=\"u'q\\\"\""]
        );
        assert_eq!(
            tokens("<p data-rq-block = \"v\" >"),
            ["<p>0..24 block=\"v\""]
        );
        assert_eq!(tokens("<p data-rq-block>"), ["<p>0..17 block=\"\""]);
        assert_eq!(tokens("<p data-rq-block x>"), ["<p>0..19 block=\"\""]);
        assert_eq!(tokens("<p data-rq-block=>"), ["<p>0..18 block=\"\""]);
        assert_eq!(tokens("<p data-rq-block=\"\"x>"), ["<p>0..21 block=\"\""]);
        assert_eq!(tokens("<p data-rq-block/>"), ["<p>0..18 / block=\"\""]);
        assert_eq!(
            tokens("<p a=1 data-rq-block=\"b\">"),
            ["<p>0..25 block=\"b\""]
        );
    }

    #[test]
    fn first_attribute_with_a_name_wins() {
        assert_eq!(
            tokens("<p DATA-RQ-BLOCK=\"a\" data-rq-block=\"b\">"),
            ["<p>0..39 block=\"a\""]
        );
        assert_eq!(
            tokens("<p data-rq-block data-rq-block=\"b\">"),
            ["<p>0..35 block=\"\""]
        );
        assert_eq!(
            tokens("<script type=a TYPE=b data-rq-block-x=c>"),
            ["<script>0..40 type=\"a\""]
        );
        assert_eq!(
            tokens("<script type data-rq-block=a type=b>"),
            ["<script>0..36 block=\"a\" type=\"\""]
        );
        assert_eq!(tokens("<p =data-rq-block=x>"), ["<p>0..20"]);
        assert_eq!(tokens("<p data-rq-blocks=x>"), ["<p>0..20"]);
        assert_eq!(tokens("<p xtype=x>"), ["<p>0..11"]);
    }

    #[test]
    fn self_closing_flag_needs_slash_gt() {
        assert_eq!(tokens("<br/>"), ["<br>0..5 /"]);
        assert_eq!(tokens("<br / >"), ["<br>0..7"]);
        assert_eq!(tokens("<br a=\"1\"/>"), ["<br>0..11 /"]);
        assert_eq!(tokens("<br a/>"), ["<br>0..7 /"]);
        assert_eq!(
            tokens("<br a=1/>"),
            ["<br>0..9"],
            "the slash is part of the value"
        );
        assert_eq!(tokens("<br/x>"), ["<br>0..6"]);
        assert_eq!(tokens("<a b=\"1\"c=2>"), ["<a>0..12"]);
    }

    #[test]
    fn eof_in_a_tag_emits_nothing() {
        for input in [
            "<div data-rq-block=\"x\"",
            "<div a='x>",
            "<div a=",
            "<div a= ",
            "<div a",
            "<div a ",
            "<div a=x",
            "<div /",
            "<div ",
            "<div",
            "</div",
            "<div a=\"b\"",
        ] {
            assert!(tokens(input).is_empty(), "{input:?}");
        }
    }

    #[test]
    fn less_than_that_is_text() {
        assert_eq!(tokens("a < b <<p> <1 <"), ["<p>7..10"]);
        assert!(tokens("</").is_empty());
        assert_eq!(tokens("</><p>"), ["<p>3..6"], "`</>` is dropped");
    }

    #[test]
    fn bogus_comments_end_at_the_first_gt() {
        assert_eq!(tokens("<!x <p a=\"b\">c"), ["comment 0..13"]);
        assert_eq!(tokens("</ <p>x"), ["comment 0..6"]);
        assert_eq!(tokens("</1>"), ["comment 0..4"]);
        assert_eq!(tokens("<?xml a=\">\"?>"), ["comment 0..10"]);
        assert_eq!(tokens("<!-x>"), ["comment 0..5"]);
        assert_eq!(tokens("<!>"), ["comment 0..3"]);
        assert_eq!(tokens("<!"), ["comment 0..2"]);
        assert_eq!(tokens("<?"), ["comment 0..2"]);
        assert_eq!(tokens("</ x"), ["comment 0..4"]);
        assert_eq!(tokens("<![cdata[x]]>"), ["comment 0..13"]);
    }

    #[test]
    fn comment_states() {
        assert_eq!(tokens("<!---->"), ["comment 0..7"]);
        assert_eq!(tokens("<!-->x"), ["comment 0..5"]);
        assert_eq!(tokens("<!--->x"), ["comment 0..6"]);
        assert_eq!(tokens("<!-- a --!><p>"), ["comment 0..11", "<p>11..14"]);
        assert_eq!(tokens("<!-- a -- b -->"), ["comment 0..15"]);
        assert_eq!(tokens("<!-- a --->"), ["comment 0..11"]);
        assert_eq!(tokens("<!-- a --!x -->"), ["comment 0..15"]);
        assert_eq!(tokens("<!-- a --!-->"), ["comment 0..13"]);
        assert_eq!(tokens("<!-- <p> -> --x>-->"), ["comment 0..19"]);
        assert_eq!(tokens("<!--x-y-->"), ["comment 0..10"]);
        assert_eq!(tokens("<!---x-->"), ["comment 0..9"]);
        assert_eq!(tokens("<!-- x"), ["comment 0..6"]);
        assert_eq!(tokens("<!--"), ["comment 0..4"]);
        assert_eq!(tokens("<!---"), ["comment 0..5"]);
        assert_eq!(tokens("<!-- --"), ["comment 0..7"]);
        assert_eq!(tokens("<!-- --!"), ["comment 0..8"]);
        assert_eq!(tokens("<!-- -"), ["comment 0..6"]);
    }

    #[test]
    fn comment_ends_before_the_next_token() {
        // A token after the comment shows that the comment does not run
        // to EOF.
        assert_eq!(tokens("<!----><p>"), ["comment 0..7", "<p>7..10"]);
        assert_eq!(tokens("<!-- a ---><p>"), ["comment 0..11", "<p>11..14"]);
        assert_eq!(tokens("<!-- a --!><p>"), ["comment 0..11", "<p>11..14"]);
        assert_eq!(tokens("<!--><p>"), ["comment 0..5", "<p>5..8"]);
        assert_eq!(tokens("<!---><p>"), ["comment 0..6", "<p>6..9"]);
    }

    #[test]
    fn comment_less_than_sign_states_do_not_end_a_comment_early() {
        // `<!x-` and `<!-x` inside a comment: the `>` after them is text.
        assert_eq!(tokens("<!--<!x-><p>--><b>"), ["comment 0..15", "<b>15..18"]);
        assert_eq!(tokens("<!--<!-x><p>--><b>"), ["comment 0..15", "<b>15..18"]);
        // `<!--` inside a comment, then `>`: the comment ends.
        assert_eq!(tokens("<!--<!--><b>"), ["comment 0..9", "<b>9..12"]);
    }

    #[test]
    fn missing_white_space_after_a_quoted_value_starts_a_new_attribute() {
        assert_eq!(
            tokens("<p a=\"1\"data-rq-block=\"b\">"),
            ["<p>0..26 block=\"b\""]
        );
        assert_eq!(
            tokens("<script type='x'TYPE=y data-rq-block=c>"),
            ["<script>0..39 block=\"c\" type=\"x\""]
        );
    }

    #[test]
    fn equals_sign_before_an_attribute_name() {
        // `=` starts an attribute named `=`; the next attribute is a new one.
        assert_eq!(
            tokens("<p = data-rq-block=\"b\">"),
            ["<p>0..23 block=\"b\""]
        );
        assert_eq!(
            tokens("<p  = data-rq-block=\"b\">"),
            ["<p>0..24 block=\"b\""]
        );
    }

    #[test]
    fn comment_less_than_sign_states() {
        assert_eq!(tokens("<!--<!-->"), ["comment 0..9"]);
        assert_eq!(tokens("<!--<!--->"), ["comment 0..10"]);
        assert_eq!(tokens("<!--<!-x-->"), ["comment 0..11"]);
        assert_eq!(tokens("<!--<!x-->"), ["comment 0..10"]);
        assert_eq!(tokens("<!--<<!-->"), ["comment 0..10"]);
        assert_eq!(tokens("<!--<x-->"), ["comment 0..9"]);
        assert_eq!(tokens("<!--<!--x-->"), ["comment 0..12"]);
        assert_eq!(tokens("<!--<!--!>"), ["comment 0..10"]);
    }

    #[test]
    fn doctype_ends_at_the_first_gt() {
        assert_eq!(tokens("<!DOCTYPE html>"), ["doctype 0..15"]);
        assert_eq!(tokens("<!doctype html PUBLIC \"a>b\">"), ["doctype 0..25"]);
        assert_eq!(tokens("<!DocType>"), ["doctype 0..10"]);
        assert_eq!(tokens("<!DOCTYPE html"), ["doctype 0..14"]);
        assert_eq!(tokens("<!DOCTYP>"), ["comment 0..9"]);
    }

    #[test]
    fn cdata_condition_of_m12() {
        assert_eq!(tokens("<![CDATA[ a < b ]]>"), ["cdata 0..19 ok"]);
        assert_eq!(tokens("<![CDATA[]]>"), ["cdata 0..12 ok"]);
        assert_eq!(tokens("<![CDATA[ a > b ]]>"), ["cdata 0..13 bad"]);
        assert_eq!(tokens("<![CDATA[]>"), ["cdata 0..11 bad"]);
        assert_eq!(tokens("<![CDATA[>"), ["cdata 0..10 bad"]);
        assert_eq!(tokens("<![CDATA[x]>"), ["cdata 0..12 bad"]);
        assert_eq!(tokens("<![CDATA[x"), ["cdata 0..10 ok"]);
    }

    #[test]
    fn rcdata_and_rawtext_end_at_the_appropriate_end_tag() {
        assert_eq!(
            tokens("<title><p a=b></titlex></TITLE ><p>"),
            ["<title>0..7", "</TITLE>23..32", "<p>32..35"]
        );
        assert_eq!(
            tokens("<textarea></textarea/></p>"),
            ["<textarea>0..10", "</textarea>10..22", "</p>22..26"]
        );
        assert_eq!(
            tokens("<style></style a=\">\"><p>"),
            ["<style>0..7", "</style>7..21", "<p>21..24"]
        );
        assert_eq!(tokens("<xmp></xm></xmp1></ xmp>"), ["<xmp>0..5"]);
        assert_eq!(tokens("<title></title"), ["<title>0..7"]);
        assert_eq!(tokens("<title></title a"), ["<title>0..7"]);
        assert_eq!(
            tokens("<iframe><</</iframe\n>"),
            ["<iframe>0..8", "</iframe>11..21"]
        );
        assert_eq!(tokens("<title>a<"), ["<title>0..7"]);
    }

    #[test]
    fn plaintext_never_ends() {
        assert_eq!(tokens("<plaintext></plaintext><p>"), ["<plaintext>0..11"]);
    }

    #[test]
    fn script_data_states() {
        // (input, which occurrence of `</script` ends the script data, if any)
        let cases: &[(&str, Option<usize>)] = &[
            ("<script>a<b</p></script>", Some(0)),
            // Escaped: `</script>` still ends the script.
            ("<script><!-- </script>", Some(0)),
            // Double escaped: `</script>` returns to escaped, `-->` to data.
            ("<script><!--<script>a</script>b--></script>", Some(1)),
            ("<script><!--<SCRIPT/></script\t>--></script>", Some(1)),
            // Not double escaped: `<scripts>` and `<p>`.
            ("<script><!--<scripts></script>", Some(0)),
            ("<script><!--<p></script>", Some(0)),
            // `<!-` alone or `<!x` does not start an escape.
            ("<script><!-<script></script>", Some(0)),
            ("<script><!x<script></script>", Some(0)),
            ("<script><<!--<script></script>--></script>", Some(1)),
            // `-->` ends the escape; `<!--` must start it again.
            ("<script><!-- --><script></script>", Some(0)),
            ("<script><!--><script></script>", Some(0)),
            ("<script><!---><script></script>", Some(0)),
            ("<script><!-- -x-<script></script>--></script>", Some(1)),
            ("<script><!-- --x<script></script>--></script>", Some(1)),
            ("<script><!-- -<script></script>--></script>", Some(1)),
            ("<script><!-- --<script></script>--></script>", Some(1)),
            ("<script><!-- ---<script></script>--></script>", Some(1)),
            ("<script><!--x<script></script>--></script>", Some(1)),
            // Escaped end tag names that are not `script`.
            ("<script><!-- </scrip></script1></script>", Some(1)),
            ("<script><!-- <1</ </script>", Some(0)),
            // In the double-escaped state.
            (
                "<script><!--<script>-<!-</scrip></script x>--></script>",
                Some(1),
            ),
            ("<script><!--<script>--<-></ script>--></script>", Some(0)),
            ("<script><!--<script>--></script>", Some(0)),
            ("<script><!--<script>---></script>", Some(0)),
            ("<script><!--<script>-></script>--></script>", Some(1)),
            ("<script><!--<script>-x</script>--></script>", Some(1)),
            ("<script><!--<script>--x</script>--></script>", Some(1)),
            ("<script><!--<script><</script>--></script>", Some(1)),
            ("<script><!--<script><x</script>--></script>", Some(1)),
            ("<script><!--<script></script\n--></script>", Some(1)),
            (
                "<script><!--<script></script1></script>--></script>",
                Some(2),
            ),
            ("<script><!--<script></scriptx --></script>", Some(1)),
            ("<script><!--<script></script", None),
            ("<script></", None),
            ("<script><!--<scr", None),
            ("<script></script", None),
            ("<script></scriptx>", None),
            ("<script></1></ script></script>", Some(0)),
            // `</script>` in the double-escaped state returns to the escaped
            // state, where the next `</script>` ends the script.
            ("<script><!--<script>a</script></script>-->", Some(1)),
            ("<script><!--<script>-</script></script>-->", Some(1)),
            ("<script><!--<script>--</script></script>-->", Some(1)),
            ("<script><!--<script></x</script></script>-->", Some(1)),
            // A `<` after a non-`script` name is reconsumed.
            ("<script><!--<x<script></script>--></script>", Some(1)),
        ];
        for (input, which) in cases {
            let expected: Vec<String> = which
                .iter()
                .map(|&n| {
                    let start = input.match_indices("</script").nth(n).unwrap().0;
                    let end = start + input[start..].find('>').unwrap() + 1;
                    format!("</script>{start}..{end}")
                })
                .collect();
            let got = tokens(input);
            assert_eq!(got[0], "<script>0..8", "{input}");
            assert_eq!(got[1..], expected, "{input}");
        }
        // After the end tag the data state continues.
        assert_eq!(
            tokens("<script>x</script><p>"),
            ["<script>0..8", "</script>9..18", "<p>18..21"]
        );
    }
}
