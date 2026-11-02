// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The interface between the server loop and a mode of the server (WP-03 S2).
//!
//! The server loop ([`crate::http::server`]) reads a request head, asks the
//! [`Handler`] what to do ([`Handler::route`]), reads the body when the handler
//! asks for it, and writes the [`Outcome`] of [`Handler::respond`]. The
//! conformance handler does no I/O; the site handler reads one file per request.
//! The workers share one handler, so a handler keeps its state behind a lock
//! ([`Sync`]).
//!
//! The conformance mode ([`crate::control::Server`]) is one handler; the site mode
//! is another.

use std::net::IpAddr;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::http::reader::RequestHead;
use crate::model::{Body, Field, Response};

/// The connection of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conn {
    /// The peer address.
    pub peer: IpAddr,
    /// The local port of the connection (the port in an allowed `Host`).
    pub local_port: u16,
}

/// What the log line of a request shows as its path (security review S1 L5).
///
/// The default is [`LogPath::Hidden`]: a path is an input value, so a handler
/// shows it only when it knows that the path is safe to log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogPath {
    /// `-`.
    #[default]
    Hidden,
    /// A fixed path of the handler (for example a control path).
    Fixed(&'static str),
    /// The path of the request target, without the query (for example a file
    /// of the site that the request maps to).
    Request,
}

/// An answer, a fixed note and the path of the log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The answer.
    pub response: Response,
    /// Why, when it is not the normal answer. A fixed text: never an input value.
    pub note: Option<&'static str>,
    /// What the log line shows as the path.
    pub log_path: LogPath,
}

impl Outcome {
    /// An answer without a note; the log path is hidden.
    #[must_use]
    pub fn plain(response: Response) -> Self {
        Self {
            response,
            note: None,
            log_path: LogPath::Hidden,
        }
    }

    /// An answer with a fixed note; the log path is hidden.
    #[must_use]
    pub fn noted(response: Response, note: &'static str) -> Self {
        Self {
            response,
            note: Some(note),
            log_path: LogPath::Hidden,
        }
    }

    /// The same answer with another log path.
    #[must_use]
    pub fn with_log_path(self, log_path: LogPath) -> Self {
        Self { log_path, ..self }
    }
}

/// The next step of a request after its head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next<R> {
    /// Answer now; the body is not read.
    Respond(Outcome),
    /// Read the body (at most `Content-Length` bytes, never more than
    /// [`crate::http::reader::MAX_BODY_BYTES`]), then call [`Handler::respond`]
    /// with this route.
    ReadBody(R),
}

/// A mode of the server: what to answer to a request.
///
/// Both methods must not panic; the server loop catches a panic anyway, logs it
/// without its message and closes the connection.
pub trait Handler: Sync {
    /// What [`Handler::respond`] needs to know from [`Handler::route`].
    type Route;

    /// The checks that need only the head. A handler that reads a body refuses
    /// a `Content-Length` over its limit here (`413`).
    fn route(&self, conn: &Conn, head: &RequestHead) -> Next<Self::Route>;

    /// The answer after the body was read.
    fn respond(&self, route: Self::Route, conn: &Conn, head: &RequestHead, body: &[u8]) -> Outcome;
}

/// A JSON answer `{"error": code}`.
#[must_use]
pub fn error_response(status: u16, code: &str) -> Response {
    Response {
        status,
        fields: vec![Field::new("Content-Type", "application/json")],
        body: Body::Bytes(
            serde_json::json!({ "error": code })
                .to_string()
                .into_bytes(),
        ),
    }
}

/// Run `f` and catch a panic: the last guard around an engine and around a
/// connection (ERR-08). The panic hook of the binary writes only the location.
///
/// # Errors
/// `Err(())` when `f` panicked.
#[allow(clippy::result_unit_err)]
pub fn guard<T, F: FnOnce() -> T>(f: F) -> Result<T, ()> {
    // AssertUnwindSafe: the engines are pure functions of shared references, and
    // the state of a handler is replaced in one step under its lock; a panic
    // leaves no state half-changed.
    catch_unwind(AssertUnwindSafe(f)).map_err(drop)
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn handler_outcome_constructors_hide_the_path() {
        let r = error_response(404, "NOT_FOUND");
        let o = Outcome::plain(r.clone());
        assert_eq!((o.note, o.log_path), (None, LogPath::Hidden));
        let o = Outcome::noted(r.clone(), "why");
        assert_eq!((o.note, o.log_path), (Some("why"), LogPath::Hidden));
        let o = o.with_log_path(LogPath::Request);
        assert_eq!((o.note, o.log_path), (Some("why"), LogPath::Request));
        assert_eq!(o.response, r);
        assert_eq!(LogPath::default(), LogPath::Hidden);
    }

    #[test]
    fn handler_error_bodies_are_json() {
        let r = error_response(503, "NO_CASE");
        assert_eq!(r.status, 503);
        assert_eq!(r.body, Body::Bytes(br#"{"error":"NO_CASE"}"#.to_vec()));
        assert_eq!(
            r.fields,
            vec![Field::new("Content-Type", "application/json")]
        );
    }

    #[test]
    fn handler_guard_catches_a_panic() {
        assert_eq!(guard(|| 7), Ok(7));
        let caught = guard(|| -> u8 { panic!("engine bug") });
        assert_eq!(caught, Err(()));
        assert_eq!(guard(|| "ok"), Ok("ok"));
    }
}
