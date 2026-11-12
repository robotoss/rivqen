// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Test-only reference of P-TPL-3 (DL-162): a small backtracking matcher of
//! regular expressions, and the canonical split written on top of it from
//! the expression text of legacy wire C-06 and C-07.
//!
//! The matcher knows only what the two expressions use: literal bytes, the
//! escapes `\w` (`[A-Za-z0-9_]`), `\s`, `\S`, `\n`, `\/`, classes `[...]`
//! with `^`, capturing groups, and the quantifiers `?`, `*`, `+` with an
//! optional `?` for the lazy form. It runs as a classic backtracking engine:
//! leftmost start, then the first path in priority order (greedy tries more
//! first, lazy tries fewer first). Matching is ASCII case-insensitive. The
//! time is exponential at worst, so it runs on small inputs only.

#![allow(
    dead_code,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_panics_doc
)]

use std::collections::BTreeSet;

use rivqen_template::legacy::{DataBlock, Diagnostic, LimitError, NotApplied, Split, SplitOutcome};
use rivqen_template::limits::TemplateLimits;

/// Expression of the title rule C-06 (legacy wire section 10).
pub const TITLE_EXPRESSION: &str = r"<title([^\n]*?)</title>";
/// Expression of the block rule C-07 (legacy wire section 10, legacy markers
/// section 1).
pub const BLOCK_EXPRESSION: &str = r"<!--sonicdiff-?(\w*)-->([\s\S]+?)<!--sonicdiff-?(\w*)-end-->";

#[derive(Debug, Clone)]
enum Node {
    Byte(u8),
    Class(Box<[bool; 256]>),
    Group(usize, Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: usize,
        max: Option<usize>,
        greedy: bool,
    },
}

/// A compiled expression.
#[derive(Debug, Clone)]
pub struct Regex {
    nodes: Vec<Node>,
    groups: usize,
}

/// Capture ranges; index 0 is the whole match.
pub type Captures = Vec<Option<(usize, usize)>>;

fn class_of_escape(escape: u8) -> [bool; 256] {
    let mut set = [false; 256];
    match escape {
        b'w' => {
            for byte in 0..=255_u8 {
                set[usize::from(byte)] = byte.is_ascii_alphanumeric() || byte == b'_';
            }
        }
        b's' | b'S' => {
            for byte in [b' ', b'\t', b'\n', 0x0b, 0x0c, b'\r'] {
                set[usize::from(byte)] = true;
            }
            if escape == b'S' {
                for flag in &mut set {
                    *flag = !*flag;
                }
            }
        }
        b'n' => set[usize::from(b'\n')] = true,
        other => set[usize::from(other)] = true,
    }
    set
}

struct Parser<'a> {
    text: &'a [u8],
    pos: usize,
    groups: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.pos).copied()
    }

    fn next(&mut self) -> u8 {
        let byte = self.text[self.pos];
        self.pos += 1;
        byte
    }

    fn sequence(&mut self) -> Vec<Node> {
        let mut nodes = Vec::new();
        while let Some(byte) = self.peek() {
            if byte == b')' {
                break;
            }
            let atom = self.atom();
            nodes.push(self.quantified(atom));
        }
        nodes
    }

    fn atom(&mut self) -> Node {
        match self.next() {
            b'(' => {
                self.groups += 1;
                let index = self.groups;
                let inner = self.sequence();
                assert_eq!(self.next(), b')', "unclosed group");
                Node::Group(index, inner)
            }
            b'[' => {
                let negated = self.peek() == Some(b'^');
                if negated {
                    self.pos += 1;
                }
                let mut set = [false; 256];
                loop {
                    let byte = self.next();
                    if byte == b']' {
                        break;
                    }
                    let part = if byte == b'\\' {
                        class_of_escape(self.next())
                    } else {
                        class_of_escape(byte)
                    };
                    for (flag, add) in set.iter_mut().zip(part) {
                        *flag |= add;
                    }
                }
                if negated {
                    for flag in &mut set {
                        *flag = !*flag;
                    }
                }
                Node::Class(Box::new(set))
            }
            b'\\' => match self.next() {
                escape @ (b'w' | b's' | b'S') => Node::Class(Box::new(class_of_escape(escape))),
                b'n' => Node::Byte(b'\n'),
                other => Node::Byte(other),
            },
            byte @ (b'?' | b'*' | b'+' | b'|' | b'.' | b'^' | b'$') => {
                panic!("unsupported syntax {}", char::from(byte))
            }
            byte => Node::Byte(byte),
        }
    }

    fn quantified(&mut self, atom: Node) -> Node {
        let (min, max) = match self.peek() {
            Some(b'?') => (0, Some(1)),
            Some(b'*') => (0, None),
            Some(b'+') => (1, None),
            _ => return atom,
        };
        self.pos += 1;
        let greedy = if self.peek() == Some(b'?') {
            self.pos += 1;
            false
        } else {
            true
        };
        Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            greedy,
        }
    }
}

impl Regex {
    /// Compile `expression`.
    #[must_use]
    pub fn new(expression: &str) -> Self {
        let mut parser = Parser {
            text: expression.as_bytes(),
            pos: 0,
            groups: 0,
        };
        let nodes = parser.sequence();
        assert_eq!(parser.pos, expression.len(), "unbalanced expression");
        Self {
            nodes,
            groups: parser.groups,
        }
    }

    /// The leftmost match at or after `from`, ASCII case-insensitive.
    #[must_use]
    pub fn find_at(&self, input: &[u8], from: usize) -> Option<Captures> {
        let matcher = Matcher { input };
        for start in from..=input.len() {
            let mut caps: Captures = vec![None; self.groups + 1];
            let mut found = None;
            if matcher.sequence(&self.nodes, start, &mut caps, &mut |end, caps| {
                let mut all = caps.clone();
                all[0] = Some((start, end));
                found = Some(all);
                true
            }) {
                return found;
            }
        }
        None
    }

    /// All matches from left to right without overlap.
    #[must_use]
    pub fn find_all(&self, input: &[u8]) -> Vec<Captures> {
        let mut out = Vec::new();
        let mut from = 0;
        while from <= input.len() {
            let Some(caps) = self.find_at(input, from) else {
                break;
            };
            let (start, end) = caps[0].unwrap();
            from = if end > start { end } else { end + 1 };
            out.push(caps);
        }
        out
    }
}

struct Matcher<'a> {
    input: &'a [u8],
}

type Continuation<'k> = &'k mut dyn FnMut(usize, &mut Captures) -> bool;

impl Matcher<'_> {
    fn one(&self, node: &Node, pos: usize) -> bool {
        let Some(&byte) = self.input.get(pos) else {
            return false;
        };
        match node {
            Node::Byte(expected) => byte.eq_ignore_ascii_case(expected),
            Node::Class(set) => {
                set[usize::from(byte)]
                    || set[usize::from(byte.to_ascii_lowercase())]
                    || set[usize::from(byte.to_ascii_uppercase())]
            }
            _ => unreachable!(),
        }
    }

    fn sequence(
        &self,
        nodes: &[Node],
        pos: usize,
        caps: &mut Captures,
        k: Continuation<'_>,
    ) -> bool {
        let Some((first, rest)) = nodes.split_first() else {
            return k(pos, caps);
        };
        match first {
            Node::Byte(_) | Node::Class(_) => {
                self.one(first, pos) && self.sequence(rest, pos + 1, caps, k)
            }
            Node::Group(index, inner) => {
                let index = *index;
                self.sequence(inner, pos, caps, &mut |end, caps| {
                    let saved = caps[index];
                    caps[index] = Some((pos, end));
                    if self.sequence(rest, end, caps, k) {
                        return true;
                    }
                    caps[index] = saved;
                    false
                })
            }
            Node::Repeat {
                node,
                min,
                max,
                greedy,
            } => self.repeat(node, *min, *max, *greedy, 0, pos, rest, caps, k),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn repeat(
        &self,
        node: &Node,
        min: usize,
        max: Option<usize>,
        greedy: bool,
        count: usize,
        pos: usize,
        rest: &[Node],
        caps: &mut Captures,
        k: Continuation<'_>,
    ) -> bool {
        let may_stop = count >= min;
        let may_more = max.is_none_or(|max| count < max);
        let more = |caps: &mut Captures, k: Continuation<'_>| {
            may_more
                && self.sequence(std::slice::from_ref(node), pos, caps, &mut |end, caps| {
                    end != pos && self.repeat(node, min, max, greedy, count + 1, end, rest, caps, k)
                })
        };
        // The order of the two tries is the priority of the paths (the
        // operands of `||` are not commutative here).
        #[allow(clippy::if_same_then_else)]
        if greedy {
            more(caps, k) || (may_stop && self.sequence(rest, pos, caps, k))
        } else {
            (may_stop && self.sequence(rest, pos, caps, k)) || more(caps, k)
        }
    }
}

/// The canonical split (C-06 to C-08 with the limits of DL-163), computed
/// with [`Regex`]. Policy order for each match of C-07, in document order:
/// name empty or over `max_name_bytes` (stays in the template), repeated
/// name, name `title`, block count, block size.
///
/// # Errors
///
/// As `split_v1`.
pub fn split(html: &[u8], limits: &TemplateLimits) -> Result<SplitOutcome, LimitError> {
    if html.len() > limits.max_document_bytes {
        return Err(LimitError::DocumentTooLarge);
    }
    if std::str::from_utf8(html).is_err() {
        return Ok(SplitOutcome::NotApplied(NotApplied::InvalidUtf8));
    }
    let (text, title) = match Regex::new(TITLE_EXPRESSION).find_at(html, 0) {
        Some(caps) => {
            let (start, end) = caps[0].unwrap();
            let mut text = html[..start].to_vec();
            text.extend_from_slice(b"{title}");
            text.extend_from_slice(&html[end..]);
            (text, html[start..end].to_vec())
        }
        None => (html.to_vec(), Vec::new()),
    };
    let mut template = Vec::new();
    let mut data = vec![DataBlock {
        key: "{title}".to_owned(),
        value: String::from_utf8(title).unwrap(),
    }];
    let mut diagnostics = Vec::new();
    let mut names = BTreeSet::new();
    let mut cursor = 0;
    for caps in Regex::new(BLOCK_EXPRESSION).find_all(&text) {
        let (start, end) = caps[0].unwrap();
        let (name_start, name_end) = caps[1].unwrap();
        let (end_start, end_end) = caps[3].unwrap();
        let name = &text[name_start..name_end];
        if name.is_empty() || name.len() > limits.max_name_bytes {
            continue;
        }
        if !names.insert(name.to_vec()) {
            return Ok(SplitOutcome::NotApplied(NotApplied::DuplicateName));
        }
        if name == b"title" {
            return Ok(SplitOutcome::NotApplied(NotApplied::BlockNamedTitle));
        }
        if data.len() > limits.max_blocks {
            return Err(LimitError::TooManyBlocks);
        }
        if end - start > limits.max_block_bytes {
            return Err(LimitError::BlockTooLarge);
        }
        let key = format!("{{{}}}", String::from_utf8(name.to_vec()).unwrap());
        if text[end_start..end_end] != *name {
            diagnostics.push(Diagnostic::MismatchedEndName { index: data.len() });
        }
        template.extend_from_slice(&text[cursor..start]);
        template.extend_from_slice(key.as_bytes());
        data.push(DataBlock {
            key,
            value: String::from_utf8(text[start..end].to_vec()).unwrap(),
        });
        cursor = end;
    }
    template.extend_from_slice(&text[cursor..]);
    Ok(SplitOutcome::Applied(Split {
        template: String::from_utf8(template).unwrap(),
        data,
        diagnostics,
    }))
}

/// The template positions of the placeholders of a split computed with
/// [`Regex`]: `(position, key)` for `{title}` (when the title matched and
/// the `{title}` text is not inside a keyed block) and for each keyed block,
/// in template order. `None` when the split does not apply.
#[must_use]
pub fn placeholders(html: &[u8], limits: &TemplateLimits) -> Option<Vec<(usize, String)>> {
    let Ok(SplitOutcome::Applied(_)) = split(html, limits) else {
        return None;
    };
    let (text, title_at) = match Regex::new(TITLE_EXPRESSION).find_at(html, 0) {
        Some(caps) => {
            let (start, end) = caps[0].unwrap();
            let mut text = html[..start].to_vec();
            text.extend_from_slice(b"{title}");
            text.extend_from_slice(&html[end..]);
            (text, Some(start))
        }
        None => (html.to_vec(), None),
    };
    let mut out = Vec::new();
    let mut template_len = 0;
    let mut cursor = 0;
    // A copied text range `cursor..until` that holds the `{title}` text.
    let copy =
        |out: &mut Vec<(usize, String)>, template_len: &mut usize, cursor: usize, until: usize| {
            if let Some(title) = title_at
                && cursor <= title
                && title < until
            {
                out.push((*template_len + title - cursor, "{title}".to_owned()));
            }
            *template_len += until - cursor;
        };
    for caps in Regex::new(BLOCK_EXPRESSION).find_all(&text) {
        let (start, end) = caps[0].unwrap();
        let (name_start, name_end) = caps[1].unwrap();
        let name = &text[name_start..name_end];
        if name.is_empty() || name.len() > limits.max_name_bytes {
            continue;
        }
        copy(&mut out, &mut template_len, cursor, start);
        let key = format!("{{{}}}", String::from_utf8(name.to_vec()).unwrap());
        out.push((template_len, key.clone()));
        template_len += key.len();
        cursor = end;
    }
    copy(&mut out, &mut template_len, cursor, text.len());
    Some(out)
}
