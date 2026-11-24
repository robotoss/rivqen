// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The delivery of the document to the web view (legacy client CL-20,
//! CL-21): `load_url`, `load_data` and the answers of the intercepted
//! requests.
//!
//! The web view gets no call before `bind_client` and `client_ready`
//! (CL-20 item 7). An intercept gets the current document once after each
//! arming (the start, a `load_url`); every other intercept gets `null`
//! (CL-20 item 5). An intercept waits for the response head when there is no
//! document yet, and for the first body byte when the document is a
//! response with no byte yet (so that an empty body, CL-32, gives `null`).

use super::exchange::Exchange;
use super::session::{Delivery, Doc, Env, Session, Source};
use crate::legacy::types::{DocumentHead, Effect, InterceptId, Presentation};

/// The status of every document (CL-20).
const DOCUMENT_STATUS: u16 = 200;

/// What [`Session::give`] does with the current document.
enum Give {
    /// Answer with a complete document.
    Complete(Vec<u8>),
    /// The body is still empty: wait for its first byte.
    Wait,
    /// Answer with a stream (opened).
    Streamed,
    /// No document can be given.
    Null,
}

impl Session {
    /// The cached page (or, in a refresh, the document of the old
    /// generation) was delivered (CL-27, CL-31, CL-35).
    pub(super) const fn delivered(&self) -> bool {
        self.cached_given || self.refreshed
    }

    /// The document is complete.
    fn is_complete(&self, doc: &Doc) -> bool {
        match doc.source {
            Source::Bytes(_) => true,
            Source::Response(id) => self.exchange(id).is_some_and(Exchange::is_complete),
        }
    }

    /// The bytes of a complete document.
    fn complete_bytes(&self, doc: &Doc) -> Option<Vec<u8>> {
        match &doc.source {
            Source::Bytes(bytes) => Some(bytes.clone()),
            Source::Response(id) => self
                .exchange(*id)
                .filter(|exchange| exchange.is_complete())
                .map(|exchange| exchange.body.clone()),
        }
    }

    /// CL-20 items 2, 3 and 7: the state when the client is bound and ready
    /// decides the first call.
    pub(super) fn present(&mut self, env: &Env<'_>, out: &mut Vec<Effect>) {
        if !self.bound || !self.ready || self.presented {
            return;
        }
        self.presented = true;
        let quick_document = env.config.presentation == Presentation::Quick
            && self.doc.as_ref().is_some_and(|doc| self.is_complete(doc));
        if quick_document {
            self.web_load_data(out);
        } else {
            self.web_load_url(out);
        }
    }

    /// The web view loads the session URL; the next intercept gets the
    /// current document.
    pub(super) fn web_load_url(&mut self, out: &mut Vec<Effect>) {
        if !self.bound || !self.ready {
            return;
        }
        out.push(Effect::LoadUrl);
        self.armed = true;
    }

    /// Quick: `load_data` of the current document (complete).
    pub(super) fn web_load_data(&mut self, out: &mut Vec<Effect>) {
        if !self.bound || !self.ready {
            return;
        }
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        let Some(body) = self.complete_bytes(doc) else {
            return;
        };
        let (id, cached) = (doc.id, doc.cached);
        let head = DocumentHead {
            status: DOCUMENT_STATUS,
            fields: doc.fields.clone(),
        };
        out.push(Effect::LoadData { head, body });
        self.note_delivery(id, cached, true);
        self.armed = false;
    }

    /// CL-22: the first document that the web view got.
    fn note_delivery(&mut self, doc: u64, cached: bool, complete: bool) {
        if self.first_delivery.is_none() {
            self.first_delivery = Some(Delivery {
                doc: Some(doc),
                complete,
            });
        }
        if cached {
            self.cached_given = true;
        }
    }

    /// CL-20 items 4 and 5: an intercepted request of the main document.
    pub(super) fn intercept(&mut self, intercept: InterceptId, out: &mut Vec<Effect>) {
        if !self.armed {
            out.push(Effect::AnswerNull(intercept));
            return;
        }
        self.armed = false;
        if self.doc.is_some() {
            self.give(intercept, out);
        } else if self.no_doc || self.decided {
            out.push(Effect::AnswerNull(intercept));
        } else {
            self.waiting.push(intercept);
        }
    }

    /// Give the current document to `intercept`: complete, as a stream, or
    /// later (no byte yet).
    pub(super) fn give(&mut self, intercept: InterceptId, out: &mut Vec<Effect>) {
        let Some(doc) = self.doc.as_ref() else {
            out.push(Effect::AnswerNull(intercept));
            return;
        };
        let (id, cached) = (doc.id, doc.cached);
        let head = DocumentHead {
            status: DOCUMENT_STATUS,
            fields: doc.fields.clone(),
        };
        let action = match &doc.source {
            Source::Bytes(bytes) => Give::Complete(bytes.clone()),
            Source::Response(exchange_id) => {
                match self
                    .exchanges
                    .iter_mut()
                    .find(|exchange| exchange.id == *exchange_id)
                {
                    Some(exchange) if exchange.is_complete() => {
                        Give::Complete(exchange.body.clone())
                    }
                    Some(exchange) if !exchange.has_ended() && exchange.body.is_empty() => {
                        Give::Wait
                    }
                    Some(exchange) if !exchange.has_ended() => {
                        out.push(Effect::AnswerDocument {
                            intercept,
                            head: head.clone(),
                        });
                        exchange.open_stream(intercept, out);
                        Give::Streamed
                    }
                    _ => Give::Null,
                }
            }
        };
        match action {
            Give::Complete(body) => {
                self.note_delivery(id, cached, true);
                out.push(Effect::AnswerDocument { intercept, head });
                if !body.is_empty() {
                    out.push(Effect::StreamChunk {
                        intercept,
                        bytes: body,
                    });
                }
                out.push(Effect::StreamEnd {
                    intercept,
                    complete: true,
                });
            }
            Give::Streamed => self.note_delivery(id, cached, false),
            Give::Wait => self.waiting.push(intercept),
            Give::Null => out.push(Effect::AnswerNull(intercept)),
        }
    }

    /// Answer the waiting intercepts with the current document, or `null`
    /// when there is none.
    pub(super) fn resolve_waiting(&mut self, out: &mut Vec<Effect>) {
        for intercept in core::mem::take(&mut self.waiting) {
            if self.doc.is_some() && !self.no_doc {
                self.give(intercept, out);
            } else {
                out.push(Effect::AnswerNull(intercept));
            }
        }
    }
}
