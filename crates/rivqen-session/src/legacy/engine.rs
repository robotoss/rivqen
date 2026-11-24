// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Every open legacy session of one engine (WP-08 S2 contract).
//!
//! Owner of the bodies and of the private items: lane L1 (T-01). The public
//! items are the contract. Until L1 is integrated, every session closes at
//! once (no session: the web view loads normally).

use rivqen_model::ids::PartitionId;

use crate::legacy::types::{Effect, Input, LegacyConfig, Now, OpenContext, SessionId};

/// The legacy sessions of one engine (CL-05 to CL-37).
#[derive(Debug)]
pub struct LegacyEngine {
    config: LegacyConfig,
    next_id: u64,
}

impl LegacyEngine {
    /// An engine with no session.
    #[must_use]
    pub const fn new(config: LegacyConfig) -> Self {
        Self { config, next_id: 0 }
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
        let _ = (now, context);
        let id = SessionId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        (id, vec![Effect::Closed])
    }

    /// One input of a session. An input of an unknown or closed session
    /// gives no effect.
    pub fn handle(&mut self, now: Now, session: SessionId, input: Input) -> Vec<Effect> {
        let _ = (now, session, input);
        Vec::new()
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
        let _ = (now, old);
        Vec::new()
    }
}
