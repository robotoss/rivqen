// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Column values of the metadata tables: the partition blob, the integer
//! rules and the shape checks of a row (ADR-004 section 3).
//!
//! Every decoder here reads a value that SQLite gives back. A value of the
//! wrong type, length or sign is [`StoreErrorKind::Corrupted`]; nothing here
//! panics or allocates more than the value it reads.

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::time::UnixTime;
use rusqlite::types::ValueRef;

/// The partition column of [`Partition::Public`].
const TAG_PUBLIC: u8 = 0x00;
/// The first byte of the partition column of [`Partition::Scoped`]; the 32
/// id bytes follow.
const TAG_SCOPED: u8 = 0x01;

/// The partition column: `00` for `public`, `01` and the 32 id bytes for a
/// scoped partition.
pub(super) fn partition_bytes(partition: Partition) -> Vec<u8> {
    match partition {
        Partition::Public => vec![TAG_PUBLIC],
        Partition::Scoped(id) => {
            let mut bytes = Vec::with_capacity(33);
            bytes.push(TAG_SCOPED);
            bytes.extend_from_slice(id.as_bytes());
            bytes
        }
    }
}

/// The partition of a partition column.
pub(super) fn decode_partition(
    value: ValueRef<'_>,
    op: &'static str,
) -> Result<Partition, StoreError> {
    let bytes = blob(value, op)?;
    match bytes.split_first() {
        Some((&TAG_PUBLIC, [])) => Ok(Partition::Public),
        Some((&TAG_SCOPED, rest)) => {
            let id: [u8; 32] = rest.try_into().map_err(|_| corrupted(op))?;
            Ok(Partition::Scoped(PartitionId::from_bytes(id)))
        }
        _ => Err(corrupted(op)),
    }
}

/// The cache key of a key column (a blob of 32 bytes).
pub(super) fn decode_key(value: ValueRef<'_>, op: &'static str) -> Result<CacheKey, StoreError> {
    let bytes: [u8; 32] = blob(value, op)?.try_into().map_err(|_| corrupted(op))?;
    Ok(CacheKey::from_bytes(bytes))
}

/// A blob of exactly `N` bytes.
pub(super) fn decode_array<const N: usize>(
    value: ValueRef<'_>,
    op: &'static str,
) -> Result<[u8; N], StoreError> {
    blob(value, op)?.try_into().map_err(|_| corrupted(op))
}

/// A blob of at most `max` bytes, copied.
pub(super) fn decode_blob(
    value: ValueRef<'_>,
    max: usize,
    op: &'static str,
) -> Result<Vec<u8>, StoreError> {
    let bytes = blob(value, op)?;
    if bytes.len() > max {
        return Err(corrupted(op));
    }
    Ok(bytes.to_vec())
}

/// UTF-8 text of at most `max` bytes, copied.
pub(super) fn decode_text(
    value: ValueRef<'_>,
    max: usize,
    op: &'static str,
) -> Result<String, StoreError> {
    let ValueRef::Text(bytes) = value else {
        return Err(corrupted(op));
    };
    if bytes.len() > max {
        return Err(corrupted(op));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| corrupted(op))?;
    Ok(text.to_owned())
}

/// A counter, a version or a size: an integer from 0 to `i64::MAX`.
pub(super) fn decode_u64(value: ValueRef<'_>, op: &'static str) -> Result<u64, StoreError> {
    let ValueRef::Integer(number) = value else {
        return Err(corrupted(op));
    };
    u64::try_from(number).map_err(|_| corrupted(op))
}

/// A time column.
pub(super) fn decode_time(value: ValueRef<'_>, op: &'static str) -> Result<UnixTime, StoreError> {
    decode_u64(value, op).map(UnixTime)
}

/// A counter, a version or a size as a column value. A value above
/// `i64::MAX` does not fit and is refused with
/// [`StoreErrorKind::Unavailable`]: the store cannot write it.
pub(super) fn sql_u64(number: u64, op: &'static str) -> Result<i64, StoreError> {
    i64::try_from(number).map_err(|_| unavailable(op))
}

/// A time as a column value. A time above `i64::MAX` is written as
/// `i64::MAX`: the map keeps the order of every time below that value
/// (about 292 billion years after 1970), so a comparison in SQL gives the
/// same result as one of the `u64` values.
pub(super) fn sql_time(time: UnixTime) -> i64 {
    i64::try_from(time.0).unwrap_or(i64::MAX)
}

/// The bytes of a blob value.
fn blob<'a>(value: ValueRef<'a>, op: &'static str) -> Result<&'a [u8], StoreError> {
    match value {
        ValueRef::Blob(bytes) => Ok(bytes),
        _ => Err(corrupted(op)),
    }
}

/// [`StoreErrorKind::Corrupted`] in `op`.
pub(super) const fn corrupted(op: &'static str) -> StoreError {
    StoreError::new(StoreErrorKind::Corrupted, op)
}

/// [`StoreErrorKind::Unavailable`] in `op`.
pub(super) const fn unavailable(op: &'static str) -> StoreError {
    StoreError::new(StoreErrorKind::Unavailable, op)
}
