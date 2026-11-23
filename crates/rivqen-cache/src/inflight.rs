// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! One network request for identical opens in one partition (R-CACHE-07;
//! freshness section 5). Owner: WP-07 S1 T-06.
//!
//! Contract (architect): the public items of this file. The table is state,
//! not a lock: the core has no threads (R-CORE-12). The session (WP-08)
//! connects it to requests and events (DL-185).
//!
//! The table is keyed by the partition of the REQUEST ([`PartitionId`]),
//! never by the partition of a stored entry: a network request carries the
//! credentials of the profile that made it. With [`Partition::Public`] as a
//! key, two profiles would join one request made with the credentials of the
//! first one (a cross-account leak).
//!
//! [`Partition::Public`]: crate::key::Partition::Public

use rivqen_model::ids::{CacheKey, PartitionId, RequestId};

/// The role of a request that joins the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// No request for the key is in flight: this request goes to the network.
    Leader,
    /// A request for the key is in flight: wait for the result of this
    /// leader.
    Follower(RequestId),
}

/// The requests in flight, by request partition and key. A request of one
/// partition never joins a request of another partition.
#[derive(Debug, Default)]
pub struct InFlight {
    entries: Vec<(PartitionId, CacheKey, RequestId, Vec<RequestId>)>,
}

impl InFlight {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Adds `request` for `key`; `partition` is the partition of the request
    /// (its profile), also when the entry it may read is in `public`.
    pub fn join(&mut self, partition: PartitionId, key: CacheKey, request: RequestId) -> Join {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.0 == partition && entry.1 == key)
        {
            entry.3.push(request);
            return Join::Follower(entry.2);
        }
        self.entries.push((partition, key, request, Vec::new()));
        Join::Leader
    }

    /// The leader of `key` in the request partition `partition` finished or
    /// failed: removes the key and returns its followers in the order of
    /// their joins.
    pub fn finish(&mut self, partition: PartitionId, key: &CacheKey) -> Vec<RequestId> {
        match self
            .entries
            .iter()
            .position(|entry| entry.0 == partition && entry.1 == *key)
        {
            Some(index) => self.entries.remove(index).3,
            None => Vec::new(),
        }
    }

    /// Removes every key of a request partition (R-CACHE-04) and returns all
    /// its requests: they are cancelled. For each key the leader comes
    /// first, then its followers.
    pub fn cancel_partition(&mut self, partition: PartitionId) -> Vec<RequestId> {
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|entry| entry.0 == partition);
        self.entries = kept;
        let mut requests = Vec::new();
        for (_, _, leader, followers) in gone {
            requests.push(leader);
            requests.extend(followers);
        }
        requests
    }

    /// The number of keys in flight.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no key is in flight.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
