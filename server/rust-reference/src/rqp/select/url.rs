// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The request URL as the WHATWG URL Standard serializes it (manifest and patch
//! §1.6: `origin` and `path` of the manifest; §2.2: the canonical URL U of the
//! binding, without the fragment).
//!
//! `Request.url` is the configured public origin followed by the request target
//! (CONTRACT.md §2; §1.6 item 5). The path and the query follow the WHATWG path
//! and query states of a special URL: `\` is a separator, `.` and `..` segments
//! (also `%2e` forms) are resolved, and the bytes of the path and the
//! special-query percent-encode sets are percent-encoded.
//!
//! The host is a subset of the WHATWG host parser, and every input outside the
//! subset fails closed (`None`; the page is sent without RQP, as DL-089 item 6):
//! user information (H-68), an IPv6 address, a `%` or a non-ASCII byte in the
//! host (they need IDNA or a host decoder), a host byte outside `[a-z0-9._-]`
//! after lower-casing (the pattern of `manifest.schema.json`), an IPv4 address
//! in another form than four decimal numbers, a port larger than 65535 or `0`
//! (the schema pattern), and a scheme other than `http` and `https`.

/// The longest `origin` of a manifest (manifest and patch §1.6, §6).
pub const MAX_ORIGIN_CHARS: usize = 512;

/// The longest `path` of a manifest (manifest and patch §1.6, §6).
pub const MAX_PATH_CHARS: usize = 4096;

/// The serialized parts of the request URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The ASCII serialization of the origin.
    pub origin: String,
    /// The serialized path (starts with `/`).
    pub path: String,
    /// The serialized query without `?`, when the URL has one (also empty).
    pub query: Option<String>,
}

impl Target {
    /// U: the URL serialization without the fragment (§2.2).
    #[must_use]
    pub fn canonical(&self) -> String {
        match &self.query {
            Some(q) => format!("{}{}?{q}", self.origin, self.path),
            None => format!("{}{}", self.origin, self.path),
        }
    }

    /// The origin and the path fit the limits of §1.6 (DL-089 item 6).
    #[must_use]
    pub fn within_limits(&self) -> bool {
        self.origin.len() <= MAX_ORIGIN_CHARS && self.path.len() <= MAX_PATH_CHARS
    }
}

/// The path percent-encode set (WHATWG URL §1.3) for one byte of UTF-8.
fn in_path_set(b: u8) -> bool {
    !(0x20..=0x7E).contains(&b)
        || matches!(
            b,
            b' ' | b'"' | b'#' | b'<' | b'>' | b'?' | b'^' | b'`' | b'{' | b'}'
        )
}

/// The special-query percent-encode set (WHATWG URL §1.3).
fn in_special_query_set(b: u8) -> bool {
    !(0x20..=0x7E).contains(&b) || matches!(b, b' ' | b'"' | b'#' | b'<' | b'>' | b'\'')
}

fn push_encoded(out: &mut String, b: u8, encode: bool) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    if encode {
        out.push('%');
        for nibble in [b >> 4, b & 0x0F] {
            if let Some(&h) = HEX.get(usize::from(nibble)) {
                out.push(char::from(h));
            }
        }
    } else {
        out.push(char::from(b));
    }
}

fn is_single_dot(segment: &str) -> bool {
    segment == "." || segment.eq_ignore_ascii_case("%2e")
}

fn is_double_dot(segment: &str) -> bool {
    [".."].contains(&segment)
        || [".%2e", "%2e.", "%2e%2e"]
            .iter()
            .any(|s| s.eq_ignore_ascii_case(segment))
}

/// Parse and serialize `url`. `None` when the URL is outside the subset of the
/// module documentation (fail closed).
#[must_use]
pub fn parse(url: &str) -> Option<Target> {
    // Leading and trailing C0 control or space; every ASCII tab or newline.
    let input: Vec<u8> = url
        .trim_matches(|c: char| c <= ' ')
        .bytes()
        .filter(|&b| !matches!(b, b'\t' | b'\n' | b'\r'))
        .collect();
    let colon = input.iter().position(|&b| b == b':')?;
    let scheme = input.get(..colon)?.to_ascii_lowercase();
    let default_port = match scheme.as_slice() {
        b"https" => 443,
        b"http" => 80,
        _ => return None,
    };
    let mut rest = input.get(colon.checked_add(1)?..)?;
    // Special authority slashes: any number of `/` and `\`.
    while let Some((&b, tail)) = rest.split_first() {
        if b != b'/' && b != b'\\' {
            break;
        }
        rest = tail;
    }
    let end = rest
        .iter()
        .position(|&b| matches!(b, b'/' | b'\\' | b'?' | b'#'))
        .unwrap_or(rest.len());
    let (authority, tail) = rest.split_at_checked(end)?;
    let origin = origin(&scheme, authority, default_port)?;
    let (path, query) = path_and_query(tail);
    Some(Target {
        origin,
        path,
        query,
    })
}

/// The serialized origin of `scheme://authority`.
fn origin(scheme: &[u8], authority: &[u8], default_port: u16) -> Option<String> {
    if authority.contains(&b'@') || authority.first() == Some(&b'[') {
        return None;
    }
    let (host, port) = match authority.iter().position(|&b| b == b':') {
        Some(i) => (
            authority.get(..i)?,
            Some(authority.get(i.checked_add(1)?..)?),
        ),
        None => (authority, None),
    };
    let host = host.to_ascii_lowercase();
    if host.is_empty()
        || !host.iter().all(|&b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
        || (ends_in_number(&host) && !is_dotted_quad(&host))
    {
        return None;
    }
    let port = match port {
        None | Some([]) => None,
        Some(digits) => {
            if !digits.iter().all(u8::is_ascii_digit) {
                return None;
            }
            // Leading zeros are allowed; more than 5 significant digits is > 65535.
            let significant: Vec<u8> = digits.iter().copied().skip_while(|&b| b == b'0').collect();
            if significant.len() > 5 {
                return None;
            }
            let value: u32 = std::str::from_utf8(&significant).ok()?.parse().unwrap_or(0);
            let value = u16::try_from(value).ok()?;
            if value == 0 {
                return None;
            }
            (value != default_port).then_some(value)
        }
    };
    let mut out = String::with_capacity(host.len().saturating_add(16));
    out.push_str(std::str::from_utf8(scheme).ok()?);
    out.push_str("://");
    out.push_str(std::str::from_utf8(&host).ok()?);
    if let Some(p) = port {
        out.push(':');
        out.push_str(&p.to_string());
    }
    Some(out)
}

/// The WHATWG "ends in a number" checker: the last label (the one before a
/// final empty label) is decimal digits or `0x` with hex digits.
fn ends_in_number(host: &[u8]) -> bool {
    let mut labels: Vec<&[u8]> = host.split(|&b| b == b'.').collect();
    if labels.len() > 1 && labels.last().is_some_and(|l| l.is_empty()) {
        labels.pop();
    }
    let Some(last) = labels.last() else {
        return false;
    };
    if !last.is_empty() && last.iter().all(u8::is_ascii_digit) {
        return true;
    }
    last.strip_prefix(b"0x")
        .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
}

/// Four decimal numbers 0 to 255 without leading zeros: the form that the IPv4
/// serializer writes, so the host is its own serialization.
fn is_dotted_quad(host: &[u8]) -> bool {
    let parts: Vec<&[u8]> = host.split(|&b| b == b'.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 3
                && p.iter().all(u8::is_ascii_digit)
                && (p.len() == 1 || p.first() != Some(&b'0'))
                && std::str::from_utf8(p)
                    .ok()
                    .and_then(|s| s.parse::<u16>().ok())
                    .is_some_and(|n| n <= 255)
        })
}

/// The path state and the query state of a special URL. The input starts at
/// the first byte after the authority.
fn path_and_query(input: &[u8]) -> (String, Option<String>) {
    let mut rest = input;
    // Path start state: a first `/` or `\` is consumed.
    if let Some((&b, tail)) = rest.split_first() {
        if b == b'/' || b == b'\\' {
            rest = tail;
        }
    }
    let mut segments: Vec<String> = Vec::new();
    let mut buffer = String::new();
    let mut terminator = None;
    for (i, &b) in rest.iter().enumerate() {
        match b {
            b'/' | b'\\' => {
                end_segment(&mut segments, std::mem::take(&mut buffer), true);
            }
            b'?' | b'#' => {
                terminator = Some(i);
                break;
            }
            _ => push_encoded(&mut buffer, b, in_path_set(b)),
        }
    }
    end_segment(&mut segments, buffer, false);
    let mut path = String::new();
    for segment in &segments {
        path.push('/');
        path.push_str(segment);
    }
    let query =
        terminator
            .and_then(|i| rest.get(i..))
            .and_then(|after| match after.split_first() {
                Some((b'?', q)) => {
                    let q = q.split(|&b| b == b'#').next().unwrap_or_default();
                    let mut out = String::with_capacity(q.len());
                    for &b in q {
                        push_encoded(&mut out, b, in_special_query_set(b));
                    }
                    Some(out)
                }
                _ => None,
            });
    (path, query)
}

/// The end of one path segment (WHATWG path state, step 1).
fn end_segment(segments: &mut Vec<String>, buffer: String, slash: bool) {
    if is_double_dot(&buffer) {
        segments.pop();
        if !slash {
            segments.push(String::new());
        }
    } else if is_single_dot(&buffer) {
        if !slash {
            segments.push(String::new());
        }
    } else {
        segments.push(buffer);
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;

    fn u(url: &str) -> Option<String> {
        parse(url).map(|t| t.canonical())
    }

    #[test]
    fn known_answer_of_binding_with_query() {
        // manifest and patch §2.2, kat.json member binding_with_query.
        assert_eq!(
            u("https://SHOP.Example.com:443/catalog?sort=price&page=2#reviews").as_deref(),
            Some("https://shop.example.com/catalog?sort=price&page=2")
        );
    }

    #[test]
    fn origin_and_path() {
        let t = parse("https://shop.example.com/catalog?x=1").unwrap();
        assert_eq!(t.origin, "https://shop.example.com");
        assert_eq!(t.path, "/catalog");
        assert_eq!(t.query.as_deref(), Some("x=1"));
        assert_eq!(
            u("http://shop.example.com:80/").as_deref(),
            Some("http://shop.example.com/")
        );
        assert_eq!(
            u("http://shop.example.com:8080/").as_deref(),
            Some("http://shop.example.com:8080/")
        );
        assert_eq!(
            u("https://shop.example.com:00443/a").as_deref(),
            Some("https://shop.example.com/a")
        );
        assert_eq!(
            u("https://shop.example.com:/a").as_deref(),
            Some("https://shop.example.com/a")
        );
        assert_eq!(
            u("https://shop.example.com").as_deref(),
            Some("https://shop.example.com/")
        );
        assert_eq!(
            u("https://shop.example.com?q").as_deref(),
            Some("https://shop.example.com/?q")
        );
        assert_eq!(
            u("https://127.0.0.1/a").as_deref(),
            Some("https://127.0.0.1/a")
        );
    }

    #[test]
    fn path_dot_segments_and_encoding() {
        assert_eq!(
            u("https://a.example/a/b/../c").as_deref(),
            Some("https://a.example/a/c")
        );
        assert_eq!(
            u("https://a.example/a/./b/.").as_deref(),
            Some("https://a.example/a/b/")
        );
        assert_eq!(
            u("https://a.example/a/%2E%2e/b").as_deref(),
            Some("https://a.example/b")
        );
        assert_eq!(
            u("https://a.example/..").as_deref(),
            Some("https://a.example/")
        );
        assert_eq!(
            u("https://a.example/a\\b").as_deref(),
            Some("https://a.example/a/b")
        );
        assert_eq!(
            u("https://a.example//x").as_deref(),
            Some("https://a.example//x")
        );
        assert_eq!(
            u("https://a.example/a b\"<>^`{}|%41é").as_deref(),
            Some("https://a.example/a%20b%22%3C%3E%5E%60%7B%7D|%41%C3%A9")
        );
        assert_eq!(
            u("https://a.example/a?b c'\"<>?^`").as_deref(),
            Some("https://a.example/a?b%20c%27%22%3C%3E?^`")
        );
        assert_eq!(
            u("https://a.example/a?").as_deref(),
            Some("https://a.example/a?")
        );
        assert_eq!(
            u("https://a.example/a#f?x").as_deref(),
            Some("https://a.example/a")
        );
        assert_eq!(
            u(" https://a.example/a\tb\n ").as_deref(),
            Some("https://a.example/ab")
        );
    }

    #[test]
    fn fails_closed_outside_the_subset() {
        for url in [
            "ftp://a.example/",
            "https://user@a.example/",
            "https://u:p@a.example/",
            "https://[::1]/",
            "https://a%41.example/",
            "https://faß.example/",
            "https://a!.example/",
            "https:///",
            "https://a.example:65536/",
            "https://a.example:0/",
            "https://a.example:12a/",
            "https://1.2.3/",
            "https://0x7f.0.0.1/",
            "https://01.2.3.4/",
            "https://1.2.3.256/",
            "https://example.123/",
            "no-colon",
        ] {
            assert_eq!(parse(url), None, "{url}");
        }
        assert!(parse("https://a.example:65535/").is_some());
        assert!(parse("https://a.example.1x/").is_some());
    }

    #[test]
    fn limits_of_origin_and_path() {
        let host = "a".repeat(MAX_ORIGIN_CHARS - "https://".len());
        assert!(parse(&format!("https://{host}/")).unwrap().within_limits());
        assert!(!parse(&format!("https://{host}a/")).unwrap().within_limits());
        let path = "p".repeat(MAX_PATH_CHARS - 1);
        assert!(
            parse(&format!("https://a.example/{path}"))
                .unwrap()
                .within_limits()
        );
        assert!(
            !parse(&format!("https://a.example/{path}p"))
                .unwrap()
                .within_limits()
        );
    }
}
