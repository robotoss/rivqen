// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Port of `tools/rqp-markup/rust/src/names.rs` (commit `92c708e1`; DL-161).
//! Keep the rules equal to the candidate: a change of the markup rules
//! changes both copies (WP-06 S1, risk R-2). Changed in both copies in
//! WP-06 S2: the name table keeps only the rule names and the open names
//! (finding S1-3).
//!
//! Tag names: interning, the name sets of `markup.md` section 2.1, and the
//! per-name "nearest open element" pointer of the token stack.
//!
//! Every tag name is normalized as the WHATWG tokenizer does it: ASCII upper
//! case becomes lower case and U+0000 becomes U+FFFD. Two raw names are the
//! same name when their normalized bytes are equal.
//!
//! The table keeps a name that is not a name of the rules only while an
//! element with this name is open (WP-06 S2, security review finding S1-3;
//! see [`NameTable`]). Its id is then given to another name: do not keep
//! the id of such a name after the last element with it is popped. The
//! rules keep ids only in the token stack (and of rule names).

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// Index of an interned tag name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NameId(usize);

/// Name-set flags (`markup.md` section 2.1).
pub(crate) mod flag {
    /// `VOID`
    pub(crate) const VOID: u32 = 1;
    /// `PCLOSE`
    pub(crate) const PCLOSE: u32 = 1 << 1;
    /// `GUARDED`
    pub(crate) const GUARDED: u32 = 1 << 2;
    /// `TABLE_PART`
    pub(crate) const TABLE_PART: u32 = 1 << 3;
    /// `table` (with `TABLE_PART` this is `TABLE_FAMILY`)
    pub(crate) const TABLE: u32 = 1 << 4;
    /// `BREAKOUT`
    pub(crate) const BREAKOUT: u32 = 1 << 5;
    /// `INTEGRATION`
    pub(crate) const INTEGRATION: u32 = 1 << 6;
    /// `BLOCK_HTML`
    pub(crate) const BLOCK_HTML: u32 = 1 << 7;
    /// `FORBIDDEN_CONTENT`
    pub(crate) const FORBIDDEN_CONTENT: u32 = 1 << 8;
    /// `h1`–`h6`
    pub(crate) const HEADING: u32 = 1 << 9;
    /// `TEXT` (any raw text kind)
    pub(crate) const TEXT: u32 = 1 << 10;
    /// `template` (stops the table-family search of M-09)
    pub(crate) const TEMPLATE: u32 = 1 << 11;
    /// `SPECIAL` (M-06 items 2 and 4; HTML elements only)
    pub(crate) const SPECIAL: u32 = 1 << 12;
    /// `FORMATTING` (M-06 items 2 and 4)
    pub(crate) const FORMATTING: u32 = 1 << 13;

    /// `TABLE_FAMILY` = `table` and `TABLE_PART`.
    pub(crate) const TABLE_FAMILY: u32 = TABLE | TABLE_PART;
}

macro_rules! known_names {
    ($($variant:ident = $text:literal),* $(,)?) => {
        /// Names that the rules use one by one. Their ids are fixed: the
        /// table interns them first, in this order.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum Known { $($variant),* }

        const KNOWN: &[&[u8]] = &[$($text),*];

        impl Known {
            /// The fixed id of this name.
            pub(crate) const fn id(self) -> NameId {
                NameId(self as usize)
            }
        }
    };
}

known_names! {
    A = b"a",
    Body = b"body",
    Button = b"button",
    Caption = b"caption",
    Col = b"col",
    Colgroup = b"colgroup",
    Dd = b"dd",
    Dl = b"dl",
    Dt = b"dt",
    Form = b"form",
    Frameset = b"frameset",
    H1 = b"h1",
    H2 = b"h2",
    H3 = b"h3",
    H4 = b"h4",
    H5 = b"h5",
    H6 = b"h6",
    Head = b"head",
    Hr = b"hr",
    Html = b"html",
    Li = b"li",
    Math = b"math",
    Menu = b"menu",
    Nobr = b"nobr",
    Noscript = b"noscript",
    Ol = b"ol",
    Optgroup = b"optgroup",
    Option = b"option",
    P = b"p",
    Plaintext = b"plaintext",
    Rb = b"rb",
    Rp = b"rp",
    Rt = b"rt",
    Rtc = b"rtc",
    Ruby = b"ruby",
    Script = b"script",
    Select = b"select",
    Span = b"span",
    Svg = b"svg",
    Table = b"table",
    Tbody = b"tbody",
    Td = b"td",
    Template = b"template",
    Tfoot = b"tfoot",
    Th = b"th",
    Thead = b"thead",
    Title = b"title",
    Tr = b"tr",
    Ul = b"ul",
}

const HEADINGS: &[&[u8]] = &[b"h1", b"h2", b"h3", b"h4", b"h5", b"h6"];

/// `VOID` of `markup.md` section 2.1. Also used by the M-11 check of
/// `noscript` content, which keeps no name table.
pub(crate) const VOID_NAMES: &[&[u8]] = &[
    b"area",
    b"base",
    b"basefont",
    b"bgsound",
    b"br",
    b"col",
    b"embed",
    b"frame",
    b"hr",
    b"image",
    b"img",
    b"input",
    b"keygen",
    b"link",
    b"meta",
    b"param",
    b"source",
    b"track",
    b"wbr",
];

/// `TABLE_PART` of `markup.md` section 2.1. Also used by the M-11 check of
/// `noscript` content (item 6), which keeps no name table.
pub(crate) const TABLE_PART_NAMES: &[&[u8]] = &[
    b"caption",
    b"colgroup",
    b"tbody",
    b"td",
    b"tfoot",
    b"th",
    b"thead",
    b"tr",
];

/// The name sets of `markup.md` section 2.1, without the `h1`–`h6` ranges
/// (added from [`HEADINGS`]).
const SETS: &[(u32, &[&[u8]], bool)] = &[
    (flag::VOID, VOID_NAMES, false),
    (
        flag::TEXT,
        &[
            b"title",
            b"textarea",
            b"iframe",
            b"noembed",
            b"noframes",
            b"noscript",
            b"style",
            b"xmp",
            b"script",
            b"plaintext",
        ],
        false,
    ),
    (
        flag::PCLOSE,
        &[
            b"address",
            b"article",
            b"aside",
            b"blockquote",
            b"center",
            b"dd",
            b"details",
            b"dialog",
            b"dir",
            b"div",
            b"dl",
            b"dt",
            b"fieldset",
            b"figcaption",
            b"figure",
            b"footer",
            b"form",
            b"header",
            b"hgroup",
            b"hr",
            b"li",
            b"listing",
            b"main",
            b"menu",
            b"nav",
            b"ol",
            b"p",
            b"plaintext",
            b"pre",
            b"search",
            b"section",
            b"summary",
            b"table",
            b"ul",
            b"xmp",
        ],
        true,
    ),
    (
        flag::GUARDED,
        &[
            b"a",
            b"applet",
            b"button",
            b"caption",
            b"colgroup",
            b"marquee",
            b"math",
            b"nobr",
            b"object",
            b"select",
            b"svg",
            b"table",
            b"tbody",
            b"td",
            b"template",
            b"tfoot",
            b"th",
            b"thead",
            b"tr",
        ],
        false,
    ),
    (flag::TABLE_PART, TABLE_PART_NAMES, false),
    (flag::TABLE, &[b"table"], false),
    (flag::TEMPLATE, &[b"template"], false),
    (
        flag::BREAKOUT,
        &[
            b"b",
            b"big",
            b"blockquote",
            b"body",
            b"br",
            b"center",
            b"code",
            b"dd",
            b"div",
            b"dl",
            b"dt",
            b"em",
            b"embed",
            b"font",
            b"head",
            b"hr",
            b"i",
            b"img",
            b"li",
            b"listing",
            b"menu",
            b"meta",
            b"nobr",
            b"ol",
            b"p",
            b"pre",
            b"ruby",
            b"s",
            b"small",
            b"span",
            b"strike",
            b"strong",
            b"sub",
            b"sup",
            b"table",
            b"tt",
            b"u",
            b"ul",
            b"var",
        ],
        true,
    ),
    (
        flag::INTEGRATION,
        &[
            b"annotation-xml",
            b"desc",
            b"foreignobject",
            b"mi",
            b"mn",
            b"mo",
            b"ms",
            b"mtext",
            b"title",
        ],
        false,
    ),
    (
        flag::BLOCK_HTML,
        &[
            b"article", b"aside", b"div", b"footer", b"header", b"main", b"nav", b"p", b"section",
            b"span", b"title",
        ],
        true,
    ),
    (
        flag::FORBIDDEN_CONTENT,
        &[
            b"body",
            b"frame",
            b"frameset",
            b"head",
            b"html",
            b"noembed",
            b"noframes",
            b"optgroup",
            b"option",
            b"plaintext",
            b"select",
            b"textarea",
            b"title",
            b"xmp",
        ],
        false,
    ),
    (flag::HEADING, &[], true),
    (
        flag::FORMATTING,
        &[
            b"a", b"b", b"big", b"code", b"em", b"font", b"i", b"nobr", b"s", b"small", b"strike",
            b"strong", b"tt", b"u",
        ],
        false,
    ),
    (
        flag::SPECIAL,
        &[
            b"address",
            b"applet",
            b"area",
            b"article",
            b"aside",
            b"base",
            b"basefont",
            b"bgsound",
            b"blockquote",
            b"body",
            b"br",
            b"button",
            b"caption",
            b"center",
            b"col",
            b"colgroup",
            b"dd",
            b"details",
            b"dir",
            b"div",
            b"dl",
            b"dt",
            b"embed",
            b"fieldset",
            b"figcaption",
            b"figure",
            b"footer",
            b"form",
            b"frame",
            b"frameset",
            b"head",
            b"header",
            b"hgroup",
            b"hr",
            b"html",
            b"iframe",
            b"img",
            b"input",
            b"keygen",
            b"li",
            b"link",
            b"listing",
            b"main",
            b"marquee",
            b"menu",
            b"meta",
            b"nav",
            b"noembed",
            b"noframes",
            b"noscript",
            b"object",
            b"ol",
            b"p",
            b"param",
            b"plaintext",
            b"pre",
            b"script",
            b"search",
            b"section",
            b"select",
            b"source",
            b"style",
            b"summary",
            b"table",
            b"tbody",
            b"td",
            b"template",
            b"textarea",
            b"tfoot",
            b"th",
            b"thead",
            b"title",
            b"tr",
            b"track",
            b"ul",
            b"wbr",
            b"xmp",
        ],
        true,
    ),
];

/// The size of the chunks of a normalized name that [`NameTable`] hashes.
const HASH_CHUNK: usize = 64;

/// U+FFFD in UTF-8: the normalized form of a U+0000 byte in a tag name.
const REPLACEMENT: [u8; 3] = [0xEF, 0xBF, 0xBD];

/// The normalized bytes of a raw tag name: ASCII upper case to lower case,
/// U+0000 to U+FFFD. The table reads names through this view, so it needs
/// no buffer for a normalized copy.
fn normalized(raw: &[u8]) -> impl Iterator<Item = u8> + '_ {
    raw.iter().flat_map(|&byte| {
        let (bytes, len) = if byte == 0 {
            (REPLACEMENT, REPLACEMENT.len())
        } else {
            ([byte.to_ascii_lowercase(), 0, 0], 1)
        };
        bytes.into_iter().take(len)
    })
}

/// The bytes of a name in the table.
#[derive(Debug)]
enum Key {
    /// A name of the rules ([`Known`] and the sets). It is normalized.
    Static(&'static [u8]),
    /// Any other name, as raw bytes. The table keeps it only while an
    /// element with this name is open (and for the pending name).
    Open(Box<[u8]>),
    /// A free entry: a new name can take its id.
    Free,
}

impl Key {
    /// True when `raw` is this name after normalization.
    fn matches(&self, raw: &[u8]) -> bool {
        match self {
            Self::Static(name) => normalized(raw).eq(name.iter().copied()),
            Self::Open(name) => normalized(raw).eq(normalized(name)),
            Self::Free => false,
        }
    }
}

#[derive(Debug)]
struct Entry {
    flags: u32,
    /// Index of the nearest (topmost) element with this name on the token
    /// stack, if one is open.
    top: Option<usize>,
    key: Key,
    /// The hash of the normalized name.
    hash: u64,
    /// The next entry with the same hash.
    next: Option<NameId>,
}

impl Entry {
    const FREE: Self = Self {
        flags: 0,
        top: None,
        key: Key::Free,
        hash: 0,
        next: None,
    };
}

/// Tag names with their flags and their open-element pointer.
///
/// Memory (WP-06 S2, security review finding S1-3): the table holds the
/// names of the rules, with static bytes, and another name only while an
/// element with this name is open on the token stack, plus at most one
/// pending name (a start tag that pushed nothing). So it holds at most
/// `R + S + 1` entries, where R is the number of rule names and S the
/// largest size of the token stack (D for the analysis, M-26). The bytes
/// that it copies are at most the input length, plus the 11 bytes of the
/// `</noscript>` that the pass of M-11 item 8 adds after C: each open name
/// and the pending name come from different start tags. No other memory
/// grows with a name: the hash reads the normalized bytes from the raw
/// bytes. The ids of names that are no longer open are given again to new
/// names.
#[derive(Debug)]
pub(crate) struct NameTable {
    /// The hash of a normalized name and the newest entry with this hash.
    heads: HashMap<u64, NameId>,
    entries: Vec<Entry>,
    /// Ids of free entries. Each is at least `fixed`.
    free: Vec<NameId>,
    /// The number of rule names. Their ids are below it; they are never
    /// freed.
    fixed: usize,
    /// The last new name that no element has used yet. It is freed when
    /// the next new name comes and no element with it is open.
    pending: Option<NameId>,
    state: RandomState,
    /// The copied bytes of the open names (tests only).
    #[cfg(test)]
    open_bytes: usize,
}

impl NameTable {
    /// A table with all names of the rules interned. [`Known`] names have
    /// their fixed ids.
    pub(crate) fn new() -> Self {
        let mut table = Self {
            heads: HashMap::new(),
            entries: Vec::new(),
            free: Vec::new(),
            fixed: 0,
            pending: None,
            state: RandomState::new(),
            #[cfg(test)]
            open_bytes: 0,
        };
        for name in KNOWN {
            table.insert_static(name);
        }
        for (bits, names, with_headings) in SETS {
            let extra: &[&[u8]] = if *with_headings { HEADINGS } else { &[] };
            for name in names.iter().chain(extra) {
                let id = table.insert_static(name);
                if let Some(entry) = table.entries.get_mut(id.0) {
                    entry.flags |= bits;
                }
            }
        }
        table.fixed = table.entries.len();
        table
    }

    /// The hash of the normalized name. The bytes go to the hasher in
    /// chunks of [`HASH_CHUNK`] bytes of the normalized name, so two raw
    /// names with the same normalized bytes give the same calls.
    fn hash(&self, raw: &[u8]) -> u64 {
        let mut hasher = self.state.build_hasher();
        let mut chunk = [0_u8; HASH_CHUNK];
        let mut len = 0_usize;
        for byte in normalized(raw) {
            if let Some(slot) = chunk.get_mut(len) {
                *slot = byte;
                len = len.saturating_add(1);
            }
            if len == HASH_CHUNK {
                hasher.write(&chunk);
                len = 0;
            }
        }
        if let Some(rest) = chunk.get(..len).filter(|rest| !rest.is_empty()) {
            hasher.write(rest);
        }
        hasher.finish()
    }

    /// The entry of a raw name with this hash. A chain holds each entry at
    /// most once, so the walk ends after at most `entries.len()` steps.
    fn find(&self, raw: &[u8], hash: u64) -> Option<NameId> {
        let mut next = self.heads.get(&hash).copied();
        for _ in 0..self.entries.len() {
            let id = next?;
            let entry = self.entries.get(id.0)?;
            if entry.key.matches(raw) {
                return Some(id);
            }
            next = entry.next;
        }
        None
    }

    /// Adds a new entry, in a free slot if there is one.
    fn insert(&mut self, key: Key, hash: u64) -> NameId {
        #[cfg(test)]
        if let Key::Open(name) = &key {
            self.open_bytes = self.open_bytes.saturating_add(name.len());
        }
        let entry = Entry {
            flags: 0,
            top: None,
            key,
            hash,
            next: self.heads.get(&hash).copied(),
        };
        let id = match self.free.pop() {
            Some(id) if id.0 < self.entries.len() => {
                if let Some(slot) = self.entries.get_mut(id.0) {
                    *slot = entry;
                }
                id
            }
            _ => {
                let id = NameId(self.entries.len());
                self.entries.push(entry);
                id
            }
        };
        self.heads.insert(hash, id);
        #[cfg(test)]
        stats::record(self.entries.len(), self.open_bytes);
        id
    }

    fn insert_static(&mut self, name: &'static [u8]) -> NameId {
        let hash = self.hash(name);
        match self.find(name, hash) {
            Some(id) => id,
            None => self.insert(Key::Static(name), hash),
        }
    }

    /// Frees the entry of a name that is not a rule name and has no open
    /// element. Other entries do not change.
    fn release(&mut self, id: NameId) {
        if id.0 < self.fixed {
            return;
        }
        let Some(entry) = self.entries.get(id.0) else {
            return;
        };
        if entry.top.is_some() || matches!(entry.key, Key::Free) {
            return;
        }
        let (hash, next) = (entry.hash, entry.next);
        if self.heads.get(&hash) == Some(&id) {
            match next {
                Some(next) => self.heads.insert(hash, next),
                None => self.heads.remove(&hash),
            };
        } else {
            let mut cursor = self.heads.get(&hash).copied();
            for _ in 0..self.entries.len() {
                let Some(entry) = cursor.and_then(|c| self.entries.get_mut(c.0)) else {
                    break;
                };
                if entry.next == Some(id) {
                    entry.next = next;
                    break;
                }
                cursor = entry.next;
            }
        }
        if let Some(entry) = self.entries.get_mut(id.0) {
            #[cfg(test)]
            if let Key::Open(name) = &entry.key {
                self.open_bytes = self.open_bytes.saturating_sub(name.len());
            }
            *entry = Entry::FREE;
        }
        self.free.push(id);
        if self.pending == Some(id) {
            self.pending = None;
        }
    }

    /// The id of a raw tag name; a new id when the name is new.
    pub(crate) fn intern(&mut self, raw: &[u8]) -> NameId {
        let hash = self.hash(raw);
        if let Some(id) = self.find(raw, hash) {
            return id;
        }
        if let Some(pending) = self.pending.take() {
            self.release(pending);
        }
        let id = self.insert(Key::Open(raw.into()), hash);
        self.pending = Some(id);
        id
    }

    /// The id of a raw tag name, if the name is in the table. A name that
    /// is not in the table has no open element and no flags.
    pub(crate) fn lookup(&self, raw: &[u8]) -> Option<NameId> {
        self.find(raw, self.hash(raw))
    }

    /// The flags of a name (0 for a name outside every set).
    pub(crate) fn flags(&self, id: NameId) -> u32 {
        self.entries.get(id.0).map_or(0, |e| e.flags)
    }

    /// True when the name is in one of the sets of `mask`.
    pub(crate) fn has(&self, id: NameId, mask: u32) -> bool {
        self.flags(id) & mask != 0
    }

    /// Index of the nearest open element with this name.
    pub(crate) fn top(&self, id: NameId) -> Option<usize> {
        self.entries.get(id.0).and_then(|e| e.top)
    }

    /// Sets the nearest open element of a name; returns the previous one.
    /// With `None` (no element with this name is open any more) the entry
    /// of a name that is not a rule name is freed: do not use its id again.
    pub(crate) fn replace_top(&mut self, id: NameId, top: Option<usize>) -> Option<usize> {
        let previous = self
            .entries
            .get_mut(id.0)
            .and_then(|e| std::mem::replace(&mut e.top, top));
        if top.is_none() {
            self.release(id);
        }
        previous
    }
}

/// The largest size of the name tables of this thread (tests only): the
/// number of entries and the copied bytes of the open names.
#[cfg(test)]
pub(crate) mod stats {
    use std::cell::Cell;

    thread_local! {
        static PEAK: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    }

    /// Sets the largest size to zero.
    pub(crate) fn reset() {
        PEAK.with(|peak| peak.set((0, 0)));
    }

    /// The largest size since [`reset`].
    pub(crate) fn peak() -> (usize, usize) {
        PEAK.with(Cell::get)
    }

    pub(super) fn record(entries: usize, bytes: usize) {
        PEAK.with(|peak| {
            let (e, b) = peak.get();
            peak.set((e.max(entries), b.max(bytes)));
        });
    }
}

/// The raw text kind that a start tag in HTML context switches to (M-04).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RawKind {
    /// RCDATA: `title`, `textarea`
    RcData,
    /// RAWTEXT: `iframe`, `noembed`, `noframes`, `noscript`, `style`, `xmp`
    RawText,
    /// Script data: `script`
    Script,
    /// PLAINTEXT: `plaintext`
    PlainText,
}

const RAW_KINDS: &[(&[u8], RawKind)] = &[
    (b"title", RawKind::RcData),
    (b"textarea", RawKind::RcData),
    (b"iframe", RawKind::RawText),
    (b"noembed", RawKind::RawText),
    (b"noframes", RawKind::RawText),
    (b"noscript", RawKind::RawText),
    (b"style", RawKind::RawText),
    (b"xmp", RawKind::RawText),
    (b"script", RawKind::Script),
    (b"plaintext", RawKind::PlainText),
];

/// The M-04 state change of a raw start tag name, with the lower-case name
/// that ends the raw text.
pub(crate) fn raw_kind(raw_name: &[u8]) -> Option<(RawKind, &'static [u8])> {
    RAW_KINDS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(raw_name))
        .map(|(name, kind)| (*kind, *name))
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

    #[test]
    fn known_ids_match_their_names() {
        let table = NameTable::new();
        for (index, name) in KNOWN.iter().enumerate() {
            assert_eq!(table.lookup(name), Some(NameId(index)));
        }
        assert_eq!(table.lookup(b"SVG"), Some(Known::Svg.id()));
        assert_eq!(table.lookup(b"title"), Some(Known::Title.id()));
    }

    #[test]
    fn normalization_lowercases_ascii_and_maps_nul() {
        let mut table = NameTable::new();
        let a = table.intern(b"Foo\0");
        let b = table.intern("foo\u{FFFD}".as_bytes());
        assert_eq!(a, b);
        assert_eq!(table.lookup(b"FOO\0"), Some(a));
        assert_eq!(table.lookup(b"foo"), None);
        let c = table.intern("FO\u{00D6}".as_bytes());
        assert_eq!(table.lookup("fo\u{00D6}".as_bytes()), Some(c));
        assert_eq!(
            table.lookup("fo\u{00F6}".as_bytes()),
            None,
            "only ASCII is lowercased"
        );
        assert_eq!(table.lookup(b"never-seen"), None);
    }

    #[test]
    fn sets_have_their_flags() {
        let table = NameTable::new();
        let has = |name: &[u8], mask| {
            let id = table.lookup(name).unwrap();
            table.has(id, mask)
        };
        assert!(has(b"br", flag::VOID));
        assert!(has(b"h3", flag::PCLOSE));
        assert!(has(b"h6", flag::BREAKOUT));
        assert!(has(b"h1", flag::BLOCK_HTML));
        assert!(has(b"h4", flag::HEADING));
        assert!(!has(b"div", flag::HEADING));
        assert!(has(b"foreignobject", flag::INTEGRATION));
        assert!(has(b"td", flag::TABLE_FAMILY));
        assert!(has(b"table", flag::TABLE_FAMILY));
        assert!(!has(b"table", flag::TABLE_PART));
        assert!(has(b"template", flag::TEMPLATE));
        assert!(has(b"xmp", flag::TEXT));
        assert!(has(b"option", flag::FORBIDDEN_CONTENT));
        assert!(!has(b"svg", flag::FORBIDDEN_CONTENT));
        assert!(!has(b"math", flag::FORBIDDEN_CONTENT));
        assert!(has(b"title", flag::FORBIDDEN_CONTENT));
        // H-78: the patch-safety elements are not forbidden in the content.
        for name in [
            &b"base"[..],
            b"basefont",
            b"bgsound",
            b"embed",
            b"iframe",
            b"link",
            b"meta",
            b"object",
            b"script",
            b"style",
        ] {
            assert!(!has(name, flag::FORBIDDEN_CONTENT), "{name:?}");
        }
        // H-94, H-95: `template` and `noscript` are allowed in the content.
        assert!(!has(b"template", flag::FORBIDDEN_CONTENT));
        assert!(!has(b"noscript", flag::FORBIDDEN_CONTENT));
        assert!(has(b"marquee", flag::GUARDED));
        assert!(!has(b"span", flag::GUARDED));
    }

    #[test]
    fn raw_kinds() {
        assert_eq!(raw_kind(b"TITLE"), Some((RawKind::RcData, &b"title"[..])));
        assert_eq!(
            raw_kind(b"noscript"),
            Some((RawKind::RawText, &b"noscript"[..]))
        );
        assert_eq!(raw_kind(b"Script"), Some((RawKind::Script, &b"script"[..])));
        assert_eq!(
            raw_kind(b"plaintext"),
            Some((RawKind::PlainText, &b"plaintext"[..]))
        );
        assert_eq!(raw_kind(b"div"), None);
    }

    #[test]
    fn top_pointer() {
        let mut table = NameTable::new();
        let id = table.intern(b"x-y");
        assert_eq!(table.top(id), None);
        assert_eq!(table.replace_top(id, Some(4)), None);
        assert_eq!(table.top(id), Some(4));
        assert_eq!(table.replace_top(id, None), Some(4));
    }

    #[test]
    fn a_name_stays_only_while_an_element_with_it_is_open() {
        let mut table = NameTable::new();
        let fixed = table.entries.len();
        // A new name that no element uses is freed at the next new name.
        let x = table.intern(b"x-a");
        assert_eq!(table.intern(b"X-A"), x);
        let y = table.intern(b"x-b");
        assert_eq!(table.lookup(b"x-a"), None);
        // An open name stays; a nested element with the same name keeps it.
        assert_eq!(table.replace_top(y, Some(0)), None);
        assert_eq!(table.replace_top(y, Some(1)), Some(0));
        table.intern(b"x-c");
        table.intern(b"x-d");
        assert_eq!(table.lookup(b"x-b"), Some(y));
        assert_eq!(table.replace_top(y, Some(0)), Some(1));
        assert_eq!(table.lookup(b"x-b"), Some(y));
        assert_eq!(table.replace_top(y, None), Some(0));
        assert_eq!(table.lookup(b"x-b"), None);
        assert_eq!(table.top(y), None);
        assert_eq!(table.flags(y), 0);
        // Rule names stay with their flags when no element is open.
        let span = Known::Span.id();
        table.replace_top(span, Some(3));
        assert_eq!(table.replace_top(span, None), Some(3));
        assert_eq!(table.lookup(b"SPAN"), Some(span));
        assert!(table.has(span, flag::BLOCK_HTML));
        let br = table.lookup(b"br").unwrap();
        table.replace_top(br, None);
        assert!(table.has(br, flag::VOID));
        // Free entries are used again: the table does not grow.
        for index in 0..10_000 {
            let id = table.intern(format!("n{index}").as_bytes());
            table.replace_top(id, Some(0));
            if index % 2 == 0 {
                table.replace_top(id, None);
            } else {
                let other = table.intern(format!("m{index}").as_bytes());
                assert_ne!(other, id);
                table.replace_top(id, None);
            }
        }
        assert!(table.entries.len() <= fixed + 3, "{}", table.entries.len());
        assert_eq!(table.lookup(b"n9999"), None);
    }

    /// The hash follows the normalization (also over the chunk size), and
    /// distinct names spread over many hashes (the chains stay short, so the
    /// work stays linear).
    #[test]
    fn hash_follows_normalization() {
        let table = NameTable::new();
        let long = "Ab\0".repeat(HASH_CHUNK);
        let normal = "ab\u{FFFD}".repeat(HASH_CHUNK);
        assert_eq!(table.hash(long.as_bytes()), table.hash(normal.as_bytes()));
        assert_eq!(table.hash(b"SVG"), table.hash(b"svg"));
        assert_ne!(
            table.hash(&normal.as_bytes()[1..]),
            table.hash(normal.as_bytes())
        );
        let hashes: std::collections::HashSet<u64> = (0..1000)
            .map(|index| table.hash(format!("n{index}").as_bytes()))
            .collect();
        assert!(hashes.len() > 990, "{}", hashes.len());
    }

    /// Names with the same hash share one chain; freeing the head, a middle
    /// or the last entry keeps the other names.
    #[test]
    fn a_hash_chain_keeps_its_other_names() {
        for freed in 0..3 {
            let mut table = NameTable::new();
            let hash = 7;
            let names: [&[u8]; 3] = [b"c-0", b"c-1", b"c-2"];
            let ids: Vec<NameId> = names
                .iter()
                .map(|name| {
                    let id = table.insert(Key::Open((*name).into()), hash);
                    table.replace_top(id, Some(0));
                    id
                })
                .collect();
            for (name, id) in names.iter().zip(&ids) {
                assert_eq!(table.find(name, hash), Some(*id));
            }
            table.replace_top(ids[freed], None);
            for (index, (name, id)) in names.iter().zip(&ids).enumerate() {
                let expected = (index != freed).then_some(*id);
                assert_eq!(table.find(name, hash), expected, "freed {freed}");
            }
            // The free entry is used by the next name, in the same chain.
            let new = table.insert(Key::Open((*b"c-3").into()), hash);
            assert_eq!(new, ids[freed]);
            assert_eq!(table.find(b"c-3", hash), Some(new));
            for (index, (name, id)) in names.iter().zip(&ids).enumerate() {
                if index != freed {
                    assert_eq!(table.find(name, hash), Some(*id));
                }
            }
        }
    }

    /// WP-06 S2 (finding S1-3): a 5 MiB page of distinct tag names keeps
    /// the name table at the rule names plus the open names. Before the
    /// change the table held one entry and one copy per distinct name
    /// (about 635 000 entries and 13 times the input in heap for the first
    /// page below).
    #[test]
    fn rqp_names_memory_bound() {
        use crate::limits::{MAX_MARKUP_INPUT_BYTES, MAX_OPEN_ELEMENTS, TemplateLimits};
        let fixed = NameTable::new().entries.len();
        let name = |mut index: usize| {
            let mut out = String::from("q");
            loop {
                out.push(char::from(b'a' + u8::try_from(index % 26).unwrap()));
                index /= 26;
                if index == 0 {
                    return out;
                }
            }
        };
        let fill = |size: usize, prefix: &str, piece: &dyn Fn(usize) -> String| {
            let mut out = String::from(prefix);
            for index in 0.. {
                let next = piece(index);
                if out.len() + next.len() > size {
                    break;
                }
                out.push_str(&next);
            }
            out.into_bytes()
        };
        // 5 MiB of distinct self-closing names in a foreign region (never
        // pushed); 1 MiB of distinct names opened and closed; one name of
        // 1 MiB U+0000 bytes (3 MiB when normalized); 5 MiB of 256 open
        // elements with long distinct names. (The bound does not depend on
        // the size; the smaller inputs keep the test fast.)
        let mib = MAX_MARKUP_INPUT_BYTES / 5;
        let foreign = fill(MAX_MARKUP_INPUT_BYTES, "<svg>", &|i| {
            format!("<{}/>", name(i))
        });
        let html = fill(mib, "", &|i| format!("<{0}></{0}>", name(i)));
        let mut nul = b"<q".to_vec();
        nul.resize(mib - 1, 0);
        nul.push(b'>');
        let long_name = MAX_MARKUP_INPUT_BYTES / MAX_OPEN_ELEMENTS - 8;
        let long: Vec<u8> = (0..MAX_OPEN_ELEMENTS)
            .flat_map(|i| format!("<{}{i:04}>", "q".repeat(long_name)).into_bytes())
            .collect();
        let limits = TemplateLimits::default();
        for (input, open) in [(&foreign, 1), (&html, 1), (&nul, 1), (&long, 256)] {
            assert!(input.len() <= MAX_MARKUP_INPUT_BYTES);
            stats::reset();
            assert!(crate::rqp::split_rqp(input, &limits).is_ok());
            let (entries, bytes) = stats::peak();
            assert!(entries <= fixed + open + 1, "{entries} entries");
            assert!(bytes <= input.len(), "{bytes} bytes");
            stats::reset();
            assert!(crate::rqp::scan::scan(input, &limits).all(|item| item.is_ok()));
            let (entries, bytes) = stats::peak();
            assert!(entries <= fixed + open + 1, "{entries} entries");
            assert!(bytes <= input.len(), "{bytes} bytes");
        }
    }
}
