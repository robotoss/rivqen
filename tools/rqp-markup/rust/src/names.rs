// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Tag names: interning, the name sets of `markup.md` section 2.1, and the
//! per-name "nearest open element" pointer of the token stack.
//!
//! Every tag name is normalized as the WHATWG tokenizer does it: ASCII upper
//! case becomes lower case and U+0000 becomes U+FFFD. Two raw names are the
//! same name when their normalized bytes are equal.

use std::collections::HashMap;

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
    Button = b"button",
    Caption = b"caption",
    Col = b"col",
    Colgroup = b"colgroup",
    Dd = b"dd",
    Dl = b"dl",
    Dt = b"dt",
    Frameset = b"frameset",
    H1 = b"h1",
    H2 = b"h2",
    H3 = b"h3",
    H4 = b"h4",
    H5 = b"h5",
    H6 = b"h6",
    Hr = b"hr",
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

/// The name sets of `markup.md` section 2.1, without the `h1`–`h6` ranges
/// (added from [`HEADINGS`]).
const SETS: &[(u32, &[&[u8]], bool)] = &[
    (
        flag::VOID,
        &[
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
        ],
        false,
    ),
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
    (
        flag::TABLE_PART,
        &[
            b"caption",
            b"colgroup",
            b"tbody",
            b"td",
            b"tfoot",
            b"th",
            b"thead",
            b"tr",
        ],
        false,
    ),
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
            b"base",
            b"basefont",
            b"bgsound",
            b"body",
            b"embed",
            b"frame",
            b"frameset",
            b"head",
            b"html",
            b"iframe",
            b"link",
            b"math",
            b"meta",
            b"noembed",
            b"noframes",
            b"noscript",
            b"object",
            b"optgroup",
            b"option",
            b"plaintext",
            b"script",
            b"select",
            b"style",
            b"svg",
            b"template",
            b"textarea",
            b"title",
            b"xmp",
        ],
        false,
    ),
    (flag::HEADING, &[], true),
];

#[derive(Clone, Copy, Debug)]
struct Entry {
    flags: u32,
    /// Index of the nearest (topmost) element with this name on the token
    /// stack, if one is open.
    top: Option<usize>,
}

/// Interned tag names with their flags and their open-element pointer.
#[derive(Debug)]
pub(crate) struct NameTable {
    map: HashMap<Vec<u8>, NameId>,
    entries: Vec<Entry>,
    scratch: Vec<u8>,
}

impl NameTable {
    /// A table with all names of the rules interned. [`Known`] names have
    /// their fixed ids.
    pub(crate) fn new() -> Self {
        let mut table = Self {
            map: HashMap::new(),
            entries: Vec::new(),
            scratch: Vec::new(),
        };
        for name in KNOWN {
            table.intern(name);
        }
        for (bits, names, with_headings) in SETS {
            let extra: &[&[u8]] = if *with_headings { HEADINGS } else { &[] };
            for name in names.iter().chain(extra) {
                let id = table.intern(name);
                if let Some(entry) = table.entries.get_mut(id.0) {
                    entry.flags |= bits;
                }
            }
        }
        table
    }

    fn normalize(&mut self, raw: &[u8]) {
        self.scratch.clear();
        for &byte in raw {
            if byte == 0 {
                self.scratch.extend_from_slice("\u{FFFD}".as_bytes());
            } else {
                self.scratch.push(byte.to_ascii_lowercase());
            }
        }
    }

    /// The id of a raw tag name; a new id when the name is new.
    pub(crate) fn intern(&mut self, raw: &[u8]) -> NameId {
        self.normalize(raw);
        if let Some(id) = self.map.get(self.scratch.as_slice()) {
            return *id;
        }
        let id = NameId(self.entries.len());
        self.entries.push(Entry {
            flags: 0,
            top: None,
        });
        self.map.insert(self.scratch.clone(), id);
        id
    }

    /// The id of a raw tag name, if the name was interned before. A name
    /// that is not interned has no open element and no flags.
    pub(crate) fn lookup(&mut self, raw: &[u8]) -> Option<NameId> {
        self.normalize(raw);
        self.map.get(self.scratch.as_slice()).copied()
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
    pub(crate) fn replace_top(&mut self, id: NameId, top: Option<usize>) -> Option<usize> {
        self.entries
            .get_mut(id.0)
            .and_then(|e| std::mem::replace(&mut e.top, top))
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
        let mut table = NameTable::new();
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
        let c = table.intern("FO\u{00D6}".as_bytes());
        let d = table.intern("fo\u{00F6}".as_bytes());
        assert_ne!(c, d, "only ASCII is lowercased");
        assert_eq!(table.lookup(b"never-seen"), None);
    }

    #[test]
    fn sets_have_their_flags() {
        let mut table = NameTable::new();
        let mut has = |name: &[u8], mask| {
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
}
