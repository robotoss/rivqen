// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The L1 memory cache (cache layers section 2; storage section 4;
//! `memoryL1Bytes`). Owner: WP-07 S1 T-06.
//!
//! Contract (architect): the public items of this file. T-06 writes the
//! bodies and the private fields.

use rivqen_model::ids::CacheKey;

use crate::key::Partition;

/// How much memory the OS asks to free (storage section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureLevel {
    /// Halve the L1 cache.
    Moderate,
    /// Clear the L1 cache.
    Critical,
}

/// The value does not fit: it is larger than the whole budget
/// (`MEMORY_BUDGET_EXCEEDED`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverBudget;

/// A bounded memory cache with least-recently-used eviction.
#[derive(Debug)]
pub struct L1Cache<V> {
    budget_bytes: u64,
    entries: Vec<(Partition, CacheKey, u64, V)>,
}

impl<V> L1Cache<V> {
    /// An empty cache with a budget in bytes.
    #[must_use]
    pub const fn new(budget_bytes: u64) -> Self {
        Self {
            budget_bytes,
            entries: Vec::new(),
        }
    }

    /// The budget in bytes.
    #[must_use]
    pub const fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    /// The bytes in use: never more than the budget.
    #[must_use]
    pub fn used_bytes(&self) -> u64 {
        self.entries
            .iter()
            .fold(0, |sum, entry| sum.saturating_add(entry.2))
    }

    /// The value of `key` in `partition`; marks it as most recently used.
    ///
    /// The entries are in order of use: the first is the least recently used.
    pub fn get(&mut self, partition: Partition, key: &CacheKey) -> Option<&V> {
        let index = self.position(partition, key)?;
        let entry = self.entries.remove(index);
        self.entries.push(entry);
        self.entries.last().map(|entry| &entry.3)
    }

    /// Stores a value of `size_bytes` and evicts least recently used values
    /// until it fits.
    ///
    /// # Errors
    ///
    /// [`OverBudget`] when `size_bytes` is over the budget. The value is not
    /// stored, and an older value of the same partition and key is removed:
    /// the caller committed a newer version to L2, so the older one must not
    /// stay readable. Other values do not change.
    pub fn insert(
        &mut self,
        partition: Partition,
        key: CacheKey,
        value: V,
        size_bytes: u64,
    ) -> Result<(), OverBudget> {
        self.remove(partition, &key);
        if size_bytes > self.budget_bytes {
            return Err(OverBudget);
        }
        let room = self.budget_bytes.saturating_sub(size_bytes);
        self.evict_down_to(room);
        self.entries.push((partition, key, size_bytes, value));
        Ok(())
    }

    /// Removes one value.
    pub fn remove(&mut self, partition: Partition, key: &CacheKey) {
        if let Some(index) = self.position(partition, key) {
            self.entries.remove(index);
        }
    }

    /// Removes every value of a partition (R-CACHE-04).
    pub fn remove_partition(&mut self, partition: Partition) {
        self.entries.retain(|entry| entry.0 != partition);
    }

    /// Removes every value (site clear; a caller that knows the hosts uses
    /// [`L1Cache::remove`]).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Reacts to memory pressure (storage section 4): `Moderate` evicts least
    /// recently used values until at most half of the budget (rounded down)
    /// is in use; `Critical` clears the cache.
    pub fn on_pressure(&mut self, level: PressureLevel) {
        match level {
            PressureLevel::Moderate => self.evict_down_to(self.budget_bytes / 2),
            PressureLevel::Critical => self.clear(),
        }
    }

    fn position(&self, partition: Partition, key: &CacheKey) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.0 == partition && entry.1 == *key)
    }

    /// Removes the least recently used values until `used_bytes` is at most
    /// `limit`.
    fn evict_down_to(&mut self, limit: u64) {
        let mut used = self.used_bytes();
        let mut count = 0usize;
        for entry in &self.entries {
            if used <= limit {
                break;
            }
            used = used.saturating_sub(entry.2);
            count = count.saturating_add(1);
        }
        self.entries.drain(..count);
    }
}
