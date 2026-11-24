// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Every open legacy session of one engine (WP-08 S2 contract; legacy
//! client CL-05 to CL-37).
//!
//! Owner of the bodies and of the private items: lane L1 (T-01). The public
//! items are the contract.
//!
//! # Rules
//!
//! - [`LegacyEngine::open`] (CL-05, CL-07): a session for a key (partition
//!   and canonical URL, H-160) that has an open session (until its
//!   `Destroy`) does not open: the effects are only [`Effect::Closed`]. Else
//!   the first effect is [`Effect::Lookup`]; the answer can still close the
//!   session for a disabled key (CL-08).
//! - [`LegacyEngine::handle`]: an input of an unknown or closed session
//!   gives no effect (FSM-02, CL-34). The session logic is the private
//!   module `crate::legacy::fsm`.
//! - [`LegacyEngine::partition_changed`] (CL-06, CL-45): one revoke of the
//!   old partition, then every open session of it stops at once. A session
//!   also stops when a later lookup reports other epochs (DL-229 (a)). A
//!   stopped session keeps its key until its `Destroy` (CL-07) and gives no
//!   effect.
//!
//! # Bounds
//!
//! At most [`MAX_SESSIONS`] open sessions. A stopped session does not count
//! in that bound (DL-229 (h)): the engine drops its state and keeps only
//! its key (partition and canonical URL) in a separate set until its
//! `Destroy`. When that set holds [`MAX_HELD_KEYS`] keys, a new session
//! does not open (fail closed: `Closed`, a normal load) until the app
//! destroys a stopped session. A stop is never refused, so the set holds
//! fewer than `MAX_HELD_KEYS + MAX_SESSIONS` keys. Over [`MAX_SESSIONS`]
//! open sessions a new session does not open either. Both are `BTreeMap`s,
//! so every walk over them is in id order and the effects are
//! deterministic.

use core::fmt;
use std::collections::BTreeMap;

use rivqen_cache::key::Partition;
use rivqen_cache::txn::{CacheTransaction, Epoch, Expect, Preconditions, TxnOp, Writer};
use rivqen_model::ids::PartitionId;

use crate::legacy::fsm::port::RecordPort;
use crate::legacy::fsm::session::{Env, Fate, Session};
use crate::legacy::record;
use crate::legacy::types::{Effect, Input, LegacyConfig, Now, OpenContext, SessionId};

/// The most open sessions of one engine. A web view has one main frame, so
/// a real app has few; the limit bounds the memory when an adapter never
/// destroys its sessions.
const MAX_SESSIONS: usize = 256;

/// The number of keys of stopped sessions (not yet destroyed) at which a
/// new session does not open. A held key is small (a partition id and a
/// canonical URL), so the bound is larger than [`MAX_SESSIONS`].
const MAX_HELD_KEYS: usize = 1024;

/// The key of a stopped session that is not destroyed yet (CL-07).
struct HeldKey {
    partition: PartitionId,
    url_id: String,
}

impl fmt::Debug for HeldKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeldKey")
            .field("url_len", &self.url_id.len())
            .finish_non_exhaustive()
    }
}

/// The legacy sessions of one engine (CL-05 to CL-37).
#[derive(Debug)]
pub struct LegacyEngine {
    config: LegacyConfig,
    next_id: u64,
    /// The open sessions.
    sessions: BTreeMap<SessionId, Session>,
    /// The keys of the stopped sessions until their `Destroy`.
    held: BTreeMap<SessionId, HeldKey>,
    port: RecordPort,
}

/// Move the stopped session `id` from `sessions` to `held`: its state is
/// dropped, its key stays taken (CL-07).
fn hold(
    sessions: &mut BTreeMap<SessionId, Session>,
    held: &mut BTreeMap<SessionId, HeldKey>,
    id: SessionId,
) {
    if let Some(session) = sessions.remove(&id) {
        held.insert(
            id,
            HeldKey {
                partition: session.partition,
                url_id: session.url_id,
            },
        );
    }
}

impl LegacyEngine {
    /// An engine with no session.
    #[must_use]
    pub const fn new(config: LegacyConfig) -> Self {
        Self::with_port(config, RecordPort::RECORD)
    }

    /// An engine with other record functions (unit tests).
    pub(crate) const fn with_port(config: LegacyConfig, port: RecordPort) -> Self {
        Self {
            config,
            next_id: 0,
            sessions: BTreeMap::new(),
            held: BTreeMap::new(),
            port,
        }
    }

    /// The configuration.
    #[must_use]
    pub const fn config(&self) -> &LegacyConfig {
        &self.config
    }

    /// The step `create_session`: a new session for the navigation of
    /// `context`. The id is new even when the session does not open; then
    /// the effects end with [`Effect::Closed`] (CL-07, CL-08).
    pub fn open(&mut self, now: Now, context: OpenContext) -> (SessionId, Vec<Effect>) {
        let _ = now; // the open reads nothing; the lookup answer carries the store state
        let id = SessionId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        if self.sessions.len() >= MAX_SESSIONS || self.held.len() >= MAX_HELD_KEYS {
            return (id, vec![Effect::Closed]);
        }
        let url_id = context.url.cache_url(&[]);
        let partition = context.partition;
        let taken = self
            .sessions
            .values()
            .any(|session| session.partition == partition && session.url_id == url_id)
            || self
                .held
                .values()
                .any(|key| key.partition == partition && key.url_id == url_id);
        if taken {
            return (id, vec![Effect::Closed]); // CL-07
        }
        let key = record::entry_key(partition, &context.url);
        let session = Session::new(context, key, url_id);
        let effects = vec![session.lookup_effect()];
        self.sessions.insert(id, session);
        (id, effects)
    }

    /// One input of a session. An input of an unknown or closed session
    /// gives no effect.
    pub fn handle(&mut self, now: Now, session: SessionId, input: Input) -> Vec<Effect> {
        let Self {
            config,
            sessions,
            held,
            port,
            ..
        } = self;
        if held.contains_key(&session) {
            // CL-06 then `destroy`: the key is free again (CL-07).
            if input == Input::Destroy {
                held.remove(&session);
            }
            return Vec::new();
        }
        let Some(state) = sessions.get_mut(&session) else {
            return Vec::new();
        };
        let env = Env { config, port, now };
        let mut out = Vec::new();
        match state.handle(&env, input, &mut out) {
            Fate::Stays => {}
            Fate::Stopped => hold(sessions, held, session),
            Fate::Gone => {
                sessions.remove(&session);
            }
        }
        out
    }

    /// The account changed (CL-06): every open session of `old` stops at
    /// once, and the entries of `old` are removed. The effects name their
    /// session; the revoke of the partition is one [`Effect::Cache`] of the
    /// first session in the list, or of no session when none is open
    /// (`None`).
    pub fn partition_changed(
        &mut self,
        now: Now,
        old: PartitionId,
    ) -> Vec<(Option<SessionId>, Effect)> {
        let _ = now; // the stop reads no time
        let mut stopped = Vec::new();
        for (id, session) in &mut self.sessions {
            if session.partition != old {
                continue;
            }
            let mut effects = Vec::new();
            session.close(&mut effects);
            stopped.push((*id, effects));
        }
        for (id, _) in &stopped {
            hold(&mut self.sessions, &mut self.held, *id);
        }
        let first = stopped.first().map(|(id, _)| *id);
        let mut out = vec![(first, Effect::Cache(Box::new(revoke(old))))];
        for (id, effects) in stopped {
            out.extend(effects.into_iter().map(|effect| (Some(id), effect)));
        }
        out
    }
}

/// The revoke of a partition (R-CACHE-04). A maintenance write (DL-220): no
/// session of the partition writes it, and a driver never refuses it, so its
/// preconditions are not read.
const fn revoke(partition: PartitionId) -> CacheTransaction {
    CacheTransaction {
        op: TxnOp::RevokePartition {
            partition: Partition::Scoped(partition),
        },
        pre: Preconditions {
            entry: Expect::Any,
            writer: Writer::Maintenance,
            partition_epoch: Epoch(0),
            site_epoch: Epoch(0),
        },
    }
}
