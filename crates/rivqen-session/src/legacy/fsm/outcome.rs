// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The outcomes of a legacy session (CL-17 to CL-19, CL-25 to CL-33,
//! CL-44), its results and messages (CL-22 to CL-24, CL-28).
//!
//! Every cache write is a transaction of `record.rs` with the write basis
//! of the session (DL-220). A data update reaches the page only through a
//! `Rebuilt` (CL-29, DL-226).

use rivqen_cache::header::CacheOffline;
use rivqen_cache::txn::{CacheTransaction, Expect};
use rivqen_diff::legacy::{RebuildInput, Rebuilt};
use rivqen_model::http::FieldLine;
use rivqen_model::time::UnixTime;
use rivqen_proto_legacy::body::read_data_body;
use rivqen_proto_legacy::fields::{merge_page_fields, page_fields};
use rivqen_proto_legacy::message::{MessageInput, ResultCode, message};
use rivqen_proto_legacy::response::LegacyHead;
use rivqen_template::legacy::{SplitOutcome, etag_v1, split_v1, template_tag_v1};

use super::exchange::End;
use super::head::{
    Kind, credentials_allow_commit, has_vary, offline, offline_allows_commit, response_etag,
    template_tag, vary_forbids_commit,
};
use super::session::{Doc, Env, Outcome, Session, Source, cache};
use crate::legacy::record::{self, NewRecord};
use crate::legacy::types::{DIFF_MAX_AGE_MS, Effect, ExchangeId, Presentation};

impl Session {
    /// Act on the class of a head of the current generation (CL-13).
    pub(super) fn on_kind(
        &mut self,
        env: &Env<'_>,
        id: ExchangeId,
        kind: Kind,
        out: &mut Vec<Effect>,
    ) {
        match kind {
            Kind::NotModified => self.not_modified(env, id, out),
            Kind::KeepCached => {
                self.decided = true;
                self.keep_cached(out);
            }
            Kind::Normal => self.normal_load(true, out),
            Kind::Http => self.http(env, out),
            Kind::FirstLoad => {
                self.decided = true;
                self.doc = Some(self.response_doc(id));
                self.resolve_waiting(out);
            }
            Kind::TemplateChange => {
                let doc = self.response_doc(id);
                if self.delivered() {
                    self.pending_new = Some(doc);
                } else {
                    self.doc = Some(doc);
                }
                self.decided = true;
                self.resolve_waiting(out);
            }
            // CL-27, CL-29: decided at the end of the body.
            Kind::DataUpdate => {}
        }
    }

    /// The document of the response of `id` (CL-21 page fields).
    fn response_doc(&mut self, id: ExchangeId) -> Doc {
        let fields = self
            .head_of(id)
            .map(|(fields, _)| page_fields(&fields))
            .unwrap_or_default();
        Doc {
            id: self.next_doc_id(),
            cached: false,
            source: Source::Response(id),
            fields,
        }
    }

    /// CL-18 item 1: with a visible entry the cached page stays and the
    /// entry stays; without one a normal load.
    pub(super) fn keep_cached(&mut self, out: &mut Vec<Effect>) {
        if self.entry.is_none() {
            self.normal_load(false, out);
        }
        self.resolve_waiting(out);
    }

    /// CL-20 item 6 (CL-14, CL-18, CL-19, CL-29, CL-32, CL-44): a normal
    /// load ends the current document; after the delivery of the cached
    /// page the session calls `load_url`.
    pub(super) fn normal_load(&mut self, remove_entry: bool, out: &mut Vec<Effect>) {
        self.decided = true;
        if remove_entry {
            self.remove_visible(out);
        }
        let was_delivered = self.delivered();
        self.doc = None;
        self.pending_new = None;
        self.no_doc = true;
        self.callback = false; // CL-23 item 5
        self.resolve_waiting(out);
        if was_delivered {
            self.web_load_url(out);
        }
    }

    /// Remove the visible entry, once (CL-14, CL-17, CL-27, CL-29, CL-31).
    pub(super) fn remove_visible(&mut self, out: &mut Vec<Effect>) {
        if self.entry.is_some() && self.entry_stored {
            out.push(cache(record::remove(&self.basis(self.seen))));
            self.entry_stored = false;
            self.seen = Expect::Absent;
        }
    }

    /// A commit or a field update replaced the entry: its version is not
    /// known any more.
    fn committed(&mut self, txn: CacheTransaction, out: &mut Vec<Effect>) {
        out.push(cache(txn));
        self.seen = Expect::Any;
    }

    /// CL-19: the entry removed, the key disabled for the `http` period, a
    /// normal load.
    fn http(&mut self, env: &Env<'_>, out: &mut Vec<Effect>) {
        let seconds = env.config.unavailable_time_ms.div_ceil(1000);
        let until = env.now.wall.plus(seconds);
        out.push(cache(record::disable(&self.basis(Expect::Any), until)));
        self.entry_stored = false;
        self.seen = Expect::Absent;
        self.normal_load(false, out);
    }

    /// CL-17: the cached page is the document; the cache rules of the
    /// response first (DL-126, DL-132).
    fn not_modified(&mut self, env: &Env<'_>, id: ExchangeId, out: &mut Vec<Effect>) {
        let Some((fields, time)) = self.head_of(id) else {
            return;
        };
        let value = offline(&fields);
        if value == CacheOffline::Http {
            self.http(env, out);
            return;
        }
        self.decided = true;
        let allowed = credentials_allow_commit(
            &self.request_fields,
            &fields,
            env.config.authenticated_cache,
            self.anonymous,
        );
        let update = (offline_allows_commit(value) && !vary_forbids_commit(&fields) && allowed)
            .then(|| self.field_update(env, &fields, time))
            .flatten();
        match update {
            Some((txn, merged)) => {
                self.committed(txn, out);
                if let Some(doc) = self.doc.as_mut().filter(|doc| doc.cached) {
                    doc.fields.clone_from(&merged);
                }
                if let Some(entry) = self.entry.as_mut() {
                    entry.fields = merged;
                }
            }
            None => self.remove_visible(out),
        }
        self.set_result(
            env,
            ResultCode::NotModified,
            ResultCode::NotModified,
            None,
            fields,
            out,
        );
        self.resolve_waiting(out);
    }

    /// CL-17: the commit of the visible entry with the merged page fields
    /// and, for a response with `Vary`, the `Vary` entries of this request.
    /// `None` when no `Vary` entry can be computed (the entry is removed).
    fn field_update(
        &self,
        env: &Env<'_>,
        fields: &[FieldLine],
        response_time: UnixTime,
    ) -> Option<(CacheTransaction, Vec<FieldLine>)> {
        let entry = self.entry.as_ref()?;
        let vary = if has_vary(fields) {
            (env.port.vary_entries)(fields, &self.request_values(), &self.vary_key)?
        } else {
            entry.vary.clone()
        };
        let merged = merge_page_fields(&entry.fields, fields);
        let record = NewRecord {
            host: &self.host,
            vary: &vary,
            page: &entry.page,
            template: &entry.template,
            data: &entry.data,
            fields: &merged,
            etag: entry.etag,
            template_tag: entry.template_tag,
            response_fields: fields,
            request_time: self.request_time,
            response_time,
        };
        Some(((env.port.commit)(&self.basis(self.seen), &record), merged))
    }

    /// CL-25, CL-31: the commit of a complete legacy page (first load or
    /// template change), or `None` when the rules do not allow it.
    fn page_commit(&self, env: &Env<'_>, id: ExchangeId) -> Option<CacheTransaction> {
        let exchange = self.exchange(id)?;
        let head = exchange.head.as_ref()?;
        let fields = head.fields.as_slice();
        let allowed = offline_allows_commit(offline(fields))
            && credentials_allow_commit(
                &self.request_fields,
                fields,
                env.config.authenticated_cache,
                self.anonymous,
            )
            && !vary_forbids_commit(fields);
        if !allowed {
            return None;
        }
        let legacy = LegacyHead::read(fields)?;
        let page = core::str::from_utf8(&exchange.body).ok()?;
        let etag = etag_v1(&exchange.body);
        if etag.as_str() != legacy.bare_etag() {
            return None;
        }
        let SplitOutcome::Applied(split) = split_v1(&exchange.body, &env.config.template).ok()?
        else {
            return None;
        };
        let tag = template_tag_v1(&split.template);
        if tag.as_str() != legacy.template_tag() {
            return None;
        }
        let vary = (env.port.vary_entries)(fields, &self.request_values(), &self.vary_key)?;
        let page_fields = page_fields(fields);
        let record = NewRecord {
            host: &self.host,
            vary: &vary,
            page,
            template: &split.template,
            data: &split.data,
            fields: &page_fields,
            etag,
            template_tag: tag,
            response_fields: fields,
            request_time: self.request_time,
            response_time: head.time,
        };
        Some((env.port.commit)(&self.basis(self.seen), &record))
    }

    /// CL-25, CL-26: the end of a complete first load.
    pub(super) fn first_load_end(&mut self, env: &Env<'_>, id: ExchangeId, out: &mut Vec<Effect>) {
        if let Some(exchange) = self.exchange_mut(id) {
            exchange.finish_streams(true, out);
        }
        if let Some(txn) = self.page_commit(env, id) {
            self.committed(txn, out);
        }
        let fields = self
            .head_of(id)
            .map(|(fields, _)| fields)
            .unwrap_or_default();
        let final_code = self.final_of(ResultCode::FirstLoad);
        self.set_result(env, ResultCode::FirstLoad, final_code, None, fields, out);
    }

    /// CL-31, CL-30: the end of a complete template change.
    pub(super) fn template_change_end(
        &mut self,
        env: &Env<'_>,
        id: ExchangeId,
        out: &mut Vec<Effect>,
    ) {
        let fields = self
            .head_of(id)
            .map(|(fields, _)| fields)
            .unwrap_or_default();
        let value = offline(&fields);
        let txn = self.page_commit(env, id);
        let code = ResultCode::TemplateChange;
        if self.pending_new.is_some() && value == CacheOffline::Store {
            // CL-30: no reload; final 304.
            self.pending_new = None;
            self.set_result(env, code, ResultCode::NotModified, None, fields, out);
        } else if let Some(new) = self.pending_new.take() {
            // CL-31: the reload after the complete body; the callback is
            // dropped (CL-23 item 5).
            self.doc = Some(new);
            self.callback = false;
            match env.config.presentation {
                Presentation::Quick => self.web_load_data(out),
                Presentation::Standard => self.web_load_url(out),
            }
            self.set_result(env, code, code, None, fields, out);
        } else {
            let final_code = self.final_of(code);
            self.set_result(env, code, final_code, None, fields, out);
        }
        if let Some(exchange) = self.exchange_mut(id) {
            exchange.finish_streams(true, out);
        }
        match txn {
            Some(txn) => self.committed(txn, out),
            // DL-132: the old template is not the server's any more.
            None => self.remove_visible(out),
        }
    }

    /// CL-27, CL-29, CL-30: the end of a complete data update.
    pub(super) fn data_update_end(&mut self, env: &Env<'_>, id: ExchangeId, out: &mut Vec<Effect>) {
        let Some((fields, time)) = self.head_of(id) else {
            return;
        };
        let Some(rebuilt) = self.rebuild(env, id, &fields) else {
            self.normal_load(true, out); // CL-29
            return;
        };
        let value = offline(&fields);
        let txn = self.data_commit(env, &rebuilt, &fields, value, time);
        let code = ResultCode::DataUpdate;
        if self.delivered() {
            if value == CacheOffline::Store || rebuilt.diff().is_empty() {
                // CL-30, CL-28.
                self.set_result(env, code, ResultCode::NotModified, None, fields, out);
            } else {
                self.set_result(env, code, code, Some(Box::new(rebuilt)), fields, out);
            }
        } else {
            let doc = Doc {
                id: self.next_doc_id(),
                cached: false,
                source: Source::Bytes(rebuilt.page().as_bytes().to_vec()),
                fields: page_fields(&fields),
            };
            self.doc = Some(doc);
            let final_code = self.final_of(code);
            self.set_result(env, code, final_code, None, fields, out);
        }
        match txn {
            Some(txn) => self.committed(txn, out),
            // CL-27 (D-46, DL-132): deliver, then remove the entry.
            None => self.remove_visible(out),
        }
    }

    /// CL-29: the checked rebuild of a data body, or `None`.
    fn rebuild(&self, env: &Env<'_>, id: ExchangeId, fields: &[FieldLine]) -> Option<Rebuilt> {
        let entry = self.entry.as_ref()?;
        let exchange = self.exchange(id)?;
        let body = read_data_body(&exchange.body, &env.config.body).ok()?;
        let html_sha1 = body.html_sha1().to_owned();
        let input = RebuildInput {
            template: &entry.template,
            stored: &entry.data,
            server: body.into_data(),
            html_sha1: &html_sha1,
            etag: response_etag(fields),
            response_template_tag: template_tag(fields),
            stored_template_tag: entry.template_tag.as_str(),
        };
        Rebuilt::new(input, &env.config.template).ok()
    }

    /// CL-27: the commit of a checked data update, or `None` when the rules
    /// do not allow it.
    fn data_commit(
        &self,
        env: &Env<'_>,
        rebuilt: &Rebuilt,
        fields: &[FieldLine],
        value: CacheOffline,
        response_time: UnixTime,
    ) -> Option<CacheTransaction> {
        let entry = self.entry.as_ref()?;
        let allowed = offline_allows_commit(value)
            && !vary_forbids_commit(fields)
            && credentials_allow_commit(
                &self.request_fields,
                fields,
                env.config.authenticated_cache,
                self.anonymous,
            );
        if !allowed {
            return None;
        }
        let vary = (env.port.vary_entries)(fields, &self.request_values(), &self.vary_key)?;
        let page_fields = page_fields(fields);
        let record = NewRecord {
            host: &self.host,
            vary: &vary,
            page: rebuilt.page(),
            template: &entry.template,
            data: rebuilt.data(),
            fields: &page_fields,
            etag: rebuilt.etag(),
            template_tag: entry.template_tag,
            response_fields: fields,
            request_time: self.request_time,
            response_time,
        };
        Some((env.port.commit)(&self.basis(self.seen), &record))
    }

    /// CL-33: the body of an outcome that needs it ended with an error (or
    /// passed a limit). No commit.
    pub(super) fn cut(&mut self, id: ExchangeId, tls: bool, out: &mut Vec<Effect>) {
        self.decided = true;
        let mut streaming = false;
        if let Some(exchange) = self.exchange_mut(id) {
            exchange.end = End::Failed;
            streaming = !exchange.streams.is_empty();
            exchange.finish_streams(false, out);
        }
        self.callback = false;
        self.pending_new = None;
        if streaming {
            self.doc = None;
            self.no_doc = true;
            self.web_load_url(out);
        } else if let Some(entry) = self.entry.take().filter(|_| !tls) {
            // CL-18 item 1: the cached page.
            let doc = self.cached_doc(&entry);
            self.doc = Some(doc);
            self.entry = Some(entry);
        } else {
            self.normal_load(false, out); // CL-18 item 3, D-50
        }
        self.resolve_waiting(out);
    }

    /// CL-16, CL-32 found at the body: the response is not a legacy
    /// response (as CL-14). A stream that already gets the body ends
    /// incomplete, then `load_url` (as CL-33).
    pub(super) fn not_text(&mut self, id: ExchangeId, out: &mut Vec<Effect>) {
        let mut streaming = false;
        if let Some(exchange) = self.exchange_mut(id) {
            streaming = !exchange.streams.is_empty();
            exchange.finish_streams(false, out);
        }
        if streaming {
            self.decided = true;
            self.remove_visible(out);
            self.doc = None;
            self.no_doc = true;
            self.pending_new = None;
            self.callback = false;
            self.resolve_waiting(out);
            self.web_load_url(out);
        } else {
            self.normal_load(true, out);
        }
    }

    /// DL-229 (c): the body of an exchange of an older generation (CL-35)
    /// failed: not UTF-8 or empty at its end (CL-16, CL-32), over its
    /// limit, or cut. Its streams end incomplete with the valid bytes only,
    /// then `load_url` (as CL-33). When it is the current document, the
    /// session has no document until the outcome of the current generation
    /// gives one, and a waiting intercept gets `null`. No cache effect and
    /// no result: CL-35 ignores a response of an older generation for the
    /// cache and the codes.
    pub(super) fn old_failed(&mut self, id: ExchangeId, out: &mut Vec<Effect>) {
        let mut streaming = false;
        if let Some(exchange) = self.exchange_mut(id) {
            exchange.end = End::Failed;
            streaming = !exchange.streams.is_empty();
            exchange.finish_streams(false, out);
        }
        if self.doc_reads(id) {
            self.doc = None;
            self.resolve_waiting(out);
        }
        if streaming {
            self.web_load_url(out);
        }
    }

    // --- codes and messages (CL-22 to CL-24, CL-28) ---

    /// CL-22: the final code of `src`: `304` when the web view has no
    /// document of the session yet, or when its first document is the
    /// current document and was complete at the delivery.
    pub(super) fn final_of(&self, src: ResultCode) -> ResultCode {
        match self.first_delivery {
            None => ResultCode::NotModified,
            Some(first)
                if first.complete
                    && first.doc.is_some()
                    && first.doc == self.doc.as_ref().map(|doc| doc.id) =>
            {
                ResultCode::NotModified
            }
            Some(_) => src,
        }
    }

    /// CL-22, CL-23 item 3: set the result; the message goes at once when a
    /// callback is registered.
    pub(super) fn set_result(
        &mut self,
        env: &Env<'_>,
        src: ResultCode,
        final_code: ResultCode,
        diff: Option<Box<Rebuilt>>,
        fields: Vec<FieldLine>,
        out: &mut Vec<Effect>,
    ) {
        self.result = Some(Outcome {
            src,
            final_code,
            diff,
            fields,
            at: env.now.mono,
            expired: false,
            diff_sent: false,
        });
        out.push(Effect::Result { src, final_code });
        if self.callback {
            self.send_message(env, out);
        }
    }

    /// CL-23, CL-24, CL-28: the message of the result, if any.
    pub(super) fn send_message(&mut self, env: &Env<'_>, out: &mut Vec<Effect>) {
        let reload = self.refreshed;
        let Some(result) = self.result.as_mut() else {
            return;
        };
        if result.final_code == ResultCode::NoResult || result.expired {
            return;
        }
        let age = env.now.mono.0.saturating_sub(result.at.0);
        if result.final_code == ResultCode::DataUpdate
            && result.diff.is_some()
            && age > DIFF_MAX_AGE_MS
        {
            // CL-28: an old diff is not sent, and no later message follows.
            result.expired = true;
            if !result.diff_sent {
                // DL-130: the page never got the diff: a normal load.
                result.final_code = ResultCode::NoResult;
                let src = result.src;
                out.push(Effect::Result {
                    src,
                    final_code: ResultCode::NoResult,
                });
                self.normal_load(false, out);
            }
            return;
        }
        let diff = result.diff.as_deref().map_or(&[][..], Rebuilt::diff);
        let input = MessageInput {
            src: result.src,
            final_code: result.final_code,
            diff,
            fields: &result.fields,
            reload,
        };
        if let Some(text) = message(&input) {
            out.push(Effect::Message(text));
        }
        if result.diff.is_some() {
            result.diff_sent = true;
        }
    }
}
