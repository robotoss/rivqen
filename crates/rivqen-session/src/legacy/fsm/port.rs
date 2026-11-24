// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The cache record functions that a session calls (CL-12, CL-25, CL-38,
//! CL-40; DL-220). The product uses [`RecordPort::RECORD`]: the public
//! functions of `crate::legacy::record`. Unit tests give fakes, so that the
//! paths with a visible entry can be tested before the record lane is
//! integrated.

use rivqen_cache::key::VaryEntry;
use rivqen_cache::txn::{CacheTransaction, StoredEntry};
use rivqen_model::http::FieldLine;

use crate::legacy::record::{self, LegacyRecord, NewRecord, NotVisible, RequestValues, WriteBasis};
use crate::legacy::types::VaryKey;

/// [`record::check_visible`].
pub(crate) type CheckVisible =
    fn(&StoredEntry, &RequestValues<'_>, &VaryKey) -> Result<LegacyRecord, NotVisible>;
/// [`record::vary_entries`].
pub(crate) type VaryEntries =
    fn(&[FieldLine], &RequestValues<'_>, &VaryKey) -> Option<Vec<VaryEntry>>;
/// [`record::commit`].
pub(crate) type Commit = fn(&WriteBasis, &NewRecord<'_>) -> CacheTransaction;

/// The record functions of a session. The removal and the disable of a key
/// always use `record::remove` and `record::disable`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RecordPort {
    /// The visible entry (CL-12, CL-40).
    pub(crate) check_visible: CheckVisible,
    /// The `Vary` entries of a commit (CL-12).
    pub(crate) vary_entries: VaryEntries,
    /// The commit transaction (CL-26, CL-17 field update).
    pub(crate) commit: Commit,
}

impl RecordPort {
    /// The functions of `crate::legacy::record`.
    pub(crate) const RECORD: Self = Self {
        check_visible: record::check_visible,
        vary_entries: record::vary_entries,
        commit: record::commit,
    };
}
