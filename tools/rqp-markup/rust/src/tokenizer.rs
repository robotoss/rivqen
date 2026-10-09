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
