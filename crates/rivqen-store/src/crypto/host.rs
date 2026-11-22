// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The host key provider (ADR-011 section 2: "WP-07 has a host key provider
//! for tests only"). The keys are in memory only, so they do not survive the
//! process; a device uses the key store of the platform (WP-10, WP-11).

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::Read;

use rivqen_cache::driver::StoreError;
use rivqen_cache::key::Partition;

use super::{DataKey, KeyProvider, unavailable};

/// The system random source of the host.
const SYSTEM_SOURCE: &str = "/dev/urandom";

/// A key provider with the keys in memory, by partition, and random bytes
/// from the system source (`/dev/urandom`, read through `std::fs`).
///
/// [`fmt::Debug`] writes the number of keys and the lock state only.
pub struct HostKeys {
    keys: HashMap<Partition, DataKey>,
    source: Box<dyn Read + Send>,
    locked: bool,
}

impl HostKeys {
    /// A provider with no keys that reads `/dev/urandom`.
    ///
    /// # Errors
    ///
    /// [`rivqen_cache::driver::StoreErrorKind::Unavailable`] when the
    /// system source cannot be opened.
    pub fn new() -> Result<Self, StoreError> {
        let source = File::open(SYSTEM_SOURCE).map_err(|_| unavailable("random"))?;
        Ok(Self::with_random_source(Box::new(source)))
    }

    /// A provider with no keys that reads its random bytes from `source`.
    /// For tests: a fixed source gives fixed keys, nonces and file ids; a
    /// source that ends makes the port fail.
    #[must_use]
    pub fn with_random_source(source: Box<dyn Read + Send>) -> Self {
        Self {
            keys: HashMap::new(),
            source,
            locked: false,
        }
    }

    /// Locks or unlocks the provider. A locked provider fails every call
    /// with `Unavailable`, as a key store that the device has locked.
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    /// Fails when the provider is locked.
    fn check_open(&self, op: &'static str) -> Result<(), StoreError> {
        if self.locked {
            Err(unavailable(op))
        } else {
            Ok(())
        }
    }

    /// Fills `buf` from the source.
    fn fill(&mut self, buf: &mut [u8], op: &'static str) -> Result<(), StoreError> {
        self.source.read_exact(buf).map_err(|_| unavailable(op))
    }
}

impl KeyProvider for HostKeys {
    fn data_key(&mut self, partition: Partition) -> Result<DataKey, StoreError> {
        const OP: &str = "data_key";
        self.check_open(OP)?;
        if let Some(key) = self.keys.get(&partition) {
            return Ok(key.duplicate());
        }
        let mut key = DataKey::zeroed();
        // On an error `key` is dropped, which sets the bytes read so far
        // to zero.
        self.fill(key.0.as_mut_slice(), OP)?;
        let copy = key.duplicate();
        self.keys.insert(partition, key);
        Ok(copy)
    }

    fn delete_key(&mut self, partition: Partition) -> Result<(), StoreError> {
        self.check_open("delete_key")?;
        // Dropping the removed key sets its bytes to zero.
        drop(self.keys.remove(&partition));
        Ok(())
    }

    fn random(&mut self, buf: &mut [u8]) -> Result<(), StoreError> {
        const OP: &str = "random";
        self.check_open(OP)?;
        self.fill(buf, OP)
    }
}

impl fmt::Debug for HostKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostKeys")
            .field("keys", &self.keys.len())
            .field("locked", &self.locked)
            .finish_non_exhaustive()
    }
}
