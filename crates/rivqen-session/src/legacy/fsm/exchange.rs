// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! One request of a session and its response (CL-01: the head first, then
//! the body), with the streams of the intercepted requests that receive the
//! body (CL-20 item 4).
//!
//! # Bounds
//!
//! The body is buffered only when [`Exchange::buffer_up_to`] set a limit;
//! [`Exchange::push`] refuses a chunk that would pass it. The UTF-8 test is
//! incremental: each call tests only the bytes after the valid prefix, so
//! the work of a body is linear in its length. A stream gets only bytes of
//! the valid prefix while the body arrives, so no stream holds bytes that
//! fail CL-16; at a failed end it gets the rest of the received bytes and
//! `complete: false`.

use rivqen_model::http::FieldLine;
use rivqen_model::time::UnixTime;

use crate::legacy::types::{Effect, ExchangeId, InterceptId};

/// The response head of an exchange.
#[derive(Clone)]
pub(crate) struct Head {
    /// The response field lines in order.
    pub(crate) fields: Vec<FieldLine>,
    /// When the head arrived (wall time).
    pub(crate) time: UnixTime,
}

/// How the body ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    /// The body is still open.
    Open,
    /// The body ended without an error.
    Complete,
    /// The body ended with an error, over a limit, or the session stopped
    /// reading it (CL-33).
    Failed,
}

/// An intercepted request that receives the body as a stream.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Stream {
    /// The intercepted request.
    pub(crate) intercept: InterceptId,
    /// The bytes of the body already given.
    pub(crate) sent: usize,
}

/// The result of [`Exchange::push`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Push {
    /// The bytes are in the buffer.
    Stored,
    /// The body is not buffered: the bytes are dropped.
    Ignored,
    /// The bytes would pass the limit; nothing is stored.
    OverLimit,
    /// The body is not valid UTF-8 (CL-16).
    NotUtf8,
}

/// One request of a session.
pub(crate) struct Exchange {
    /// The id of the request.
    pub(crate) id: ExchangeId,
    /// The refresh generation of the request (CL-35).
    pub(crate) generation: u64,
    /// The response head, after it arrived.
    pub(crate) head: Option<Head>,
    /// The buffered body bytes.
    pub(crate) body: Vec<u8>,
    /// The length of the prefix of `body` that is valid UTF-8 and complete.
    pub(crate) valid: usize,
    /// The largest body that is buffered; `None`: the body is not buffered.
    limit: Option<usize>,
    /// How the body ended.
    pub(crate) end: End,
    /// The intercepted requests that receive the body.
    pub(crate) streams: Vec<Stream>,
}

impl Exchange {
    /// A request with no response yet.
    pub(crate) const fn new(id: ExchangeId, generation: u64) -> Self {
        Self {
            id,
            generation,
            head: None,
            body: Vec::new(),
            valid: 0,
            limit: None,
            end: End::Open,
            streams: Vec::new(),
        }
    }

    /// Buffer the body, at most `limit` bytes.
    pub(crate) const fn buffer_up_to(&mut self, limit: usize) {
        self.limit = Some(limit);
    }

    /// The body ended without an error.
    pub(crate) fn is_complete(&self) -> bool {
        self.end == End::Complete
    }

    /// The body ended (with or without an error).
    pub(crate) fn has_ended(&self) -> bool {
        self.end != End::Open
    }

    /// The whole buffered body is valid UTF-8 (CL-16).
    pub(crate) const fn is_text(&self) -> bool {
        self.valid == self.body.len()
    }

    /// Append body bytes and extend the valid UTF-8 prefix.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Push {
        let Some(limit) = self.limit else {
            return Push::Ignored;
        };
        if self
            .body
            .len()
            .checked_add(bytes.len())
            .is_none_or(|total| total > limit)
        {
            return Push::OverLimit;
        }
        self.body.extend_from_slice(bytes);
        let rest = self.body.get(self.valid..).unwrap_or_default();
        match core::str::from_utf8(rest) {
            Ok(_) => {
                self.valid = self.body.len();
                Push::Stored
            }
            // A sequence that the next chunk can complete.
            Err(error) if error.error_len().is_none() => {
                self.valid = self.valid.saturating_add(error.valid_up_to());
                Push::Stored
            }
            Err(_) => Push::NotUtf8,
        }
    }

    /// Give each stream the valid bytes that it has not got yet.
    pub(crate) fn forward(&mut self, out: &mut Vec<Effect>) {
        let valid = self.valid;
        for stream in &mut self.streams {
            if let Some(bytes) = self.body.get(stream.sent..valid)
                && !bytes.is_empty()
            {
                out.push(Effect::StreamChunk {
                    intercept: stream.intercept,
                    bytes: bytes.to_vec(),
                });
                stream.sent = valid;
            }
        }
    }

    /// End every stream: the rest of the received bytes, then the end with
    /// `complete` (CL-33, CL-34, CL-06).
    pub(crate) fn finish_streams(&mut self, complete: bool, out: &mut Vec<Effect>) {
        for stream in core::mem::take(&mut self.streams) {
            if let Some(bytes) = self.body.get(stream.sent..)
                && !bytes.is_empty()
            {
                out.push(Effect::StreamChunk {
                    intercept: stream.intercept,
                    bytes: bytes.to_vec(),
                });
            }
            out.push(Effect::StreamEnd {
                intercept: stream.intercept,
                complete,
            });
        }
    }

    /// Start a stream for `intercept` with the valid bytes so far.
    pub(crate) fn open_stream(&mut self, intercept: InterceptId, out: &mut Vec<Effect>) {
        if let Some(bytes) = self.body.get(..self.valid)
            && !bytes.is_empty()
        {
            out.push(Effect::StreamChunk {
                intercept,
                bytes: bytes.to_vec(),
            });
        }
        self.streams.push(Stream {
            intercept,
            sent: self.valid,
        });
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::{End, Exchange, Push};
    use crate::legacy::types::{Effect, ExchangeId, InterceptId};

    fn buffered(limit: usize) -> Exchange {
        let mut exchange = Exchange::new(ExchangeId(1), 0);
        exchange.buffer_up_to(limit);
        exchange
    }

    #[test]
    fn push_keeps_an_incomplete_sequence_out_of_the_valid_prefix() {
        let mut exchange = buffered(16);
        assert_eq!(exchange.push(b"a\xc3"), Push::Stored);
        assert_eq!(exchange.valid, 1);
        assert!(!exchange.is_text());
        assert_eq!(exchange.push(b"\xa9b"), Push::Stored);
        assert_eq!(exchange.valid, 4);
        assert!(exchange.is_text());
    }

    #[test]
    fn push_finds_an_invalid_byte() {
        let mut exchange = buffered(16);
        assert_eq!(exchange.push(b"ab"), Push::Stored);
        assert_eq!(exchange.push(b"\xff"), Push::NotUtf8);
    }

    #[test]
    fn push_refuses_bytes_over_the_limit_and_ignores_without_a_limit() {
        let mut exchange = buffered(3);
        assert_eq!(exchange.push(b"abc"), Push::Stored);
        assert_eq!(exchange.push(b"d"), Push::OverLimit);
        assert_eq!(exchange.body, b"abc");
        let mut plain = Exchange::new(ExchangeId(2), 0);
        assert_eq!(plain.push(b"abc"), Push::Ignored);
        assert_eq!(plain.body.len(), 0);
    }

    #[test]
    fn streams_get_only_valid_bytes_then_the_rest_at_the_end() {
        let mut exchange = buffered(16);
        let mut out = Vec::new();
        assert_eq!(exchange.push(b"a"), Push::Stored);
        exchange.open_stream(InterceptId(7), &mut out);
        assert_eq!(exchange.push(b"b\xc3"), Push::Stored);
        exchange.forward(&mut out);
        exchange.end = End::Failed;
        exchange.finish_streams(false, &mut out);
        let chunks: Vec<&[u8]> = out
            .iter()
            .filter_map(|effect| match effect {
                Effect::StreamChunk { bytes, .. } => Some(bytes.as_slice()),
                _ => None,
            })
            .collect();
        assert_eq!(chunks, vec![&b"a"[..], &b"b"[..], &b"\xc3"[..]]);
        assert_eq!(
            out.last(),
            Some(&Effect::StreamEnd {
                intercept: InterceptId(7),
                complete: false
            })
        );
        assert_eq!(exchange.streams.len(), 0);
    }
}
