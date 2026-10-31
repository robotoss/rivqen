// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! HTTP/1.1 request reader (RFC 9112 §2–§6; RFC 9110 §5.5).
//!
//! The reader is a pure function over the bytes of a [`BufRead`]: the server loop
//! gives it a socket behind a deadline, the tests give it a byte slice. It keeps
//! the request exactly: the method, the request target, and every field line with
//! its name case, its order and its repeats. Only leading and trailing spaces and
//! tabs of a value are removed (RFC 9110 §5.5).
//!
//! Limits (untrusted input): the head (request line, field lines, final empty
//! line) is at most [`MAX_HEAD_BYTES`]; at most [`MAX_FIELD_LINES`] field lines; a
//! body is read only when its `Content-Length` is at most [`MAX_BODY_BYTES`]. The
//! reader looks at each input byte once (linear time) and never allocates more
//! than these limits.

use std::io::{self, BufRead, Read};

use crate::model::Field;

/// The largest head: request line, field lines and the final empty line, with
/// their CRLFs. A request field section of 8 KiB always fits (CONTRACT.md §3).
pub const MAX_HEAD_BYTES: usize = 64 * 1024;

/// The largest number of field lines in a request.
pub const MAX_FIELD_LINES: usize = 256;

/// The largest request body that the server reads (CONTRACT.md §2, `POST /case`).
pub const MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;

/// The HTTP version of the request line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// `HTTP/1.0`.
    Http10,
    /// `HTTP/1.1`.
    Http11,
}

/// A request head that passed every check of this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHead {
    /// The method token as received.
    pub method: String,
    /// The request target in origin-form (`/path?query`), as received.
    pub target: String,
    /// The HTTP version.
    pub version: Version,
    /// Every field line in order; the value without leading and trailing spaces
    /// and tabs.
    pub fields: Vec<Field>,
    /// The body length of `Content-Length` (`0` when absent). A value larger than
    /// `u64` is [`u64::MAX`]; the caller refuses it with `413`.
    pub content_length: u64,
}

impl RequestHead {
    /// The path of the target: the part before the first `?`.
    #[must_use]
    pub fn path(&self) -> &str {
        self.target
            .split_once('?')
            .map_or(self.target.as_str(), |(path, _)| path)
    }

    /// The values of the field lines named `name` (ASCII case-insensitive), in order.
    pub fn values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.fields
            .iter()
            .filter(move |f| f.is(name))
            .map(|f| f.value.as_str())
    }
}

/// A request that the server refuses with `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejection {
    /// The status of the answer: `400`, `413`, `414`, `431`, `501` or `505`.
    pub status: u16,
    /// What is wrong, for the log line. A fixed text: never an input byte.
    pub reason: &'static str,
}

/// Why no request head was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// The peer closed the connection before the first byte.
    Closed,
    /// The read deadline passed (answer `408`).
    TimedOut,
    /// Another I/O error: the connection is broken, no answer is possible.
    Io(io::ErrorKind),
    /// The request is invalid or too large.
    Reject(Rejection),
}

const fn reject(status: u16, reason: &'static str) -> ReadError {
    ReadError::Reject(Rejection { status, reason })
}

const CR: u8 = b'\r';
const LF: u8 = b'\n';

fn io_error(error: &io::Error) -> ReadError {
    match error.kind() {
        // A socket read timeout is `WouldBlock` on Unix and `TimedOut` on Windows.
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ReadError::TimedOut,
        kind => ReadError::Io(kind),
    }
}

/// Read and check one request head.
///
/// # Errors
/// [`ReadError`]; see [`read_head_bytes`] and [`parse_head`].
pub fn read_head<R: BufRead>(input: &mut R) -> Result<RequestHead, ReadError> {
    let bytes = read_head_bytes(input)?;
    parse_head(&bytes)
}

/// Read the bytes of one head, up to and including the empty line that ends it.
///
/// Empty lines before the request line are skipped (RFC 9112 §2.2). The function
/// consumes only the bytes it looked at; the body stays in `input`. It stops at
/// the first defect that the line framing shows: a CR that is not followed by LF,
/// or an LF that does not follow a CR (`400`); more than [`MAX_HEAD_BYTES`]
/// bytes (`414` inside the request line, else `431`).
///
/// # Errors
/// [`ReadError::Closed`] when the input ends before the first byte; `400` when it
/// ends inside the head; [`ReadError::TimedOut`] and [`ReadError::Io`] for I/O.
pub fn read_head_bytes<R: BufRead>(input: &mut R) -> Result<Vec<u8>, ReadError> {
    let mut head: Vec<u8> = Vec::new();
    // Every byte looked at, with the skipped empty lines; at most MAX_HEAD_BYTES.
    let mut seen: usize = 0;
    let mut after_cr = false;
    loop {
        let chunk = match input.fill_buf() {
            Ok(chunk) => chunk,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(io_error(&e)),
        };
        if chunk.is_empty() {
            return Err(if seen == 0 {
                ReadError::Closed
            } else {
                reject(400, "the connection closed inside the head")
            });
        }
        let mut used: usize = 0;
        let mut outcome: Option<Result<(), ReadError>> = None;
        for &byte in chunk {
            if seen >= MAX_HEAD_BYTES {
                outcome = Some(Err(if head.contains(&LF) {
                    reject(431, "the head is larger than 64 KiB")
                } else {
                    reject(414, "the request line is larger than 64 KiB")
                }));
                break;
            }
            seen = seen.saturating_add(1);
            used = used.saturating_add(1);
            if after_cr {
                after_cr = false;
                if byte != LF {
                    outcome = Some(Err(reject(400, "a CR without LF")));
                    break;
                }
                head.push(byte);
                if head.as_slice() == b"\r\n" {
                    head.clear();
                } else if head.ends_with(b"\r\n\r\n") {
                    outcome = Some(Ok(()));
                    break;
                }
            } else if byte == LF {
                outcome = Some(Err(reject(400, "an LF without CR")));
                break;
            } else {
                after_cr = byte == CR;
                head.push(byte);
            }
        }
        input.consume(used);
        match outcome {
            Some(Ok(())) => return Ok(head),
            Some(Err(e)) => return Err(e),
            None => {}
        }
    }
}

/// RFC 9110 §5.6.2 `tchar`.
#[must_use]
pub fn is_tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// RFC 9110 §5.6.2 `token`: one or more `tchar`.
#[must_use]
pub fn is_token(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|&b| is_tchar(b))
}

/// RFC 9110 §5.5 `field-value` byte: HTAB, SP, VCHAR or obs-text. CR, LF, NUL
/// and the other control characters are not.
#[must_use]
pub fn is_field_value_byte(byte: u8) -> bool {
    byte == b'\t' || (0x20..=0x7e).contains(&byte) || byte >= 0x80
}

fn is_ows(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// `bytes` without leading and trailing spaces and tabs.
#[must_use]
pub fn trim_ows(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|&b| !is_ows(b))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&b| !is_ows(b))
        .map_or(start, |i| i.saturating_add(1));
    bytes.get(start..end).unwrap_or_default()
}

/// Check the bytes of one head (they end with CRLF CRLF) and give the request.
///
/// Request line: `method SP request-target SP HTTP-version` with single spaces; the
/// method is a token; the target is in origin-form (`/`, then visible ASCII
/// without `#`) else `400`; `HTTP/1.1` or `HTTP/1.0`, another `HTTP/x.y` is `505`.
/// Field line: `token ":" OWS value OWS`; a space before the colon, obs-fold, an
/// invalid name, a control character or a value that is not UTF-8 is `400`; more
/// than [`MAX_FIELD_LINES`] lines is `431`. `Transfer-Encoding` is `501`.
/// `Content-Length`: a list of digits; different values are `400`, the same value
/// repeated is one value (RFC 9112 §6.3). `Host`: more than one line, or none in
/// `HTTP/1.1`, is `400` (RFC 9112 §3.2).
///
/// # Errors
/// [`ReadError::Reject`] with the status above.
pub fn parse_head(bytes: &[u8]) -> Result<RequestHead, ReadError> {
    if bytes.len() > MAX_HEAD_BYTES {
        return Err(reject(431, "the head is larger than 64 KiB"));
    }
    // Without the final empty line, every line ends with CRLF.
    let Some(content) = bytes.strip_suffix(b"\r\n").filter(|c| c.ends_with(b"\r\n")) else {
        return Err(reject(400, "the head does not end with an empty line"));
    };
    let mut lines = content.split_inclusive(|&b| b == LF);
    let request_line = strip_crlf(lines.next().unwrap_or_default())?;
    let (method, target, version) = parse_request_line(request_line)?;
    let mut fields = Vec::new();
    for raw in lines {
        if fields.len() >= MAX_FIELD_LINES {
            return Err(reject(431, "more than 256 field lines"));
        }
        fields.push(parse_field_line(strip_crlf(raw)?)?);
    }
    let content_length = framing(&fields)?;
    let hosts = fields.iter().filter(|f| f.is("host")).count();
    if hosts > 1 || (hosts == 0 && version == Version::Http11) {
        return Err(reject(400, "not exactly one Host field line"));
    }
    Ok(RequestHead {
        method,
        target,
        version,
        fields,
        content_length,
    })
}

/// A line of `split_inclusive(LF)` without its CRLF. An LF without CR before it
/// and a CR inside the line are `400`.
fn strip_crlf(line: &[u8]) -> Result<&[u8], ReadError> {
    let Some(line) = line.strip_suffix(b"\r\n") else {
        return Err(reject(400, "an LF without CR"));
    };
    if line.contains(&CR) {
        return Err(reject(400, "a CR without LF"));
    }
    Ok(line)
}

fn parse_request_line(line: &[u8]) -> Result<(String, String, Version), ReadError> {
    let mut parts = line.split(|&b| b == b' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(reject(400, "the request line is not three parts"));
    };
    let version = match version {
        b"HTTP/1.1" => Version::Http11,
        b"HTTP/1.0" => Version::Http10,
        [b'H', b'T', b'T', b'P', b'/', major, b'.', minor]
            if major.is_ascii_digit() && minor.is_ascii_digit() =>
        {
            return Err(reject(505, "an HTTP version other than 1.0 and 1.1"));
        }
        _ => return Err(reject(400, "an invalid HTTP version")),
    };
    if !is_token(method) {
        return Err(reject(400, "the method is not a token"));
    }
    if target.first() != Some(&b'/') {
        return Err(reject(400, "the request target is not in origin-form"));
    }
    if !target
        .iter()
        .all(|&b| (0x21..=0x7e).contains(&b) && b != b'#')
    {
        return Err(reject(400, "an invalid byte in the request target"));
    }
    Ok((ascii(method), ascii(target), version))
}

/// Bytes that are checked to be ASCII.
fn ascii(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn parse_field_line(line: &[u8]) -> Result<Field, ReadError> {
    if line.is_empty() {
        return Err(reject(400, "an empty line inside the head"));
    }
    if line.first().copied().is_some_and(is_ows) {
        return Err(reject(
            400,
            "obs-fold or a space at the start of a field line",
        ));
    }
    let Some(colon) = line.iter().position(|&b| b == b':') else {
        return Err(reject(400, "a field line without a colon"));
    };
    let (name, rest) = line.split_at(colon);
    if name.last().copied().is_some_and(is_ows) {
        return Err(reject(400, "a space before the colon"));
    }
    if !is_token(name) {
        return Err(reject(400, "the field name is not a token"));
    }
    let value = trim_ows(rest.get(1..).unwrap_or_default());
    if !value.iter().all(|&b| is_field_value_byte(b)) {
        return Err(reject(400, "a control character in a field value"));
    }
    let Ok(value) = std::str::from_utf8(value) else {
        return Err(reject(400, "a field value that is not UTF-8"));
    };
    Ok(Field {
        name: ascii(name),
        value: value.to_owned(),
    })
}

/// The body length of the field lines (RFC 9112 §6.1–§6.3).
fn framing(fields: &[Field]) -> Result<u64, ReadError> {
    if fields.iter().any(|f| f.is("transfer-encoding")) {
        return Err(reject(501, "Transfer-Encoding is not supported"));
    }
    let mut length: Option<&str> = None;
    for field in fields.iter().filter(|f| f.is("content-length")) {
        for element in field.value.split(',') {
            let element = element.trim_matches([' ', '\t']);
            if element.is_empty() || !element.bytes().all(|b| b.is_ascii_digit()) {
                return Err(reject(400, "Content-Length is not a number"));
            }
            let digits = element.trim_start_matches('0');
            match length {
                None => length = Some(digits),
                Some(first) if first == digits => {}
                Some(_) => return Err(reject(400, "Content-Length values differ")),
            }
        }
    }
    // Leading zeros are removed; an empty string is 0. More than 19 digits is
    // larger than any limit: u64::MAX.
    Ok(length.map_or(0, |digits| {
        if digits.is_empty() {
            0
        } else {
            digits.parse::<u64>().unwrap_or(u64::MAX)
        }
    }))
}

/// Read a body of `length` bytes from `input` (after [`read_head`]).
///
/// # Errors
/// `413` when `length` is larger than [`MAX_BODY_BYTES`] (nothing is read); `400`
/// when the input ends early; [`ReadError::TimedOut`] and [`ReadError::Io`].
pub fn read_body<R: Read>(input: &mut R, length: u64) -> Result<Vec<u8>, ReadError> {
    if length > MAX_BODY_BYTES {
        return Err(reject(413, "the body is larger than 8 MiB"));
    }
    let mut body = Vec::new();
    match input.take(length).read_to_end(&mut body) {
        Ok(_) => {}
        Err(e) => return Err(io_error(&e)),
    }
    if u64::try_from(body.len()).ok() != Some(length) {
        return Err(reject(400, "the connection closed inside the body"));
    }
    Ok(body)
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

    fn read(bytes: &[u8]) -> Result<RequestHead, ReadError> {
        let mut input = bytes;
        read_head(&mut input)
    }

    fn status(bytes: &[u8]) -> u16 {
        match read(bytes) {
            Err(ReadError::Reject(r)) => r.status,
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    fn reason(bytes: &[u8]) -> &'static str {
        match read(bytes) {
            Err(ReadError::Reject(r)) => r.reason,
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    #[test]
    fn reader_keeps_name_case_order_repeats_and_trims_ows() {
        let head = read(
            b"GET /catalog?x=1 HTTP/1.1\r\nHost: shop.example.com\r\nAccept-Diff:  true \t\r\naccept-diff:true\r\nX-Empty:\r\nX-Utf8: \xc3\xa9t\xc3\xa9\r\n\r\n",
        )
        .unwrap();
        assert_eq!(head.method, "GET");
        assert_eq!(head.target, "/catalog?x=1");
        assert_eq!(head.path(), "/catalog");
        assert_eq!(head.version, Version::Http11);
        assert_eq!(
            head.fields,
            vec![
                Field::new("Host", "shop.example.com"),
                Field::new("Accept-Diff", "true"),
                Field::new("accept-diff", "true"),
                Field::new("X-Empty", ""),
                Field::new("X-Utf8", "été"),
            ]
        );
        assert_eq!(
            head.values("ACCEPT-DIFF").collect::<Vec<_>>(),
            ["true", "true"]
        );
        assert_eq!(head.content_length, 0);
    }

    #[test]
    fn reader_path_without_query() {
        let head = read(b"GET /a/b HTTP/1.0\r\n\r\n").unwrap();
        assert_eq!(head.path(), "/a/b");
        assert_eq!(head.version, Version::Http10);
        let head = read(b"GET /?q HTTP/1.0\r\n\r\n").unwrap();
        assert_eq!(head.path(), "/");
    }

    #[test]
    fn reader_http10_needs_no_host() {
        assert!(read(b"GET / HTTP/1.0\r\n\r\n").is_ok());
        assert_eq!(status(b"GET / HTTP/1.1\r\n\r\n"), 400);
        assert_eq!(status(b"GET / HTTP/1.0\r\nHost: a\r\nhost: a\r\n\r\n"), 400);
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nHOST: b\r\n\r\n"),
            "not exactly one Host field line"
        );
    }

    #[test]
    fn reader_skips_empty_lines_before_the_request_line() {
        let head = read(b"\r\n\r\nGET / HTTP/1.1\r\nHost: a\r\n\r\n").unwrap();
        assert_eq!(head.target, "/");
    }

    #[test]
    fn reader_leaves_the_body_in_the_input() {
        let mut input: &[u8] = b"POST /x HTTP/1.1\r\nHost: a\r\nContent-Length: 3\r\n\r\nabcdef";
        let head = read_head(&mut input).unwrap();
        assert_eq!(head.content_length, 3);
        assert_eq!(read_body(&mut input, head.content_length).unwrap(), b"abc");
        assert_eq!(input, b"def");
    }

    #[test]
    fn reader_rejects_bare_lf_and_bare_cr() {
        assert_eq!(reason(b"GET / HTTP/1.1\nHost: a\n\n"), "an LF without CR");
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\rb\r\n\r\n"),
            "a CR without LF"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\r\n\r\n"),
            "a CR without LF"
        );
        // The pure parser finds them too.
        assert_eq!(
            parse_head(b"GET / HTTP/1.1\r\nHost: a\nX: b\r\n\r\n"),
            Err(reject(400, "an LF without CR"))
        );
        assert_eq!(
            parse_head(b"GET / HTTP/1.1\r\nHost: a\rX: b\r\n\r\n"),
            Err(reject(400, "a CR without LF"))
        );
    }

    #[test]
    fn reader_rejects_obs_fold_and_space_before_colon() {
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nX: b\r\n c\r\n\r\n"),
            "obs-fold or a space at the start of a field line"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nX: b\r\n\tc\r\n\r\n"),
            "obs-fold or a space at the start of a field line"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nX : b\r\n\r\n"),
            "a space before the colon"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nX\t: b\r\n\r\n"),
            "a space before the colon"
        );
    }

    #[test]
    fn reader_rejects_invalid_names_and_lines() {
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nX(y): b\r\n\r\n"),
            "the field name is not a token"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\n: b\r\n\r\n"),
            "the field name is not a token"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nno colon\r\n\r\n"),
            "a field line without a colon"
        );
        assert_eq!(
            parse_head(b"GET / HTTP/1.1\r\nHost: a\r\n\r\nX: b\r\n\r\n"),
            Err(reject(400, "an empty line inside the head"))
        );
    }

    #[test]
    fn reader_rejects_control_characters_and_non_utf8_values() {
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nX: a\x00b\r\n\r\n"),
            "a control character in a field value"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nX: a\x7fb\r\n\r\n"),
            "a control character in a field value"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nX: a\x1fb\r\n\r\n"),
            "a control character in a field value"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1\r\nHost: a\r\nX: \xff\xfe\r\n\r\n"),
            "a field value that is not UTF-8"
        );
        // HTAB inside a value is allowed.
        let head = read(b"GET / HTTP/1.1\r\nHost: a\r\nX: a\tb\r\n\r\n").unwrap();
        assert_eq!(head.fields[1].value, "a\tb");
    }

    #[test]
    fn reader_request_line_rules() {
        assert_eq!(
            reason(b"GET  / HTTP/1.1\r\nHost: a\r\n\r\n"),
            "the request line is not three parts"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.1 x\r\nHost: a\r\n\r\n"),
            "the request line is not three parts"
        );
        assert_eq!(
            reason(b"GET /\r\nHost: a\r\n\r\n"),
            "the request line is not three parts"
        );
        assert_eq!(status(b"GET / HTTP/2.0\r\nHost: a\r\n\r\n"), 505);
        assert_eq!(status(b"GET / HTTP/1.2\r\nHost: a\r\n\r\n"), 505);
        assert_eq!(
            reason(b"GET / http/1.1\r\nHost: a\r\n\r\n"),
            "an invalid HTTP version"
        );
        assert_eq!(
            reason(b"GET / HTTP/1.x\r\nHost: a\r\n\r\n"),
            "an invalid HTTP version"
        );
        assert_eq!(
            reason(b"GET / HTTP/x.1\r\nHost: a\r\n\r\n"),
            "an invalid HTTP version"
        );
        assert_eq!(
            reason(b"G(T / HTTP/1.1\r\nHost: a\r\n\r\n"),
            "the method is not a token"
        );
        assert_eq!(
            reason(b" / HTTP/1.1\r\nHost: a\r\n\r\n"),
            "the method is not a token"
        );
    }

    #[test]
    fn reader_rejects_targets_that_are_not_origin_form() {
        for target in [
            &b"http://shop.example.com/"[..],
            b"*",
            b"shop.example.com:443",
            b"",
        ] {
            let mut req = b"GET ".to_vec();
            req.extend_from_slice(target);
            req.extend_from_slice(b" HTTP/1.1\r\nHost: a\r\n\r\n");
            assert_eq!(status(&req), 400, "{}", String::from_utf8_lossy(target));
        }
        assert_eq!(
            reason(b"GET /a#b HTTP/1.1\r\nHost: a\r\n\r\n"),
            "an invalid byte in the request target"
        );
        assert_eq!(
            reason(b"GET /\xc3\xa9 HTTP/1.1\r\nHost: a\r\n\r\n"),
            "an invalid byte in the request target"
        );
        assert_eq!(
            reason(b"GET /\x7f HTTP/1.1\r\nHost: a\r\n\r\n"),
            "an invalid byte in the request target"
        );
        // The visible ASCII edges are allowed.
        assert_eq!(
            read(b"GET /!~ HTTP/1.1\r\nHost: a\r\n\r\n").unwrap().target,
            "/!~"
        );
    }

    #[test]
    fn reader_transfer_encoding_is_501() {
        assert_eq!(
            status(b"POST / HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n"),
            501
        );
        assert_eq!(
            status(
                b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 3\r\ntransfer-encoding: gzip\r\n\r\n"
            ),
            501
        );
    }

    fn length(value: &str) -> Result<u64, ReadError> {
        let req = format!("POST / HTTP/1.1\r\nHost: a\r\n{value}\r\n\r\n");
        read(req.as_bytes()).map(|h| h.content_length)
    }

    #[test]
    fn reader_content_length_rules() {
        assert_eq!(length("Content-Length: 12"), Ok(12));
        assert_eq!(length("Content-Length: 0"), Ok(0));
        assert_eq!(length("Content-Length: 000"), Ok(0));
        assert_eq!(length("Content-Length: 007"), Ok(7));
        assert_eq!(length("Content-Length: 5, 5"), Ok(5));
        assert_eq!(length("Content-Length: 5\r\ncontent-length: 05"), Ok(5));
        assert_eq!(
            length("Content-Length: 99999999999999999999999"),
            Ok(u64::MAX)
        );
        let bad = Err(reject(400, "Content-Length is not a number"));
        assert_eq!(length("Content-Length: "), bad);
        assert_eq!(length("Content-Length: +5"), bad);
        assert_eq!(length("Content-Length: 5,"), bad);
        assert_eq!(length("Content-Length: 0x5"), bad);
        assert_eq!(length("Content-Length: 5 5"), bad);
        let differ = Err(reject(400, "Content-Length values differ"));
        assert_eq!(length("Content-Length: 5, 6"), differ);
        assert_eq!(length("Content-Length: 5\r\nContent-Length: 6"), differ);
        assert_eq!(length("Content-Length: 0, 1"), differ);
    }

    #[test]
    fn reader_field_section_of_8_kib_and_near_64_kib_works() {
        for size in [8 * 1024, MAX_HEAD_BYTES - 100] {
            let mut req = b"GET / HTTP/1.1\r\nHost: a\r\nX-Pad: ".to_vec();
            req.resize(size - 4, b'p');
            req.extend_from_slice(b"\r\n\r\n");
            assert_eq!(req.len(), size);
            let head = read(&req).unwrap();
            assert_eq!(head.fields.len(), 2);
        }
    }

    #[test]
    fn reader_head_limit_is_exact() {
        let build = |size: usize| {
            let mut req = b"GET / HTTP/1.1\r\nHost: a\r\nX-Pad: ".to_vec();
            req.resize(size - 4, b'p');
            req.extend_from_slice(b"\r\n\r\n");
            req
        };
        assert!(read(&build(MAX_HEAD_BYTES)).is_ok());
        assert_eq!(
            read(&build(MAX_HEAD_BYTES + 1)),
            Err(reject(431, "the head is larger than 64 KiB"))
        );
        assert_eq!(
            parse_head(&build(MAX_HEAD_BYTES + 1)),
            Err(reject(431, "the head is larger than 64 KiB"))
        );
        assert!(parse_head(&build(MAX_HEAD_BYTES)).is_ok());
    }

    #[test]
    fn reader_stops_at_the_limit_without_reading_more() {
        let mut req = b"GET / HTTP/1.1\r\nHost: a\r\nX-Pad: ".to_vec();
        req.resize(200_000, b'p');
        let mut input = req.as_slice();
        assert_eq!(
            read_head(&mut input).map(|_| ()),
            Err(reject(431, "the head is larger than 64 KiB"))
        );
        assert_eq!(req.len() - input.len(), MAX_HEAD_BYTES);
    }

    #[test]
    fn reader_long_request_line_is_414() {
        let mut req = b"GET /".to_vec();
        req.resize(MAX_HEAD_BYTES + 10, b'a');
        assert_eq!(status(&req), 414);
    }

    #[test]
    fn reader_field_line_count_limit() {
        let build = |n: usize| {
            let mut req = b"GET / HTTP/1.1\r\nHost: a\r\n".to_vec();
            for _ in 1..n {
                req.extend_from_slice(b"X: 1\r\n");
            }
            req.extend_from_slice(b"\r\n");
            req
        };
        assert_eq!(read(&build(MAX_FIELD_LINES)).unwrap().fields.len(), 256);
        assert_eq!(
            read(&build(MAX_FIELD_LINES + 1)),
            Err(reject(431, "more than 256 field lines"))
        );
    }

    #[test]
    fn reader_end_of_input() {
        assert_eq!(read(b""), Err(ReadError::Closed));
        assert_eq!(
            read(b"GET / HTTP/1.1\r\nHost: a\r\n"),
            Err(reject(400, "the connection closed inside the head"))
        );
        assert_eq!(
            read(b"\r\n"),
            Err(reject(400, "the connection closed inside the head"))
        );
        assert_eq!(
            parse_head(b"GET / HTTP/1.1\r\nHost: a\r\n"),
            Err(reject(400, "the head does not end with an empty line"))
        );
    }

    #[test]
    fn reader_body_limits() {
        let mut input: &[u8] = b"abc";
        assert_eq!(
            read_body(&mut input, MAX_BODY_BYTES + 1),
            Err(reject(413, "the body is larger than 8 MiB"))
        );
        assert_eq!(input, b"abc", "nothing is read");
        assert_eq!(
            read_body(&mut input, 4),
            Err(reject(400, "the connection closed inside the body"))
        );
        let big = vec![7u8; usize::try_from(MAX_BODY_BYTES).unwrap()];
        let mut input = big.as_slice();
        assert_eq!(
            read_body(&mut input, MAX_BODY_BYTES).unwrap().len(),
            big.len()
        );
        let mut empty: &[u8] = b"";
        assert_eq!(read_body(&mut empty, 0), Ok(Vec::new()));
    }

    struct Failing(io::ErrorKind);
    impl Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(self.0.into())
        }
    }
    impl BufRead for Failing {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(self.0.into())
        }
        fn consume(&mut self, _: usize) {}
    }

    #[test]
    fn reader_maps_io_errors() {
        assert_eq!(
            read_head(&mut Failing(io::ErrorKind::WouldBlock)),
            Err(ReadError::TimedOut)
        );
        assert_eq!(
            read_head(&mut Failing(io::ErrorKind::TimedOut)),
            Err(ReadError::TimedOut)
        );
        assert_eq!(
            read_head(&mut Failing(io::ErrorKind::ConnectionReset)),
            Err(ReadError::Io(io::ErrorKind::ConnectionReset))
        );
        assert_eq!(
            read_body(&mut Failing(io::ErrorKind::WouldBlock), 3),
            Err(ReadError::TimedOut)
        );
    }

    #[test]
    fn reader_retries_interrupted() {
        struct Once(bool, &'static [u8]);
        impl Read for Once {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Ok(0)
            }
        }
        impl BufRead for Once {
            fn fill_buf(&mut self) -> io::Result<&[u8]> {
                if self.0 {
                    Ok(self.1)
                } else {
                    self.0 = true;
                    Err(io::ErrorKind::Interrupted.into())
                }
            }
            fn consume(&mut self, n: usize) {
                self.1 = &self.1[n..];
            }
        }
        let head = read_head(&mut Once(false, b"GET / HTTP/1.0\r\n\r\n")).unwrap();
        assert_eq!(head.target, "/");
    }

    #[test]
    fn reader_token_and_value_bytes() {
        for b in b"!#$%&'*+-.^_`|~09azAZ" {
            assert!(is_tchar(*b), "{b}");
        }
        for b in b"\"(),/:;<=>?@[\\]{} \t\x7f\x80" {
            assert!(!is_tchar(*b), "{b}");
        }
        assert!(!is_token(b""));
        assert!(is_field_value_byte(b'\t'));
        assert!(is_field_value_byte(b' '));
        assert!(is_field_value_byte(b'~'));
        assert!(is_field_value_byte(0x80));
        assert!(!is_field_value_byte(0x7f));
        assert!(!is_field_value_byte(0x1f));
        assert!(!is_field_value_byte(0x00));
        assert_eq!(trim_ows(b" \t a b \t"), b"a b");
        assert_eq!(trim_ows(b"   "), b"");
        assert_eq!(trim_ows(b""), b"");
        assert_eq!(trim_ows(b"x"), b"x");
    }
}
