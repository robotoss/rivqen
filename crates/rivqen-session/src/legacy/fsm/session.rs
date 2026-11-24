// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! One legacy session: its state, the request flow (CL-08 to CL-12), the
//! routing of the inputs, the refresh (CL-35) and the stop (CL-06, CL-34).
//! The outcomes are in [`super::outcome`], the delivery to the web view in
//! [`super::deliver`].
//!
//! # State
//!
//! The session is a product of small parts, not one enum: the request flow
//! ([`Step`]), the current document and its delivery (CL-20), the result
//! (CL-22) and the exchanges. Each part changes only by the rules that name
//! it. Session FSM section 5 maps these parts to the 13 states of section 1.

use core::fmt;

use rivqen_cache::key::Partition;
use rivqen_cache::txn::{CacheTransaction, Epoch, Expect, StoredEntry};
use rivqen_diff::legacy::Rebuilt;
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::time::{Instant, UnixTime};
use rivqen_proto_legacy::message::ResultCode;
use rivqen_proto_legacy::request::{EntryValidators, LegacyRequest, request_fields};
use rivqen_url::ParsedUrl;

use super::exchange::{End, Exchange, Head};
use super::head::Kind;
use super::port::RecordPort;
use crate::legacy::record::{self, LegacyRecord, NotVisible, RequestValues, WriteBasis};
use crate::legacy::types::{
    Effect, ExchangeId, Input, InterceptId, LegacyConfig, LookupResult, Now, OpenContext,
    RequestSpec, TransportError, VaryKey,
};

/// The most exchanges that a session keeps: the current one and the older
/// ones that a document or a stream still reads. Over this number the
/// oldest one ends (its streams end incomplete).
pub(crate) const MAX_EXCHANGES: usize = 4;

/// What a call of the session needs from its engine.
pub(crate) struct Env<'a> {
    /// The configuration of the engine.
    pub(crate) config: &'a LegacyConfig,
    /// The record functions.
    pub(crate) port: &'a RecordPort,
    /// The time of the call.
    pub(crate) now: Now,
}

/// What the engine does with the session after an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fate {
    /// The session stays.
    Stays,
    /// The session stopped as CL-06 and gave [`Effect::Closed`]: it gives
    /// no more effects, but it holds its key until its `Destroy` (CL-07).
    Stopped,
    /// The session gave [`Effect::Closed`] and is gone (it did not open, or
    /// it was destroyed): the engine removes it.
    Gone,
}

/// The request flow of a generation (CL-08 to CL-11).
pub(super) enum Step {
    /// Waiting for the answer to [`Effect::Lookup`]. `opening`: the first
    /// lookup of the session, where a disabled key closes it (CL-08).
    Lookup {
        /// The first lookup of the session.
        opening: bool,
    },
    /// Waiting for the answer to [`Effect::ReadCookie`] (CL-11 item 1),
    /// with the entry of the lookup.
    Cookie {
        /// The entry under the key, if any.
        stored: Option<Box<StoredEntry>>,
    },
    /// The request is sent, or the generation decided without one.
    Done,
}

/// A document (CL-20 item 1).
pub(super) struct Doc {
    /// The identity of the document in its session (CL-22: "the first
    /// document that it received is the current document").
    pub(super) id: u64,
    /// The cached page of the visible entry.
    pub(super) cached: bool,
    /// The bytes.
    pub(super) source: Source,
    /// The page fields (CL-21).
    pub(super) fields: Vec<FieldLine>,
}

/// Where the bytes of a document are.
pub(super) enum Source {
    /// A complete document: a cached page or a rebuilt page.
    Bytes(Vec<u8>),
    /// The body of the response of an exchange (complete when it ended
    /// without an error).
    Response(ExchangeId),
}

/// The first document that the web view received (CL-22).
#[derive(Debug, Clone, Copy)]
pub(super) struct Delivery {
    /// The document; `None` after a refresh with no document (CL-35).
    pub(super) doc: Option<u64>,
    /// It was complete at the delivery.
    pub(super) complete: bool,
}

/// The result of a generation (CL-22) and what its message needs (CL-24).
pub(super) struct Outcome {
    /// The source code.
    pub(super) src: ResultCode,
    /// The final code.
    pub(super) final_code: ResultCode,
    /// The checked rebuild of a data update whose diff goes to the page
    /// (CL-27): the only source of a diff (DL-226).
    pub(super) diff: Option<Box<Rebuilt>>,
    /// The field lines of the response that set the result.
    pub(super) fields: Vec<FieldLine>,
    /// When the result was set (CL-28).
    pub(super) at: Instant,
    /// CL-28: the diff is too old; no message follows.
    pub(super) expired: bool,
    /// The diff reached the page once.
    pub(super) diff_sent: bool,
}

/// One legacy session (CL-05 to CL-37).
#[allow(clippy::struct_excessive_bools)] // one flag per condition of CL-20 and CL-22
pub(crate) struct Session {
    // --- identity (CL-05) ---
    /// The partition of the request.
    pub(crate) partition: PartitionId,
    /// The canonical URL (CL-07 compares it).
    pub(crate) url_id: String,
    /// The key of the entry (H-160).
    pub(super) key: CacheKey,
    /// The request target: path and query (CL-09).
    target: String,
    /// The host of the URL (the site of a commit).
    pub(super) host: String,
    /// The account is `null` (CL-25).
    pub(super) anonymous: bool,
    /// The network was valid at the open (CL-10).
    network_valid: bool,
    /// The fields that the platform adds to the request (CL-12).
    pub(super) platform_fields: Vec<FieldLine>,
    /// The key of the `Vary` digests.
    pub(super) vary_key: VaryKey,

    // --- request flow ---
    pub(super) step: Step,
    /// The session stopped (CL-06, CL-34): it gives no effect.
    pub(crate) stopped: bool,
    /// The epochs of the opening lookup (DL-220, DL-229 (a)). A later
    /// lookup with other epochs stops the session; these never change.
    partition_epoch: Epoch,
    site_epoch: Epoch,
    /// What the session knows of the stored entry of the key.
    pub(super) seen: Expect,
    /// The visible entry of the generation (CL-12, CL-40).
    pub(super) entry: Option<LegacyRecord>,
    /// The visible entry is still in the store as the session saw it.
    pub(super) entry_stored: bool,
    /// The legacy request fields of the generation (CL-09).
    pub(super) request_fields: Vec<FieldLine>,
    /// When the request was sent.
    pub(super) request_time: UnixTime,
    /// The exchanges: the current one and older ones still read.
    pub(super) exchanges: Vec<Exchange>,
    /// The exchange of the current generation.
    pub(super) current: Option<ExchangeId>,
    /// The number of requests sent: the last exchange id.
    sent: u64,

    // --- generation (CL-35) ---
    pub(super) generation: u64,
    pub(super) refreshed: bool,
    pub(super) kind: Option<Kind>,

    // --- web view (CL-20) ---
    pub(super) bound: bool,
    pub(super) ready: bool,
    pub(super) presented: bool,
    /// The next intercept gets the current document (CL-20 items 4, 5).
    pub(super) armed: bool,

    // --- document (CL-20) ---
    pub(super) doc: Option<Doc>,
    /// A normal load ended the current document (CL-20 item 6).
    pub(super) no_doc: bool,
    /// The outcome is known: a waiting intercept is answered.
    pub(super) decided: bool,
    /// A template change after the delivery of the cached page (CL-31).
    pub(super) pending_new: Option<Doc>,
    /// The intercepts that wait for the head or for the first byte.
    pub(super) waiting: Vec<InterceptId>,
    pub(super) first_delivery: Option<Delivery>,
    pub(super) cached_given: bool,
    next_doc: u64,

    // --- result (CL-22 to CL-24) ---
    pub(super) result: Option<Outcome>,
    /// A page callback is registered (CL-23 item 2).
    pub(super) callback: bool,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("stopped", &self.stopped)
            .field("generation", &self.generation)
            .field("kind", &self.kind)
            .field("exchanges", &self.exchanges.len())
            .field("waiting", &self.waiting.len())
            .finish_non_exhaustive()
    }
}

/// The host of a serialized origin (`scheme://host[:port]`).
fn host_of(origin: &str) -> String {
    let rest = origin.split_once("://").map_or(origin, |(_, rest)| rest);
    let host = match rest.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => rest,
    };
    host.to_ascii_lowercase()
}

/// The request target of a URL: path and query (CL-09).
fn target_of(url: &ParsedUrl) -> String {
    match url.binding_url().strip_prefix(url.origin()) {
        Some(rest) if rest.starts_with('/') => rest.to_owned(),
        _ => "/".to_owned(),
    }
}

/// The legacy request fields without the validators: the session fields of
/// the `Vary` match (CL-12). A cookie that no field can hold counts as no
/// cookie here; the request then fails (CL-09).
fn match_fields(accept_diff: bool, cookie: Option<&str>) -> Vec<FieldLine> {
    let with_cookie = LegacyRequest {
        accept_diff,
        entry: None,
        cookie,
    };
    request_fields(&with_cookie).unwrap_or_else(|_| {
        request_fields(&LegacyRequest {
            cookie: None,
            ..with_cookie
        })
        .unwrap_or_default()
    })
}

/// A cache effect.
pub(super) fn cache(txn: CacheTransaction) -> Effect {
    Effect::Cache(Box::new(txn))
}

impl Session {
    /// A new session for `context`; its first effect is the lookup of `key`.
    pub(crate) fn new(context: OpenContext, key: CacheKey, url_id: String) -> Self {
        Self {
            partition: context.partition,
            url_id,
            key,
            target: target_of(&context.url),
            host: host_of(context.url.origin()),
            anonymous: context.anonymous,
            network_valid: context.network_valid,
            platform_fields: context.platform_fields,
            vary_key: context.vary_key,
            step: Step::Lookup { opening: true },
            stopped: false,
            partition_epoch: Epoch(0),
            site_epoch: Epoch(0),
            seen: Expect::Any,
            entry: None,
            entry_stored: false,
            request_fields: Vec::new(),
            request_time: UnixTime(0),
            exchanges: Vec::new(),
            current: None,
            sent: 0,
            generation: 0,
            refreshed: false,
            kind: None,
            bound: false,
            ready: false,
            presented: false,
            armed: true,
            doc: None,
            no_doc: false,
            decided: false,
            pending_new: None,
            waiting: Vec::new(),
            first_delivery: None,
            cached_given: false,
            next_doc: 0,
            result: None,
            callback: false,
        }
    }

    /// The lookup of the key (H-160).
    pub(crate) const fn lookup_effect(&self) -> Effect {
        Effect::Lookup {
            partition: Partition::Scoped(self.partition),
            key: self.key,
        }
    }

    /// The write basis of the session with `expect` (DL-220): the request
    /// partition and the epochs of the opening lookup (DL-229 (a)).
    pub(super) const fn basis(&self, expect: Expect) -> WriteBasis {
        WriteBasis {
            partition: self.partition,
            key: self.key,
            partition_epoch: self.partition_epoch,
            site_epoch: self.site_epoch,
            expect,
        }
    }

    /// The values of the `Vary` names (CL-12): the request fields of the
    /// generation and the platform fields.
    pub(super) fn request_values(&self) -> RequestValues<'_> {
        RequestValues {
            session_fields: &self.request_fields,
            platform_fields: &self.platform_fields,
        }
    }

    /// A new document id.
    pub(super) const fn next_doc_id(&mut self) -> u64 {
        let id = self.next_doc;
        self.next_doc = self.next_doc.saturating_add(1);
        id
    }

    /// The exchange `id`, if the session keeps it.
    pub(super) fn exchange(&self, id: ExchangeId) -> Option<&Exchange> {
        self.exchanges.iter().find(|exchange| exchange.id == id)
    }

    /// The exchange `id`, if the session keeps it.
    pub(super) fn exchange_mut(&mut self, id: ExchangeId) -> Option<&mut Exchange> {
        self.exchanges.iter_mut().find(|exchange| exchange.id == id)
    }

    /// The exchange `id` is the request of the current generation.
    pub(super) fn is_fresh(&self, id: ExchangeId) -> bool {
        self.current == Some(id)
            && self
                .exchange(id)
                .is_some_and(|exchange| exchange.generation == self.generation)
    }

    /// One input. The engine never calls it for a stopped session.
    pub(crate) fn handle(&mut self, env: &Env<'_>, input: Input, out: &mut Vec<Effect>) -> Fate {
        match input {
            Input::Lookup(result) => {
                let fate = self.on_lookup(env, *result, out);
                if fate != Fate::Stays {
                    return fate;
                }
            }
            Input::Cookie(cookie) => self.on_cookie(env, cookie.as_deref(), out),
            Input::ResponseHead {
                exchange,
                status,
                fields,
            } => self.on_head(env, exchange, status, fields, out),
            Input::BodyChunk { exchange, bytes } => self.on_chunk(exchange, &bytes, out),
            Input::BodyEnd { exchange } => self.on_end(env, exchange, out),
            Input::TransportFailed { exchange, error } => {
                self.on_failed(exchange, error, out);
            }
            Input::BindClient => {
                self.bound = true;
                self.present(env, out);
            }
            Input::ClientReady => {
                self.ready = true;
                self.present(env, out);
            }
            Input::Intercept(intercept) => self.intercept(intercept, out),
            Input::GetDiff => {
                self.callback = true;
                self.send_message(env, out);
            }
            Input::Refresh => self.refresh(out),
            Input::Destroy => {
                self.close(out);
                return Fate::Gone;
            }
            // CL-36; FSM-05 deadlines are S3.
            Input::PageFinished | Input::Tick => {}
        }
        self.prune(out);
        Fate::Stays
    }

    // --- request flow (CL-08 to CL-12) ---

    /// The answer to the lookup.
    ///
    /// The opening lookup sets the epochs of every write of the session
    /// (DL-220). A later lookup (CL-35) that reports another partition
    /// epoch or site epoch means a revoke or a site clear that the engine
    /// did not see: the session stops as CL-06 and writes nothing
    /// (DL-229 (a), RSK-06).
    fn on_lookup(&mut self, env: &Env<'_>, result: LookupResult, out: &mut Vec<Effect>) -> Fate {
        let Step::Lookup { opening } = self.step else {
            return Fate::Stays;
        };
        if opening {
            if result.disabled {
                // CL-08: the session does not open; nothing else.
                self.stopped = true;
                out.push(Effect::Closed);
                return Fate::Gone;
            }
            self.partition_epoch = result.partition_epoch;
            self.site_epoch = result.site_epoch;
        } else if result.partition_epoch != self.partition_epoch
            || result.site_epoch != self.site_epoch
        {
            self.close(out);
            return Fate::Stopped;
        }
        if self.network_valid {
            self.step = Step::Cookie {
                stored: result.entry.map(Box::new),
            };
            out.push(Effect::ReadCookie);
        } else {
            // CL-10: no request and no cookie call; the cached page or a
            // normal load. Without a cookie read the `Vary` match uses no
            // cookie.
            self.step = Step::Done;
            self.choose_entry(env, result.entry.as_ref(), None, out);
            self.decided = true;
            if self.entry.is_none() {
                self.normal_load(false, out);
            }
            self.resolve_waiting(out);
        }
        Fate::Stays
    }

    /// The visible entry of the generation (CL-12, CL-40). A damaged entry
    /// is removed.
    fn choose_entry(
        &mut self,
        env: &Env<'_>,
        stored: Option<&StoredEntry>,
        cookie: Option<&str>,
        out: &mut Vec<Effect>,
    ) {
        self.entry = None;
        self.entry_stored = false;
        let Some(stored) = stored else {
            self.seen = Expect::Absent;
            return;
        };
        let fields = match_fields(env.config.accept_diff, cookie);
        let values = RequestValues {
            session_fields: &fields,
            platform_fields: &self.platform_fields,
        };
        match (env.port.check_visible)(stored, &values, &self.vary_key) {
            Ok(mut record) => {
                // DL-229 (d): the stored lines pass the filter of DL-226 (b)
                // whatever the record port gives, so neither the cached
                // document nor the field update of CL-17 holds such a line.
                record.fields = record::stored_page_fields(&record.fields);
                self.seen = Expect::Version(record.version);
                self.entry_stored = true;
                let doc = self.cached_doc(&record);
                self.doc = Some(doc);
                self.entry = Some(record);
            }
            Err(NotVisible::OtherVariant) => self.seen = Expect::Version(stored.version),
            Err(NotVisible::Damaged) => {
                out.push(cache(record::remove(
                    &self.basis(Expect::Version(stored.version)),
                )));
                self.seen = Expect::Absent;
            }
        }
    }

    /// The document of the cached page of `record` (CL-20, CL-21: the stored
    /// page fields after the filter of DL-229 (d), lower case, in a stable
    /// order by name).
    pub(super) fn cached_doc(&mut self, record: &LegacyRecord) -> Doc {
        Doc {
            id: self.next_doc_id(),
            cached: true,
            source: Source::Bytes(record.page.as_bytes().to_vec()),
            fields: record::stored_page_fields(&record.fields),
        }
    }

    /// The cookie: the request goes now (CL-09, CL-11 item 1).
    fn on_cookie(&mut self, env: &Env<'_>, cookie: Option<&str>, out: &mut Vec<Effect>) {
        if !matches!(self.step, Step::Cookie { .. }) {
            return;
        }
        let Step::Cookie { stored } = core::mem::replace(&mut self.step, Step::Done) else {
            return;
        };
        self.choose_entry(env, stored.as_deref(), cookie, out);
        let request = LegacyRequest {
            accept_diff: env.config.accept_diff,
            entry: self.entry.as_ref().map(|entry| EntryValidators {
                etag: entry.etag,
                template_tag: entry.template_tag,
            }),
            cookie,
        };
        let Ok(fields) = request_fields(&request) else {
            // A cookie that no field can hold: no legacy request, a normal
            // load (RequestError::CookieValue).
            self.decided = true;
            self.normal_load(false, out);
            return;
        };
        self.sent = self.sent.saturating_add(1);
        let id = ExchangeId(self.sent);
        self.request_fields.clone_from(&fields);
        self.request_time = env.now.wall;
        self.exchanges.push(Exchange::new(id, self.generation));
        self.current = Some(id);
        out.push(Effect::Send(RequestSpec {
            exchange: id,
            target: self.target.clone(),
            fields,
        }));
    }

    // --- response (CL-13) ---

    /// A response head.
    fn on_head(
        &mut self,
        env: &Env<'_>,
        id: ExchangeId,
        status: u16,
        fields: Vec<FieldLine>,
        out: &mut Vec<Effect>,
    ) {
        if id.0 == 0 || id.0 > self.sent {
            return; // not an exchange of this session
        }
        let cookies = super::head::set_cookie_values(&fields);
        if !cookies.is_empty() {
            // CL-11 item 2: before the session acts on the response, also
            // for an older generation.
            out.push(Effect::StoreCookies(cookies));
        }
        let fresh = self.is_fresh(id);
        let entry_etag = self.entry.as_ref().map(|entry| entry.etag);
        let Some(exchange) = self.exchange_mut(id) else {
            return;
        };
        if exchange.head.is_some() || exchange.has_ended() {
            return;
        }
        let kind = fresh.then(|| {
            super::head::classify(
                status,
                &fields,
                entry_etag
                    .as_ref()
                    .map(rivqen_integrity::legacy::Sha1Hex::as_str),
            )
        });
        match kind {
            Some(Kind::FirstLoad | Kind::TemplateChange) => {
                exchange.buffer_up_to(env.config.template.max_document_bytes());
            }
            Some(Kind::DataUpdate) => exchange.buffer_up_to(env.config.body.max_bytes()),
            _ => {}
        }
        exchange.head = Some(Head {
            fields,
            time: env.now.wall,
        });
        if let Some(kind) = kind {
            self.kind = Some(kind);
            self.on_kind(env, id, kind, out);
        }
    }

    /// A body chunk.
    fn on_chunk(&mut self, id: ExchangeId, bytes: &[u8], out: &mut Vec<Effect>) {
        let fresh = self.is_fresh(id);
        let Some(exchange) = self.exchange_mut(id) else {
            return;
        };
        if exchange.head.is_none() || exchange.has_ended() {
            return;
        }
        match exchange.push(bytes) {
            super::exchange::Push::Ignored => {}
            super::exchange::Push::Stored => {
                exchange.forward(out);
                if self.doc_reads(id) && !self.waiting.is_empty() {
                    self.resolve_waiting(out);
                }
            }
            push => {
                exchange.end = End::Failed;
                if !fresh {
                    self.old_failed(id, out);
                } else if push == super::exchange::Push::NotUtf8 {
                    self.not_text(id, out); // CL-16
                } else {
                    self.cut(id, false, out); // over the limit: CL-33
                }
            }
        }
    }

    /// The end of a body.
    ///
    /// Every buffered body (a page or a data body, of this generation or of
    /// an older one) runs the end checks of CL-16 and CL-32 first: a body
    /// that is empty or not UTF-8 ends [`End::Failed`], its streams end
    /// incomplete and it is never a complete document (DL-229 (c)).
    fn on_end(&mut self, env: &Env<'_>, id: ExchangeId, out: &mut Vec<Effect>) {
        let fresh = self.is_fresh(id);
        let kind = self.kind;
        let Some(exchange) = self.exchange_mut(id) else {
            return;
        };
        if exchange.head.is_none() || exchange.has_ended() {
            return;
        }
        if exchange.is_buffered() && (exchange.body.is_empty() || !exchange.is_text()) {
            exchange.end = End::Failed;
            if fresh {
                self.not_text(id, out); // CL-32, CL-16
            } else {
                self.old_failed(id, out);
            }
            return;
        }
        exchange.end = End::Complete;
        match kind {
            Some(kind) if fresh && kind.needs_body() => {
                match kind {
                    Kind::FirstLoad => self.first_load_end(env, id, out),
                    Kind::TemplateChange => self.template_change_end(env, id, out),
                    _ => self.data_update_end(env, id, out),
                }
            }
            _ => {
                exchange.finish_streams(true, out);
                if self.doc_reads(id) && !self.waiting.is_empty() {
                    self.resolve_waiting(out);
                }
            }
        }
    }

    /// A transport failure: before the head (CL-18) or after it (CL-33).
    fn on_failed(&mut self, id: ExchangeId, error: TransportError, out: &mut Vec<Effect>) {
        let fresh = self.is_fresh(id);
        let kind = self.kind;
        let Some(exchange) = self.exchange_mut(id) else {
            return;
        };
        if exchange.has_ended() {
            return;
        }
        exchange.end = End::Failed;
        let tls = error == TransportError::Tls;
        if !fresh {
            self.old_failed(id, out);
        } else if exchange.head.is_none() {
            self.decided = true;
            if tls {
                self.normal_load(false, out); // CL-18 item 3
            } else {
                self.keep_cached(out); // CL-18 item 1
            }
        } else if kind.is_some_and(Kind::needs_body) {
            self.cut(id, tls, out); // CL-33
        } else {
            // The outcome was decided at the head without the body.
            exchange.finish_streams(false, out);
        }
    }

    // --- refresh and stop (CL-35, CL-34, CL-06) ---

    /// CL-35: a new generation with the validators of the visible entry and
    /// a new cookie read.
    fn refresh(&mut self, out: &mut Vec<Effect>) {
        self.generation = self.generation.saturating_add(1);
        self.refreshed = true;
        self.result = None;
        self.kind = None;
        self.pending_new = None;
        self.no_doc = false;
        self.decided = false;
        self.armed = false;
        // The document of the old generation counts as delivered.
        self.first_delivery = Some(Delivery {
            doc: self.doc.as_ref().map(|doc| doc.id),
            complete: true,
        });
        for intercept in core::mem::take(&mut self.waiting) {
            out.push(Effect::AnswerNull(intercept));
        }
        self.entry = None;
        self.entry_stored = false;
        self.seen = Expect::Any;
        // A lookup or a cookie read in progress serves the new generation.
        if matches!(self.step, Step::Done) {
            self.step = Step::Lookup { opening: false };
            out.push(self.lookup_effect());
        }
    }

    /// CL-34, CL-06: the result, then the stop, then `Closed`. A waiting
    /// intercept gets `null`; a stream ends incomplete; no later effect.
    pub(crate) fn close(&mut self, out: &mut Vec<Effect>) {
        let (src, final_code) = self
            .result
            .as_ref()
            .map_or((ResultCode::NoResult, ResultCode::NoResult), |result| {
                (result.src, result.final_code)
            });
        out.push(Effect::Result { src, final_code });
        for intercept in core::mem::take(&mut self.waiting) {
            out.push(Effect::AnswerNull(intercept));
        }
        for exchange in &mut self.exchanges {
            exchange.finish_streams(false, out);
        }
        self.stopped = true;
        self.exchanges = Vec::new();
        self.doc = None;
        self.pending_new = None;
        self.entry = None;
        self.result = None;
        self.request_fields = Vec::new();
        self.platform_fields = Vec::new();
        out.push(Effect::Closed);
    }

    /// Drop the exchanges that nothing reads; keep at most
    /// [`MAX_EXCHANGES`].
    fn prune(&mut self, out: &mut Vec<Effect>) {
        let current = self.current;
        let doc = self.doc_exchange();
        let pending = self.pending_new.as_ref().and_then(Doc::exchange);
        self.exchanges.retain(|exchange| {
            Some(exchange.id) == current
                || Some(exchange.id) == doc
                || Some(exchange.id) == pending
                || !exchange.streams.is_empty()
        });
        while self.exchanges.len() > MAX_EXCHANGES {
            let Some(index) = self
                .exchanges
                .iter()
                .position(|exchange| Some(exchange.id) != current)
            else {
                break;
            };
            let mut oldest = self.exchanges.remove(index);
            oldest.end = End::Failed;
            oldest.finish_streams(false, out);
        }
    }

    /// The exchange that the current document reads.
    pub(super) fn doc_exchange(&self) -> Option<ExchangeId> {
        self.doc.as_ref().and_then(Doc::exchange)
    }

    /// The current document reads the body of `id`.
    pub(super) fn doc_reads(&self, id: ExchangeId) -> bool {
        self.doc_exchange() == Some(id)
    }

    /// The fields and the time of the head of `id`.
    pub(super) fn head_of(&self, id: ExchangeId) -> Option<(Vec<FieldLine>, UnixTime)> {
        self.exchange(id)
            .and_then(|exchange| exchange.head.as_ref())
            .map(|head| (head.fields.clone(), head.time))
    }
}

impl Doc {
    /// The exchange whose body is the document.
    pub(super) const fn exchange(&self) -> Option<ExchangeId> {
        match self.source {
            Source::Response(id) => Some(id),
            Source::Bytes(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::host_of;

    #[test]
    fn host_of_removes_the_scheme_and_the_port() {
        assert_eq!(host_of("https://Shop.Example.com"), "shop.example.com");
        assert_eq!(host_of("https://a.example:8443"), "a.example");
        assert_eq!(host_of("http://[::1]:8080"), "[::1]");
        assert_eq!(host_of("http://[::1]"), "[::1]");
    }
}
