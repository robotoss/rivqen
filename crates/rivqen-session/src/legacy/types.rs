// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The contract of the legacy session (WP-08 S2): what the adapter gives
//! and what it must do.
//!
//! The adapter calls the engine for one engine from one logical thread at a
//! time (core API section 2). It executes the effects of one call in the
//! order of the list. An effect that needs an answer (`Lookup`,
//! `ReadCookie`, `Send`) is answered by one later input with the same
//! session id (and the same exchange id for `Send`). An input of a stopped
//! session, of an older refresh generation or of an unknown exchange gives
//! no effect, never an error (FSM-02, CL-34, CL-35).
//!
//! `Debug` of every type writes no URL, field value, cookie, body byte or
//! digest: only ids, codes, counts and lengths.

use core::fmt;

use rivqen_cache::key::Partition;
use rivqen_cache::txn::{CacheTransaction, Epoch, StoredEntry, TxnOp};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::Secret;
use rivqen_model::time::{Instant, UnixTime};
use rivqen_proto_legacy::body::BodyLimits;
use rivqen_proto_legacy::message::ResultCode;
use rivqen_template::limits::TemplateLimits;
use rivqen_url::ParsedUrl;

/// The diff age of CL-28: a message with a diff older than this at its
/// delivery is not sent.
pub const DIFF_MAX_AGE_MS: u64 = 30_000;

/// The default `http` period of CL-19 (6 hours).
pub const DEFAULT_UNAVAILABLE_TIME_MS: u64 = 6 * 60 * 60 * 1000;

/// The time of one call: the monotonic time for the timers and the diff
/// age (CL-04, CL-28), the wall time for the cache (store time, response
/// time, the end of a disabled period).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    /// Milliseconds on the monotonic clock of the adapter.
    pub mono: Instant,
    /// Seconds since the Unix epoch.
    pub wall: UnixTime,
}

/// How the session gives the document to the web view (session FSM
/// section 3; legacy client CL-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    /// `load_data` of a complete document at `ClientReady`, else `LoadUrl`.
    Quick,
    /// Always `LoadUrl`; the session answers the intercepted request.
    Standard,
}

/// The configuration of one engine (CL-03). The limits come from the
/// checked `EngineConfig` (DL-163).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyConfig {
    /// Quick or Standard.
    pub presentation: Presentation,
    /// The value of `accept-diff` (CL-09); default `true`.
    pub accept_diff: bool,
    /// The app allows the commit of a credentialed exchange (CL-25,
    /// H-150 for RQP); default `false`.
    pub authenticated_cache: bool,
    /// The `http` period of CL-19 in milliseconds.
    pub unavailable_time_ms: u64,
    /// The limits of the split, the rebuild and the cached page.
    pub template: TemplateLimits,
    /// The limits of a data body.
    pub body: BodyLimits,
}

/// The key of the keyed digests of the `Vary` values (identity section 1,
/// DL-127, H-147): HMAC-SHA256 with a key of the key port of the adapter.
/// `Debug` writes no byte of it.
#[derive(Clone)]
pub struct VaryKey(pub Secret<[u8; 32]>);

impl fmt::Debug for VaryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VaryKey(..)")
    }
}

/// What the adapter knows when the web view starts a navigation that the
/// session may serve.
#[derive(Clone)]
pub struct OpenContext {
    /// The URL of the navigation, parsed by `rivqen_url::parse`; an URL
    /// that does not parse gets no session (the adapter loads it normally).
    pub url: ParsedUrl,
    /// The partition of the account (the adapter computes it, identity
    /// section 1). The account `null` has a partition of its own.
    pub partition: PartitionId,
    /// The account is `null` (CL-05): a commit is never allowed (CL-25).
    pub anonymous: bool,
    /// The network is valid (CL-10).
    pub network_valid: bool,
    /// The request field lines that the platform adds to the request
    /// (for example `accept-encoding`, `accept-language`): the values of
    /// the `Vary` names come from them, from the cookie and from the
    /// session fields (CL-12).
    pub platform_fields: Vec<FieldLine>,
    /// The key of the `Vary` digests.
    pub vary_key: VaryKey,
}

impl fmt::Debug for OpenContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenContext")
            .field("url", &self.url)
            .field("partition", &self.partition)
            .field("anonymous", &self.anonymous)
            .field("network_valid", &self.network_valid)
            .field("platform_fields", &self.platform_fields.len())
            .finish_non_exhaustive()
    }
}

/// The id of a session of one engine. Ids are never used again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u64);

/// The id of one request of a session (one per `Send`). A refresh sends a
/// new exchange; the answers of an older exchange are ignored (CL-35).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExchangeId(pub u64);

/// The id of one intercepted main-frame request of the web view (CL-20
/// item 4). The adapter makes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InterceptId(pub u64);

/// Why a request failed (CL-13 item 1, CL-18, CL-33).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportError {
    /// The connection was refused.
    Refused,
    /// A connection error other than TLS.
    Connection,
    /// A TLS error.
    Tls,
    /// A read error (before the head: CL-18; after it: CL-33).
    Read,
}

/// The answer of the adapter to [`Effect::Lookup`].
#[derive(Clone, PartialEq, Eq)]
pub struct LookupResult {
    /// The entry under the key, if one exists (one entry per URL, H-160).
    /// The session decides whether it is visible (CL-12, CL-40).
    pub entry: Option<StoredEntry>,
    /// The key is disabled at this time (CL-08, CL-19).
    pub disabled: bool,
    /// The epoch of the partition of the session at this read (DL-205,
    /// DL-220): every write of the session names it.
    pub partition_epoch: Epoch,
    /// The epoch of the site clear at this read (CL-45).
    pub site_epoch: Epoch,
}

impl fmt::Debug for LookupResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LookupResult")
            .field("entry", &self.entry.as_ref().map(|e| e.version))
            .field("disabled", &self.disabled)
            .field("partition_epoch", &self.partition_epoch)
            .field("site_epoch", &self.site_epoch)
            .finish()
    }
}

/// One input of a session.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Input {
    /// The answer to [`Effect::Lookup`].
    Lookup(Box<LookupResult>),
    /// The answer to [`Effect::ReadCookie`]: the cookie of the session URL
    /// (`None` or empty: no `cookie` field, CL-11).
    Cookie(Option<String>),
    /// The status and the field lines of a response (CL-01: the session
    /// acts on the head first).
    ResponseHead {
        /// The exchange of the response.
        exchange: ExchangeId,
        /// The status code.
        status: u16,
        /// The response field lines in order (decoded body: no
        /// `Content-Encoding` applies to the body bytes below).
        fields: Vec<FieldLine>,
    },
    /// Body bytes of a response, in order.
    BodyChunk {
        /// The exchange of the response.
        exchange: ExchangeId,
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// The body ended without an error.
    BodyEnd {
        /// The exchange of the response.
        exchange: ExchangeId,
    },
    /// The request failed: before the head (CL-18) or after it (CL-33).
    TransportFailed {
        /// The exchange of the request.
        exchange: ExchangeId,
        /// The kind of failure.
        error: TransportError,
    },
    /// The web view client is bound to the session (step `bind_client`).
    BindClient,
    /// The web view is ready (step `client_ready`, CL-20 items 2, 3, 7).
    ClientReady,
    /// The web view asks for the main document (step `request_resource`,
    /// CL-20 item 4).
    Intercept(InterceptId),
    /// The page asks for its message (step `get_diff`, CL-23 item 2).
    GetDiff,
    /// The page finished loading (CL-36: no effect).
    PageFinished,
    /// The app reloads the session (CL-35).
    Refresh,
    /// The app destroys the session (CL-34).
    Destroy,
    /// A deadline of an [`Effect::ScheduleTimer`] passed (FSM-05).
    Tick,
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lookup(result) => f.debug_tuple("Lookup").field(result).finish(),
            Self::Cookie(cookie) => f
                .debug_tuple("Cookie")
                .field(&cookie.as_ref().map(String::len))
                .finish(),
            Self::ResponseHead {
                exchange,
                status,
                fields,
            } => f
                .debug_struct("ResponseHead")
                .field("exchange", exchange)
                .field("status", status)
                .field("fields", &fields.len())
                .finish(),
            Self::BodyChunk { exchange, bytes } => f
                .debug_struct("BodyChunk")
                .field("exchange", exchange)
                .field("len", &bytes.len())
                .finish(),
            Self::BodyEnd { exchange } => f
                .debug_struct("BodyEnd")
                .field("exchange", exchange)
                .finish(),
            Self::TransportFailed { exchange, error } => f
                .debug_struct("TransportFailed")
                .field("exchange", exchange)
                .field("error", error)
                .finish(),
            Self::BindClient => f.write_str("BindClient"),
            Self::ClientReady => f.write_str("ClientReady"),
            Self::Intercept(id) => f.debug_tuple("Intercept").field(id).finish(),
            Self::GetDiff => f.write_str("GetDiff"),
            Self::PageFinished => f.write_str("PageFinished"),
            Self::Refresh => f.write_str("Refresh"),
            Self::Destroy => f.write_str("Destroy"),
            Self::Tick => f.write_str("Tick"),
        }
    }
}

/// The request of a session (CL-09).
#[derive(Clone, PartialEq, Eq)]
pub struct RequestSpec {
    /// The id that the answers name.
    pub exchange: ExchangeId,
    /// The request target: path and query of the session URL.
    pub target: String,
    /// The legacy field lines of CL-09 in order (`accept-diff`, then the
    /// validators of a visible entry, then `cookie`). The adapter adds its
    /// platform fields and its transport fields; it does not follow a
    /// redirect (CL-44).
    pub fields: Vec<FieldLine>,
}

impl fmt::Debug for RequestSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestSpec")
            .field("exchange", &self.exchange)
            .field("target_len", &self.target.len())
            .field("fields", &self.fields.len())
            .finish()
    }
}

/// A document for the web view (CL-20, CL-21): the status, the page fields
/// and the decoded body bytes (UTF-8; the adapter gives them with an
/// explicit `utf-8` encoding, section 9.12).
#[derive(Clone, PartialEq, Eq)]
pub struct DocumentHead {
    /// The status code (`200`).
    pub status: u16,
    /// The page fields of CL-21.
    pub fields: Vec<FieldLine>,
}

impl fmt::Debug for DocumentHead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocumentHead")
            .field("status", &self.status)
            .field("fields", &self.fields.len())
            .finish()
    }
}

/// One effect of a session. The adapter executes the effects of one call in
/// order.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Effect {
    /// Read the entry under `key` in `partition`, the disabled state of the
    /// key and the two epochs; answer with [`Input::Lookup`].
    Lookup {
        /// The partition of the session.
        partition: Partition,
        /// The key of one entry per URL (H-160).
        key: CacheKey,
    },
    /// Read the cookie of the session URL now (CL-11 item 1); answer with
    /// [`Input::Cookie`]. The request follows it.
    ReadCookie,
    /// Send the request (CL-09) and report the response.
    Send(RequestSpec),
    /// Give these `Set-Cookie` and `Set-Cookie2` values, in order, to the
    /// cookie store of the platform for the session URL (CL-11 item 2).
    StoreCookies(Vec<String>),
    /// The web view loads the session URL itself (a normal load, FSM-06).
    LoadUrl,
    /// Quick: give this complete document to the web view with the session
    /// URL as its base URL (CL-20 item 2).
    LoadData {
        /// The status and the page fields.
        head: DocumentHead,
        /// The document bytes.
        body: Vec<u8>,
    },
    /// Answer the intercepted request with no document: the web view loads
    /// it from the network (CL-20 item 5).
    AnswerNull(InterceptId),
    /// Answer the intercepted request with a document; its bytes follow in
    /// [`Effect::StreamChunk`] and end with [`Effect::StreamEnd`]. A
    /// complete document is one head, its chunks and an end with
    /// `complete: true` in the same list.
    AnswerDocument {
        /// The intercepted request.
        intercept: InterceptId,
        /// The status and the page fields.
        head: DocumentHead,
    },
    /// Bytes of an answered document.
    StreamChunk {
        /// The intercepted request.
        intercept: InterceptId,
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// The end of an answered document; `complete: false` when the
    /// response body ended with an error or the session stopped (CL-33,
    /// CL-34, CL-06).
    StreamEnd {
        /// The intercepted request.
        intercept: InterceptId,
        /// The document is complete.
        complete: bool,
    },
    /// Give this message to the page callback (CL-23, CL-24).
    Message(String),
    /// The result of the session is set (CL-22); the adapter reports it to
    /// the app. The last one before [`Effect::Closed`] is the result of the
    /// session.
    Result {
        /// The source code.
        src: ResultCode,
        /// The final code.
        final_code: ResultCode,
    },
    /// Execute this cache transaction (a commit, a removal, a field update,
    /// a disable or a partition revoke). The precondition names the request
    /// partition of the session (DL-220). A transaction that the store
    /// refuses changes nothing; the session does not wait for the outcome.
    Cache(Box<CacheTransaction>),
    /// Send [`Input::Tick`] at this time unless a later effect of the
    /// session replaces it (FSM-05).
    ScheduleTimer(Instant),
    /// The session did not open (CL-07, CL-08) or is closed (CL-34,
    /// CL-06): no later effect names it. The web view loads normally when
    /// the session did not open.
    Closed,
}

impl fmt::Debug for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lookup { partition, key } => f
                .debug_struct("Lookup")
                .field("partition", partition)
                .field("key", key)
                .finish(),
            Self::ReadCookie => f.write_str("ReadCookie"),
            Self::Send(spec) => f.debug_tuple("Send").field(spec).finish(),
            Self::StoreCookies(values) => {
                f.debug_tuple("StoreCookies").field(&values.len()).finish()
            }
            Self::LoadUrl => f.write_str("LoadUrl"),
            Self::LoadData { head, body } => f
                .debug_struct("LoadData")
                .field("head", head)
                .field("len", &body.len())
                .finish(),
            Self::AnswerNull(id) => f.debug_tuple("AnswerNull").field(id).finish(),
            Self::AnswerDocument { intercept, head } => f
                .debug_struct("AnswerDocument")
                .field("intercept", intercept)
                .field("head", head)
                .finish(),
            Self::StreamChunk { intercept, bytes } => f
                .debug_struct("StreamChunk")
                .field("intercept", intercept)
                .field("len", &bytes.len())
                .finish(),
            Self::StreamEnd {
                intercept,
                complete,
            } => f
                .debug_struct("StreamEnd")
                .field("intercept", intercept)
                .field("complete", complete)
                .finish(),
            Self::Message(text) => f.debug_tuple("Message").field(&text.len()).finish(),
            Self::Result { src, final_code } => f
                .debug_struct("Result")
                .field("src", src)
                .field("final_code", final_code)
                .finish(),
            Self::Cache(txn) => f
                .debug_tuple("Cache")
                .field(&format_args!("{}", op_kind(&txn.op)))
                .finish(),
            Self::ScheduleTimer(at) => f.debug_tuple("ScheduleTimer").field(at).finish(),
            Self::Closed => f.write_str("Closed"),
        }
    }
}

/// The name of a transaction kind, without its content.
const fn op_kind(op: &TxnOp) -> &'static str {
    match op {
        TxnOp::Replace { .. } => "Replace",
        TxnOp::UpdateFields { .. } => "UpdateFields",
        TxnOp::Remove { .. } => "Remove",
        TxnOp::DisableKey { .. } => "DisableKey",
        TxnOp::RevokePartition { .. } => "RevokePartition",
        TxnOp::ClearSite { .. } => "ClearSite",
        _ => "Other",
    }
}
