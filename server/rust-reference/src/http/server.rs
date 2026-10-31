// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The server loop: one connection at a time, one request per connection.
//!
//! Per connection: one deadline for the whole request ([`Timeouts::read`], so a
//! slow client cannot hold the only connection), a write timeout, then
//! `Connection: close`. After an early answer (the body is not read) the server
//! shuts down its side and discards what the client still sends for at most
//! [`Timeouts::linger`], so that the client reads the answer and not a reset
//! (RFC 9112 §9.6). A panic in the handler of a connection is caught; the loop
//! goes on.
//!
//! Log (stderr): one line per request with the method, the path without the
//! query, the status, the duration and a fixed note. Never a field value, a
//! body or a query string.

use std::fmt;
use std::io::{self, BufReader, Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use crate::control::{Conn, Next, Outcome, Server, error_response, guard};
use crate::http::reader::{ReadError, RequestHead, read_body, read_head};
use crate::http::writer::write_response;

/// The time limits of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// The whole request (head and body) must arrive within this time; then `408`.
    pub read: Duration,
    /// The whole answer must be written within this time; then the connection
    /// is closed.
    pub write: Duration,
    /// After an early answer: the longest time to discard client bytes.
    pub linger: Duration,
}

impl Default for Timeouts {
    /// 10 s read, 10 s write, 2 s linger.
    fn default() -> Self {
        Self {
            read: Duration::from_secs(10),
            write: Duration::from_secs(10),
            linger: Duration::from_secs(2),
        }
    }
}

/// The most bytes discarded after an early answer.
const MAX_LINGER_BYTES: u64 = 16 * 1024 * 1024;
/// The longest method and path in a log line.
const MAX_LOG_METHOD: usize = 32;
const MAX_LOG_PATH: usize = 256;

/// Write one line to stderr. When stderr is closed there is no other channel:
/// the line is dropped.
pub fn log(line: fmt::Arguments<'_>) {
    let mut err = io::stderr().lock();
    if writeln!(err, "{line}").is_err() {
        // No other channel to report on.
    }
}

/// Serve `listener` forever, one connection at a time.
pub fn serve(listener: &TcpListener, server: &mut Server, timeouts: &Timeouts) -> ! {
    loop {
        match listener.accept() {
            Ok((stream, peer)) => {
                if guard(|| serve_connection(&stream, peer.ip(), server, timeouts)).is_err() {
                    log(format_args!(
                        "connection handler panicked; the server goes on"
                    ));
                }
            }
            Err(e) => {
                log(format_args!("accept failed: {:?}", e.kind()));
                // A persistent error (for example no free file descriptor) must not
                // make the loop spin.
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// A reader with one deadline over all reads.
struct Deadline<'a> {
    stream: &'a TcpStream,
    until: Option<Instant>,
}

impl Read for Deadline<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(until) = self.until {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            self.stream.set_read_timeout(Some(left))?;
        }
        let mut stream = self.stream;
        stream.read(buf)
    }
}

/// A writer with one deadline over all writes: a client that reads slowly cannot
/// hold the only connection longer than [`Timeouts::write`].
struct DeadlineWriter<'a> {
    stream: &'a TcpStream,
    until: Option<Instant>,
}

impl Write for DeadlineWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(until) = self.until {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            self.stream.set_write_timeout(Some(left))?;
        }
        let mut stream = self.stream;
        stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut stream = self.stream;
        stream.flush()
    }
}

/// The JSON error code of a status of the HTTP layer.
fn http_error_code(status: u16) -> &'static str {
    match status {
        408 => "TIMEOUT",
        413 => "TOO_LARGE",
        414 => "URI_TOO_LONG",
        431 => "FIELDS_TOO_LARGE",
        501 => "NOT_IMPLEMENTED",
        505 => "VERSION_NOT_SUPPORTED",
        _ => "BAD_REQUEST",
    }
}

/// An answer of the HTTP layer for a request that cannot be read.
fn read_failure(error: ReadError) -> Option<Outcome> {
    match error {
        ReadError::Closed | ReadError::Io(_) => None,
        ReadError::TimedOut => Some(Outcome {
            response: error_response(408, http_error_code(408)),
            note: Some("the request did not arrive in time"),
        }),
        ReadError::Reject(r) => Some(Outcome {
            response: error_response(r.status, http_error_code(r.status)),
            note: Some(r.reason),
        }),
    }
}

/// What the log line says about the request.
struct Seen<'a> {
    method: &'a str,
    path: &'a str,
}

const UNSEEN: Seen<'static> = Seen {
    method: "-",
    path: "-",
};

fn log_request(seen: &Seen<'_>, status: u16, start: Instant, note: Option<&str>) {
    let method = seen.method.get(..MAX_LOG_METHOD).unwrap_or(seen.method);
    let path = seen.path.get(..MAX_LOG_PATH).unwrap_or(seen.path);
    let ms = start.elapsed().as_millis();
    match note {
        Some(note) => log(format_args!("{method} {path} {status} {ms}ms ({note})")),
        None => log(format_args!("{method} {path} {status} {ms}ms")),
    }
}

fn serve_connection(stream: &TcpStream, peer: IpAddr, server: &mut Server, timeouts: &Timeouts) {
    let start = Instant::now();
    let local_port = match stream.local_addr() {
        Ok(addr) => addr.port(),
        Err(e) => {
            log(format_args!(
                "connection without a local address: {:?}",
                e.kind()
            ));
            return;
        }
    };
    let conn = Conn { peer, local_port };
    let mut input = BufReader::with_capacity(
        8 * 1024,
        Deadline {
            stream,
            until: start.checked_add(timeouts.read),
        },
    );
    let head = match read_head(&mut input) {
        Ok(head) => head,
        Err(error) => {
            if let ReadError::Io(kind) = error {
                log(format_args!(
                    "- - - {}ms (read failed: {kind:?})",
                    start.elapsed().as_millis()
                ));
            }
            if let Some(outcome) = read_failure(error) {
                answer(
                    stream, &mut input, &UNSEEN, &outcome, false, true, start, timeouts,
                );
            }
            return;
        }
    };
    let seen = Seen {
        method: &head.method,
        path: head.path(),
    };
    let (outcome, early) = match server.route(&conn, &head) {
        Next::Respond(outcome) => (outcome, head.content_length > 0),
        Next::ReadBody(route) => match read_body(&mut input, head.content_length) {
            Ok(body) => (server.respond(route, &head, &body), false),
            Err(error) => {
                let Some(outcome) = read_failure(error) else {
                    log_request(&seen, 0, start, Some("the body read failed"));
                    return;
                };
                (outcome, true)
            }
        },
    };
    answer(
        stream,
        &mut input,
        &seen,
        &outcome,
        is_head(&head),
        early,
        start,
        timeouts,
    );
}

fn is_head(head: &RequestHead) -> bool {
    head.method == "HEAD"
}

#[allow(clippy::too_many_arguments)]
fn answer(
    stream: &TcpStream,
    input: &mut BufReader<Deadline<'_>>,
    seen: &Seen<'_>,
    outcome: &Outcome,
    head_request: bool,
    early: bool,
    start: Instant,
    timeouts: &Timeouts,
) {
    let mut out = DeadlineWriter {
        stream,
        until: Instant::now().checked_add(timeouts.write),
    };
    let written = write_response(&mut out, &outcome.response, head_request);
    let note = match &written {
        Ok(()) => outcome.note,
        Err(_) => Some("the answer could not be written"),
    };
    log_request(seen, outcome.response.status, start, note);
    if stream.shutdown(Shutdown::Write).is_err() {
        // The peer closed first: the answer is written or lost; nothing to do.
        return;
    }
    if early && written.is_ok() {
        discard(input, timeouts.linger);
    }
}

/// Discard client bytes until the end of the stream, an error, the time limit or
/// [`MAX_LINGER_BYTES`]. The bytes are never kept.
fn discard(input: &mut BufReader<Deadline<'_>>, linger: Duration) {
    input.get_mut().until = Instant::now().checked_add(linger);
    let mut sink = [0u8; 8 * 1024];
    let mut left = MAX_LINGER_BYTES;
    while left > 0 {
        match input.read(&mut sink) {
            Ok(0) | Err(_) => return,
            Ok(n) => left = left.saturating_sub(u64::try_from(n).unwrap_or(u64::MAX)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_http_error_codes() {
        assert_eq!(http_error_code(400), "BAD_REQUEST");
        assert_eq!(http_error_code(408), "TIMEOUT");
        assert_eq!(http_error_code(413), "TOO_LARGE");
        assert_eq!(http_error_code(414), "URI_TOO_LONG");
        assert_eq!(http_error_code(431), "FIELDS_TOO_LARGE");
        assert_eq!(http_error_code(501), "NOT_IMPLEMENTED");
        assert_eq!(http_error_code(505), "VERSION_NOT_SUPPORTED");
    }

    #[test]
    fn server_read_failures_and_answers() {
        assert_eq!(read_failure(ReadError::Closed), None);
        assert_eq!(
            read_failure(ReadError::Io(io::ErrorKind::ConnectionReset)),
            None
        );
        let o = read_failure(ReadError::TimedOut);
        assert_eq!(o.map(|o| o.response.status), Some(408));
    }

    #[test]
    fn server_default_timeouts() {
        let t = Timeouts::default();
        assert_eq!(t.read, Duration::from_secs(10));
        assert_eq!(t.write, Duration::from_secs(10));
        assert_eq!(t.linger, Duration::from_secs(2));
    }
}
