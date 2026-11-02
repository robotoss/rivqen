// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The server loop: an accept loop, a bounded queue and a pool of workers; one
//! request per connection (WP-03 S2 §2 "Hardening" items 1, 2 and 5; DL-097).
//!
//! The accept loop hands each connection to a queue of [`QUEUE_CAPACITY`]
//! connections. When the queue is full, the accept loop waits (backpressure: the
//! kernel backlog holds the next clients). [`ServeOptions::workers`] threads take
//! the connections from the queue in accept order; each serves one connection at
//! a time with a [`Handler`].
//!
//! Per connection ([`Timeouts`]): the first byte of the head must arrive within
//! [`Timeouts::first_byte`], the rest of the head within [`Timeouts::head`] after
//! it, the whole request (head and body) within [`Timeouts::read`], and the answer
//! must be written within [`Timeouts::write`]; then `Connection: close`. A
//! connection that sends no byte in time is closed without an answer; a request
//! that is late after its first byte gets `408`. After an early answer (the body
//! is not read) the server shuts down its side and discards what the client
//! still sends for at most [`Timeouts::linger`], so that the client reads the
//! answer and not a reset (RFC 9112 §9.6). A panic in a handler is caught; the
//! worker goes on.
//!
//! Log (stderr): one line per request with the method (when it is a method of
//! RFC 9110 §9 or `PATCH`, else `-`), the path that the handler allows
//! ([`LogPath`], else `-`), the status, the duration and a fixed note. Never a
//! field value, a body or a query string. Lines about the connections (accept
//! failure, I/O error, idle connection, panic) go through a [`LogLimiter`]: at
//! most one line of a kind per [`LOG_WINDOW`], with the count of the suppressed
//! lines (ERR-09).

use std::fmt;
use std::io::{self, BufReader, Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::http::handler::{Conn, Handler, LogPath, Next, Outcome, error_response, guard};
use crate::http::reader::{ReadError, RequestHead, read_body, read_head};
use crate::http::writer::write_response;

/// The time limits of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// The first byte of the head must arrive within this time after the worker
    /// takes the connection; else the connection is closed without an answer.
    pub first_byte: Duration,
    /// The rest of the head must arrive within this time after its first byte;
    /// then `408`.
    pub head: Duration,
    /// The whole request (head and body) must arrive within this time after the
    /// worker takes the connection; then `408`.
    pub read: Duration,
    /// The whole answer must be written within this time; then the connection
    /// is closed.
    pub write: Duration,
    /// After an early answer: the longest time to discard client bytes.
    pub linger: Duration,
}

impl Default for Timeouts {
    /// 5 s first byte, 5 s head, 10 s request, 10 s answer, 2 s linger.
    fn default() -> Self {
        Self {
            first_byte: Duration::from_secs(5),
            head: Duration::from_secs(5),
            read: Duration::from_secs(10),
            write: Duration::from_secs(10),
            linger: Duration::from_secs(2),
        }
    }
}

/// The capacity of the queue between the accept loop and the workers.
pub const QUEUE_CAPACITY: usize = 64;
/// The most workers.
pub const MAX_WORKERS: usize = 64;
/// The stack of a worker: the size of the main thread stack on Linux and macOS,
/// so that a worker has the stack that the S1 loop had on the main thread (also
/// with musl, where the default thread stack is small).
pub const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

/// The options of [`serve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServeOptions {
    /// The time limits of a connection.
    pub timeouts: Timeouts,
    /// The number of workers, 1 to [`MAX_WORKERS`] (a value outside is clamped).
    /// The conformance mode uses 1: the runner sends one case at a time, and one
    /// worker answers the connections in accept order.
    pub workers: usize,
}

impl Default for ServeOptions {
    /// The default time limits, one worker.
    fn default() -> Self {
        Self {
            timeouts: Timeouts::default(),
            workers: 1,
        }
    }
}

/// The most bytes discarded after an early answer.
const MAX_LINGER_BYTES: u64 = 16 * 1024 * 1024;
/// The longest path in a log line.
const MAX_LOG_PATH: usize = 256;
/// The methods that a log line shows (RFC 9110 §9, and `PATCH`); any other
/// method is `-`.
const LOG_METHODS: [&str; 9] = [
    "GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH",
];

/// Write one line to stderr. When stderr is closed there is no other channel:
/// the line is dropped.
pub fn log(line: fmt::Arguments<'_>) {
    let mut err = io::stderr().lock();
    if writeln!(err, "{line}").is_err() {
        // No other channel to report on.
    }
}

/// The time window of [`LogLimiter`].
pub const LOG_WINDOW: Duration = Duration::from_secs(1);
/// The most kinds that a [`LogLimiter`] keeps; a new kind then replaces the kind
/// with the oldest line.
pub const MAX_LOG_KINDS: usize = 64;

/// The event of a rate-limited log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogEvent {
    /// `accept` failed.
    AcceptFailed,
    /// A read of a request failed with an I/O error.
    ReadFailed,
    /// A connection sent no byte before the first-byte deadline.
    Idle,
    /// A handler panicked.
    Panicked,
    /// A connection has no local address.
    NoLocalAddress,
}

/// The kind of a rate-limited log line: the event and, for an I/O error, its
/// kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogKind {
    /// The event.
    pub event: LogEvent,
    /// The I/O error kind, when there is one.
    pub io: Option<io::ErrorKind>,
}

impl LogKind {
    /// A kind without an I/O error kind.
    #[must_use]
    pub const fn of(event: LogEvent) -> Self {
        Self { event, io: None }
    }

    /// A kind with an I/O error kind.
    #[must_use]
    pub const fn io(event: LogEvent, kind: io::ErrorKind) -> Self {
        Self {
            event,
            io: Some(kind),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    kind: LogKind,
    last: Instant,
    suppressed: u64,
}

/// A rate limit for repeated log lines (DL-097 item 5; ERR-09): at most one line
/// of a [`LogKind`] per [`LOG_WINDOW`]. The next line of the kind after the
/// window says how many lines were suppressed. The memory is bounded by
/// [`MAX_LOG_KINDS`].
#[derive(Debug, Default)]
pub struct LogLimiter {
    slots: Mutex<Vec<Slot>>,
}

impl LogLimiter {
    /// A limiter that has seen no line.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The line to write for a line of `kind` with `text` at `now`, or `None`
    /// when the line is suppressed.
    pub fn line(&self, kind: LogKind, now: Instant, text: fmt::Arguments<'_>) -> Option<String> {
        // The lock is held only for this bookkeeping, which cannot panic.
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        let suppressed = if let Some(slot) = slots.iter_mut().find(|s| s.kind == kind) {
            if now.saturating_duration_since(slot.last) < LOG_WINDOW {
                slot.suppressed = slot.suppressed.saturating_add(1);
                return None;
            }
            slot.last = now;
            std::mem::take(&mut slot.suppressed)
        } else {
            let slot = Slot {
                kind,
                last: now,
                suppressed: 0,
            };
            if slots.len() < MAX_LOG_KINDS {
                slots.push(slot);
            } else if let Some(oldest) = slots.iter_mut().min_by_key(|s| s.last) {
                *oldest = slot;
            }
            0
        };
        drop(slots);
        Some(if suppressed == 0 {
            text.to_string()
        } else {
            format!("{text} ({suppressed} similar lines suppressed)")
        })
    }

    /// Write the line of `kind` now, unless it is suppressed.
    pub fn log(&self, kind: LogKind, text: fmt::Arguments<'_>) {
        if let Some(line) = self.line(kind, Instant::now(), text) {
            log(format_args!("{line}"));
        }
    }
}

/// Serve `listener` with `handler` until the workers cannot run.
///
/// The function returns only when a worker cannot be started (then the started
/// workers end before it returns) or when no worker is left to take a
/// connection; the caller reports the error and exits.
pub fn serve<H: Handler>(listener: &TcpListener, handler: &H, options: &ServeOptions) -> io::Error {
    let limiter = LogLimiter::new();
    let (sender, receiver) = mpsc::sync_channel::<(TcpStream, IpAddr)>(QUEUE_CAPACITY);
    let receiver = Mutex::new(receiver);
    let workers = options.workers.clamp(1, MAX_WORKERS);
    thread::scope(|scope| {
        // Moved into this closure: when it returns, the sender is dropped, the
        // workers see the end of the queue and the scope can join them.
        let sender = sender;
        for _ in 0..workers {
            let spawned = thread::Builder::new()
                .name("rivqen-worker".to_owned())
                .stack_size(WORKER_STACK_BYTES)
                .spawn_scoped(scope, || {
                    work(&receiver, handler, &options.timeouts, &limiter);
                });
            if let Err(e) = spawned {
                return e;
            }
        }
        accept_loop(listener, &sender, &limiter)
    })
}

fn accept_loop(
    listener: &TcpListener,
    sender: &SyncSender<(TcpStream, IpAddr)>,
    limiter: &LogLimiter,
) -> io::Error {
    loop {
        match listener.accept() {
            Ok((stream, peer)) => {
                // Blocks while the queue is full.
                if sender.send((stream, peer.ip())).is_err() {
                    return io::Error::other("no worker is left");
                }
            }
            Err(e) => {
                limiter.log(
                    LogKind::io(LogEvent::AcceptFailed, e.kind()),
                    format_args!("accept failed: {:?}", e.kind()),
                );
                // A persistent error (for example no free file descriptor) must not
                // make the loop spin.
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn work<H: Handler>(
    receiver: &Mutex<Receiver<(TcpStream, IpAddr)>>,
    handler: &H,
    timeouts: &Timeouts,
    limiter: &LogLimiter,
) {
    loop {
        // The lock is released at the end of this statement, before the
        // connection is served. Nothing can panic while it is held.
        let next = receiver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv();
        let Ok((stream, peer)) = next else {
            return;
        };
        if guard(|| serve_connection(&stream, peer, handler, timeouts, limiter)).is_err() {
            limiter.log(
                LogKind::of(LogEvent::Panicked),
                format_args!("connection handler panicked; the server goes on"),
            );
        }
    }
}

/// The earlier of two deadlines; `None` is no deadline.
fn earliest(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

/// A reader with a deadline over all reads. Before the first byte the deadline
/// is the first-byte deadline; the first byte moves it to the head deadline.
struct Deadline<'a> {
    stream: &'a TcpStream,
    until: Option<Instant>,
    /// The time for the rest of the head after the first byte; `None` after the
    /// first byte.
    head: Option<Duration>,
    /// The deadline of the whole request; no phase goes past it.
    request_until: Option<Instant>,
}

impl Deadline<'_> {
    fn got_a_byte(&self) -> bool {
        self.head.is_none()
    }
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
        let n = stream.read(buf)?;
        if n > 0 {
            if let Some(head) = self.head.take() {
                self.until = earliest(Instant::now().checked_add(head), self.request_until);
            }
        }
        Ok(n)
    }
}

/// A writer with one deadline over all writes: a client that reads slowly cannot
/// hold a worker longer than [`Timeouts::write`].
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
        ReadError::TimedOut => Some(Outcome::noted(
            error_response(408, http_error_code(408)),
            "the request did not arrive in time",
        )),
        ReadError::Reject(r) => Some(Outcome::noted(
            error_response(r.status, http_error_code(r.status)),
            r.reason,
        )),
    }
}

/// The method of a log line: a method of [`LOG_METHODS`], else `-`.
fn log_method(method: &str) -> &'static str {
    LOG_METHODS
        .iter()
        .find(|m| **m == method)
        .copied()
        .unwrap_or("-")
}

/// The path of a log line.
fn log_path(head: &RequestHead, log_path: LogPath) -> &str {
    match log_path {
        LogPath::Hidden => "-",
        LogPath::Fixed(path) => path,
        // The target is visible ASCII (reader), so any cut is a char boundary.
        LogPath::Request => head.path().get(..MAX_LOG_PATH).unwrap_or(head.path()),
    }
}

/// What the log line says about the request.
struct Seen<'a> {
    method: &'static str,
    path: &'a str,
}

const UNSEEN: Seen<'static> = Seen {
    method: "-",
    path: "-",
};

fn log_request(seen: &Seen<'_>, status: u16, start: Instant, note: Option<&str>) {
    let (method, path) = (seen.method, seen.path);
    let ms = start.elapsed().as_millis();
    match note {
        Some(note) => log(format_args!("{method} {path} {status} {ms}ms ({note})")),
        None => log(format_args!("{method} {path} {status} {ms}ms")),
    }
}

fn serve_connection<H: Handler>(
    stream: &TcpStream,
    peer: IpAddr,
    handler: &H,
    timeouts: &Timeouts,
    limiter: &LogLimiter,
) {
    let start = Instant::now();
    let local_port = match stream.local_addr() {
        Ok(addr) => addr.port(),
        Err(e) => {
            limiter.log(
                LogKind::io(LogEvent::NoLocalAddress, e.kind()),
                format_args!("connection without a local address: {:?}", e.kind()),
            );
            return;
        }
    };
    let conn = Conn { peer, local_port };
    let request_until = start.checked_add(timeouts.read);
    let mut input = BufReader::with_capacity(
        8 * 1024,
        Deadline {
            stream,
            until: earliest(start.checked_add(timeouts.first_byte), request_until),
            head: Some(timeouts.head),
            request_until,
        },
    );
    let head = match read_head(&mut input) {
        Ok(head) => head,
        Err(error) => {
            match error {
                ReadError::Io(kind) => limiter.log(
                    LogKind::io(LogEvent::ReadFailed, kind),
                    format_args!(
                        "- - - {}ms (read failed: {kind:?})",
                        start.elapsed().as_millis()
                    ),
                ),
                ReadError::TimedOut if !input.get_ref().got_a_byte() => {
                    limiter.log(
                        LogKind::of(LogEvent::Idle),
                        format_args!(
                            "- - - {}ms (no request byte in time; closed)",
                            start.elapsed().as_millis()
                        ),
                    );
                    return;
                }
                _ => {}
            }
            if let Some(outcome) = read_failure(error) {
                answer(
                    stream, &mut input, &UNSEEN, &outcome, false, true, start, timeouts,
                );
            }
            return;
        }
    };
    // The body has the rest of the request deadline.
    input.get_mut().until = request_until;
    let method = log_method(&head.method);
    let (outcome, early) = match handler.route(&conn, &head) {
        Next::Respond(outcome) => (outcome, head.content_length > 0),
        Next::ReadBody(route) => match read_body(&mut input, head.content_length) {
            Ok(body) => (handler.respond(route, &conn, &head, &body), false),
            Err(error) => {
                let Some(outcome) = read_failure(error) else {
                    let seen = Seen { method, path: "-" };
                    log_request(&seen, 0, start, Some("the body read failed"));
                    return;
                };
                (outcome, true)
            }
        },
    };
    let seen = Seen {
        method,
        path: log_path(&head, outcome.log_path),
    };
    answer(
        stream,
        &mut input,
        &seen,
        &outcome,
        head.method == "HEAD",
        early,
        start,
        timeouts,
    );
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
    let deadline = input.get_mut();
    // Only the linger time counts now: a byte must not start a head deadline.
    deadline.head = None;
    deadline.until = Instant::now().checked_add(linger);
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::http::reader::Version;

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
        let o = read_failure(ReadError::TimedOut).unwrap();
        assert_eq!(o.response.status, 408);
        assert_eq!(o.log_path, LogPath::Hidden);
    }

    #[test]
    fn server_default_timeouts_and_options() {
        let t = Timeouts::default();
        assert_eq!(t.first_byte, Duration::from_secs(5));
        assert_eq!(t.head, Duration::from_secs(5));
        assert_eq!(t.read, Duration::from_secs(10));
        assert_eq!(t.write, Duration::from_secs(10));
        assert_eq!(t.linger, Duration::from_secs(2));
        let o = ServeOptions::default();
        assert_eq!((o.timeouts, o.workers), (t, 1));
    }

    #[test]
    fn server_earliest_deadline() {
        let now = Instant::now();
        let later = now + Duration::from_secs(1);
        assert_eq!(earliest(Some(now), Some(later)), Some(now));
        assert_eq!(earliest(Some(later), Some(now)), Some(now));
        assert_eq!(earliest(Some(now), None), Some(now));
        assert_eq!(earliest(None, Some(later)), Some(later));
        assert_eq!(earliest(None, None), None);
    }

    #[test]
    fn server_log_shows_only_known_methods_and_allowed_paths() {
        for m in LOG_METHODS {
            assert_eq!(log_method(m), m);
        }
        for m in ["get", "BREW", "GETX", "", "secret-token"] {
            assert_eq!(log_method(m), "-");
        }
        let head = |target: &str| RequestHead {
            method: "GET".to_owned(),
            target: target.to_owned(),
            version: Version::Http11,
            fields: Vec::new(),
            content_length: 0,
        };
        let h = head("/a/b?token=1");
        assert_eq!(log_path(&h, LogPath::Hidden), "-");
        assert_eq!(log_path(&h, LogPath::Fixed("/fixed")), "/fixed");
        assert_eq!(log_path(&h, LogPath::Request), "/a/b");
        let long = format!("/{}", "a".repeat(400));
        let h = head(&long);
        assert_eq!(log_path(&h, LogPath::Request).len(), MAX_LOG_PATH);
    }
}
