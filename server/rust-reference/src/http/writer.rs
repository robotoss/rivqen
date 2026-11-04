// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! HTTP/1.1 response writer (RFC 9112 §4–§6).
//!
//! It writes the status line, then the field lines of the [`Response`] exactly as
//! given (name case, order, repeats), then `Content-Length` for [`Body::Bytes`],
//! then `Connection: close`, then the body. It adds no other line (no `Date`, no
//! `Server`). There is no body for a `HEAD` request, for [`Body::None`] and for
//! the statuses without content (`204`, `304`; RFC 9110 §6.4.1); `204` and `304`
//! also get no `Content-Length` (RFC 9110 §8.6).

use std::io::{self, Write};

use crate::http::reader::{is_field_value_byte, is_token};
use crate::model::{Body, Response};

/// The fields that the writer owns: a response must not have them.
const FRAMING_FIELDS: [&str; 3] = ["content-length", "transfer-encoding", "connection"];

/// Check that `response` can be written exactly: a final status `200`…`599`;
/// each name a token and not a framing field; each value only field-value bytes
/// (no CR, LF, NUL or other control character; RFC 9110 §5.5).
///
/// # Errors
/// What is wrong, as a fixed text without input values.
pub fn check_response(response: &Response) -> Result<(), &'static str> {
    if !(200..=599).contains(&response.status) {
        return Err("the status is not 200 to 599");
    }
    for field in &response.fields {
        if !is_token(field.name.as_bytes()) {
            return Err("a field name is not a token");
        }
        if FRAMING_FIELDS.iter().any(|name| field.is(name)) {
            return Err("a framing field (Content-Length, Transfer-Encoding, Connection)");
        }
        if !field.value.bytes().all(is_field_value_byte) {
            return Err("a control character in a field value");
        }
    }
    Ok(())
}

/// The reason phrase of the status line (RFC 9110 §15); empty for other codes.
#[must_use]
pub fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        304 => "Not Modified",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Content Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        421 => "Misdirected Request",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        _ => "",
    }
}

/// The bytes of the head: status line, field lines, framing lines, empty line.
///
/// # Errors
/// [`io::ErrorKind::InvalidInput`] when [`check_response`] refuses the response.
pub fn head_bytes(response: &Response) -> io::Result<Vec<u8>> {
    check_response(response)
        .map_err(|reason| io::Error::new(io::ErrorKind::InvalidInput, reason))?;
    let mut head = Vec::new();
    head.extend_from_slice(b"HTTP/1.1 ");
    head.extend_from_slice(response.status.to_string().as_bytes());
    head.push(b' ');
    head.extend_from_slice(reason_phrase(response.status).as_bytes());
    head.extend_from_slice(b"\r\n");
    for field in &response.fields {
        head.extend_from_slice(field.name.as_bytes());
        head.extend_from_slice(b": ");
        head.extend_from_slice(field.value.as_bytes());
        head.extend_from_slice(b"\r\n");
    }
    if let Body::Bytes(bytes) = &response.body {
        if !no_content(response.status) {
            head.extend_from_slice(b"Content-Length: ");
            head.extend_from_slice(bytes.len().to_string().as_bytes());
            head.extend_from_slice(b"\r\n");
        }
    }
    head.extend_from_slice(b"Connection: close\r\n\r\n");
    Ok(head)
}

/// `204` and `304` never have content (RFC 9110 §15.3.5, §15.4.5).
fn no_content(status: u16) -> bool {
    status == 204 || status == 304
}

/// Write `response` to `out` and flush. `head_request`: the request method is
/// `HEAD`, so no body is sent.
///
/// # Errors
/// [`io::ErrorKind::InvalidInput`] before any byte when [`check_response`] refuses
/// the response; the I/O errors of `out`.
pub fn write_response<W: Write>(
    out: &mut W,
    response: &Response,
    head_request: bool,
) -> io::Result<()> {
    let head = head_bytes(response)?;
    out.write_all(&head)?;
    if let Body::Bytes(bytes) = &response.body {
        if !head_request && !no_content(response.status) {
            out.write_all(bytes)?;
        }
    }
    out.flush()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::Field;

    fn written(response: &Response, head_request: bool) -> Vec<u8> {
        let mut out = Vec::new();
        write_response(&mut out, response, head_request).unwrap();
        out
    }

    fn response(status: u16, fields: Vec<Field>, body: Body) -> Response {
        Response {
            status,
            fields,
            body,
        }
    }

    #[test]
    fn writer_keeps_case_order_and_repeats() {
        let r = response(
            200,
            vec![
                Field::new("Content-Type", "text/html; charset=utf-8"),
                Field::new("template-tag", "abc"),
                Field::new("Set-Cookie", "a=1"),
                Field::new("set-cookie", "b=2"),
                Field::new("X-Empty", ""),
                Field::new("X-Utf8", "été"),
            ],
            Body::Bytes(b"<p>hi</p>".to_vec()),
        );
        assert_eq!(
            written(&r, false),
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\ntemplate-tag: abc\r\nSet-Cookie: a=1\r\nset-cookie: b=2\r\nX-Empty: \r\nX-Utf8: été\r\nContent-Length: 9\r\nConnection: close\r\n\r\n<p>hi</p>"
                .as_bytes()
        );
    }

    #[test]
    fn writer_head_request_gets_length_but_no_body() {
        let r = response(200, vec![], Body::Bytes(b"abc".to_vec()));
        assert_eq!(
            written(&r, true),
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\n"
        );
    }

    #[test]
    fn writer_body_none_has_no_length_and_no_body() {
        let r = response(200, vec![Field::new("ETag", "\"x\"")], Body::None);
        assert_eq!(
            written(&r, false),
            b"HTTP/1.1 200 OK\r\nETag: \"x\"\r\nConnection: close\r\n\r\n"
        );
    }

    #[test]
    fn writer_204_and_304_have_no_length_and_no_body() {
        for (status, line) in [(204, "204 No Content"), (304, "304 Not Modified")] {
            let r = response(status, vec![], Body::Bytes(b"page".to_vec()));
            assert_eq!(
                written(&r, false),
                format!("HTTP/1.1 {line}\r\nConnection: close\r\n\r\n").as_bytes()
            );
        }
    }

    #[test]
    fn writer_other_status_has_an_empty_reason() {
        let r = response(299, vec![], Body::Bytes(Vec::new()));
        assert_eq!(
            written(&r, false),
            b"HTTP/1.1 299 \r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
    }

    #[test]
    fn writer_refuses_invalid_responses_before_any_byte() {
        let cases = [
            response(199, vec![], Body::None),
            response(600, vec![], Body::None),
            response(200, vec![Field::new("X Y", "1")], Body::None),
            response(200, vec![Field::new("", "1")], Body::None),
            response(200, vec![Field::new("Content-Length", "1")], Body::None),
            response(
                200,
                vec![Field::new("transfer-encoding", "chunked")],
                Body::None,
            ),
            response(
                200,
                vec![Field::new("CONNECTION", "keep-alive")],
                Body::None,
            ),
            response(200, vec![Field::new("X", "a\r\nInjected: 1")], Body::None),
            response(200, vec![Field::new("X", "a\nb")], Body::None),
            response(200, vec![Field::new("X", "a\0b")], Body::None),
        ];
        for r in cases {
            let mut out = Vec::new();
            let e = write_response(&mut out, &r, false).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::InvalidInput, "{r:?}");
            assert!(out.is_empty(), "{r:?}");
        }
        assert!(check_response(&response(200, vec![], Body::None)).is_ok());
        assert!(check_response(&response(599, vec![Field::new("X", "a\tb")], Body::None)).is_ok());
    }

    #[test]
    fn writer_reason_phrases() {
        for (status, phrase) in [
            (400, "Bad Request"),
            (403, "Forbidden"),
            (404, "Not Found"),
            (405, "Method Not Allowed"),
            (408, "Request Timeout"),
            (413, "Content Too Large"),
            (414, "URI Too Long"),
            (415, "Unsupported Media Type"),
            (422, "Unprocessable Content"),
            (421, "Misdirected Request"),
            (431, "Request Header Fields Too Large"),
            (500, "Internal Server Error"),
            (501, "Not Implemented"),
            (503, "Service Unavailable"),
            (505, "HTTP Version Not Supported"),
        ] {
            assert_eq!(reason_phrase(status), phrase);
        }
    }
}
