// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! One network request for identical opens in one partition (R-CACHE-07;
//! freshness section 5). Owner: WP-07 S1 T-06.
//!
//! Contract (architect): the public items of this file. The table is state,
//! not a lock: the core has no threads (R-CORE-12). The session (WP-08)
//! connects it to requests and events (DL-185).

use rivqen_model::ids::{CacheKey, RequestId};

use crate::key::Partition;

/// The role of a request that joins the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// No request for the key is in flight: this request goes to the network.
    Leader,
    /// A request for the key is in flight: wait for the result of this
    /// leader.
    Follower(RequestId),
}

/// The requests in flight, by partition and key. A key of one partition
/// never joins a request of another partition.
#[derive(Debug, Default)]
pub struct InFlight {
    entries: Vec<(Partition, CacheKey, RequestId, Vec<RequestId>)>,
}

impl InFlight {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Adds `request` for `key` in `partition`.
    pub fn join(&mut self, partition: Partition, key: CacheKey, request: RequestId) -> Join {
        // Stub (T-06): every request is its own leader.
        let _ = (partition, key, request);
        Join::Leader
    }

    /// The leader of `key` finished or failed: removes the key and returns
    /// its followers in the order of their joins.
    pub fn finish(&mut self, partition: Partition, key: &CacheKey) -> Vec<RequestId> {
        // Stub (T-06).
        let _ = (partition, key);
        Vec::new()
    }

    /// Removes every key of a partition (R-CACHE-04) and returns all its
    /// requests: they are cancelled.
    pub fn cancel_partition(&mut self, partition: Partition) -> Vec<RequestId> {
        // Stub (T-06).
        let _ = partition;
        Vec::new()
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
