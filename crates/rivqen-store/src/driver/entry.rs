// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Reads of entries: the size rules of an entry file, the open of a row and
//! of a file with the associated data of ADR-011, `read` and `list`.

use rivqen_cache::driver::{EntrySummary, StoreError, StoreErrorKind};
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{EntryMeta, SCHEMA_VERSION, SnapshotBody, StoredEntry};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::CacheKey;
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;

use super::layout::{read_entry_file, remove_unnamed};
use super::{FileStore, as_unavailable, corrupted, unavailable};
use crate::crypto::{
    Aad, DataKey, DecodeLimits, FileContent, FileId, KeyProvider, Role, RowContent, SEAL_OVERHEAD,
    decode_file, decode_row, encode_file, open,
};
use crate::meta::{MetaTxn, Row};

const OP_READ: &str = "driver.read";
const OP_LIST: &str = "driver.list";
const OP_OPEN_ROW: &str = "driver.open_row";

/// The lengths that an entry file can have for a content size
/// (`content_bytes` of the row: page, template and data blocks).
///
/// The sealed file is `content + base + template + blocks * block` bytes,
/// where `template` is 0 or the cost of a present template and `blocks` is
/// at most `max_blocks`. The constants come from [`encode_file`] itself, so
/// they follow the codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileShape {
    /// The sealed length of an empty content.
    base: u64,
    /// The extra length of a present template.
    template: u64,
    /// The extra length of one data block.
    block: u64,
    /// The most data blocks that a file decodes with.
    max_blocks: u64,
    /// The largest content that a file decodes with.
    max_content: u64,
}

impl FileShape {
    pub(super) fn new(limits: &DecodeLimits) -> Self {
        let empty = SnapshotBody {
            page: PageBytes::new(Vec::new()),
            template: None,
            data: Vec::new(),
            fields: Vec::new(),
        };
        let with_template = SnapshotBody {
            template: Some(PageBytes::new(Vec::new())),
            ..empty.clone()
        };
        let with_block = SnapshotBody {
            data: vec![(String::new(), PageBytes::new(Vec::new()))],
            ..empty.clone()
        };
        let base = len_u64(encode_file(&empty).len());
        let template = len_u64(encode_file(&with_template).len()).saturating_sub(base);
        let block = len_u64(encode_file(&with_block).len()).saturating_sub(base);
        let per_block = limits
            .max_block_key_bytes
            .saturating_add(limits.max_block_bytes);
        Self {
            base: base.saturating_add(len_u64(SEAL_OVERHEAD)),
            template,
            block,
            max_blocks: limits.max_blocks,
            max_content: limits
                .max_document_bytes
                .saturating_mul(2)
                .saturating_add(limits.max_blocks.saturating_mul(per_block)),
        }
    }

    /// True when a sealed file of `file_len` bytes can hold a content of
    /// `content` bytes (ADR-004 section 4, step 4).
    pub(super) fn fits(&self, file_len: u64, content: u64) -> bool {
        if content > self.max_content {
            return false;
        }
        let Some(extra) = file_len
            .checked_sub(content)
            .and_then(|rest| rest.checked_sub(self.base))
        else {
            return false;
        };
        [0, self.template].into_iter().any(|template| {
            extra.checked_sub(template).is_some_and(|blocks| {
                blocks.checked_rem(self.block) == Some(0)
                    && blocks
                        .checked_div(self.block)
                        .is_some_and(|count| count <= self.max_blocks)
            })
        })
    }
}

/// A `usize` as `u64`; every Rust target has a `usize` of at most 64 bits.
pub(super) fn len_u64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

/// The content size of an entry: page, template and data blocks (the part
/// of `SnapshotBody::size_bytes` that is in the file).
pub(super) fn content_bytes(
    page: &PageBytes,
    template: Option<&PageBytes>,
    data: &[(String, PageBytes)],
) -> u64 {
    let mut total = len_u64(page.len());
    if let Some(template) = template {
        total = total.saturating_add(len_u64(template.len()));
    }
    for (key, value) in data {
        total = total
            .saturating_add(len_u64(key.len()))
            .saturating_add(len_u64(value.len()));
    }
    total
}

/// The size of the page fields (the part of `SnapshotBody::size_bytes`
/// that is in the row).
pub(super) fn field_bytes(fields: &[FieldLine]) -> u64 {
    fields.iter().fold(0_u64, |total, line| {
        total
            .saturating_add(len_u64(line.name.len()))
            .saturating_add(len_u64(line.value.len()))
    })
}

/// The associated data of the row or the file of `row`.
pub(super) fn aad_of(role: Role, partition: Partition, key: &CacheKey, id: &FileId) -> Aad {
    Aad::new(role, SCHEMA_VERSION, partition, key, id)
}

/// True when the content of `body` (page, template, data blocks) decodes
/// from an entry file with `limits`: the size checks of `decode_file` on
/// the values, with no encode. A file of such a content also has a length
/// that [`FileShape::fits`] accepts for its `content_bytes`: the content is
/// at most `max_content`, and the length is the one of its encoding.
pub(super) fn file_fits(body: &SnapshotBody, limits: &DecodeLimits) -> bool {
    let document = |bytes: &PageBytes| len_u64(bytes.len()) <= limits.max_document_bytes;
    document(&body.page)
        && body.template.as_ref().is_none_or(document)
        && len_u64(body.data.len()) <= limits.max_blocks
        && body.data.iter().all(|(key, value)| {
            len_u64(key.len()) <= limits.max_block_key_bytes
                && len_u64(value.len()) <= limits.max_block_bytes
        })
}

/// Opens and decodes the `sealed` column of `row`, and checks that the
/// host and the store time under the seal are the clear columns of `row`
/// (ADR-011 section 3). Every failure is `Corrupted`.
pub(super) fn open_row(
    key: &DataKey,
    row: &Row,
    limits: &DecodeLimits,
) -> Result<RowContent, StoreError> {
    let id = FileId::from_bytes(row.file_id);
    let aad = aad_of(Role::Row, row.partition, &row.key, &id);
    let plain = open(key, &aad, &row.sealed)?;
    let content = decode_row(&plain, limits)?;
    if content.host != row.host || content.stored_at != row.stored_at {
        return Err(corrupted(OP_OPEN_ROW));
    }
    Ok(content)
}

/// The metadata of an entry from its clear columns and its opened row.
pub(super) fn entry_meta(row: &Row, content: RowContent) -> (EntryMeta, Vec<FieldLine>) {
    let meta = EntryMeta {
        key: row.key,
        partition: row.partition,
        host: content.host,
        protocol: content.protocol,
        vary: content.vary,
        validators: content.validators,
        policy: content.policy,
        stored_at: content.stored_at,
        last_used_at: row.last_used_at,
    };
    (meta, content.fields)
}

/// What the metadata says about a key at a read.
enum Lookup {
    /// The key is disabled at the time of the read.
    Hidden,
    /// No row.
    Absent,
    /// A row that does not decode; it was removed in the same transaction.
    Removed,
    /// The row.
    Found(Row),
}

/// The entry of the key in the metadata transaction `txn`.
fn lookup(
    txn: &mut MetaTxn<'_>,
    partition: Partition,
    key: &CacheKey,
    now: UnixTime,
) -> Result<Lookup, StoreError> {
    match txn.disabled_until(partition, key) {
        Ok(Some(until)) if now < until => return Ok(Lookup::Hidden),
        Ok(_) => {}
        // A record that does not decode hides the key (DL-209): a normal
        // load is the safe side.
        Err(error) if error.kind() == StoreErrorKind::Corrupted => return Ok(Lookup::Hidden),
        Err(error) => return Err(error),
    }
    match txn.row(partition, key) {
        Ok(Some(row)) => Ok(Lookup::Found(row)),
        Ok(None) => Ok(Lookup::Absent),
        Err(error) if error.kind() == StoreErrorKind::Corrupted => {
            txn.delete_row(partition, key)?;
            Ok(Lookup::Removed)
        }
        Err(error) => Err(error),
    }
}

impl<P: KeyProvider> FileStore<P> {
    /// `StorageDriver::read`.
    pub(super) fn read_entry(
        &mut self,
        partition: Partition,
        key: &CacheKey,
        now: UnixTime,
    ) -> Result<Option<StoredEntry>, StoreError> {
        let found = self
            .db_mut(OP_READ)?
            .transaction(|txn| lookup(txn, partition, key, now))
            .map_err(as_unavailable)?;
        let row = match found {
            Lookup::Hidden | Lookup::Absent => return Ok(None),
            Lookup::Removed => return Err(corrupted(OP_READ)),
            Lookup::Found(row) => row,
        };
        // The statement selects by both columns; check it again here, so a
        // row of another address is never an answer (RSK-06).
        if row.partition != partition || row.key != *key {
            return self.remove_corrupted(&row);
        }
        // A key port that fails removes nothing: the entry may open later.
        let data_key = self
            .keys
            .data_key(partition)
            .map_err(|error| unavailable(error.op()))?;
        match self.open_entry(&data_key, &row) {
            Ok(entry) => Ok(Some(entry)),
            Err(error) if error.kind() == StoreErrorKind::Corrupted => self.remove_corrupted(&row),
            Err(error) => Err(as_unavailable(error)),
        }
    }

    /// Opens the row and the file of `row` and checks that they belong
    /// together.
    fn open_entry(&self, data_key: &DataKey, row: &Row) -> Result<StoredEntry, StoreError> {
        let content = open_row(data_key, row, &self.limits)?;
        let id = FileId::from_bytes(row.file_id);
        let shape = self.shape;
        let sealed = read_entry_file(
            &self.layout.file_path(&id),
            |len| shape.fits(len, row.content_bytes),
            OP_READ,
        )?;
        let aad = aad_of(Role::File, row.partition, &row.key, &id);
        let plain = open(data_key, &aad, &sealed)?;
        let file: FileContent = decode_file(&plain, &self.limits)?;
        let stored = content_bytes(&file.page, file.template.as_ref(), &file.data);
        if stored != row.content_bytes {
            return Err(corrupted(OP_READ));
        }
        let (meta, fields) = entry_meta(row, content);
        Ok(StoredEntry {
            meta,
            version: row.version,
            body: file.into_body(fields),
        })
    }

    /// Removes the entry of `row` (R-CACHE-06) and gives `Corrupted`; gives
    /// `Unavailable` when the removal of the row fails.
    fn remove_corrupted<T>(&mut self, row: &Row) -> Result<T, StoreError> {
        self.remove_rows(std::slice::from_ref(row), OP_READ)?;
        Err(corrupted(OP_READ))
    }

    /// Removes the rows of `rows` from the metadata, then their files. Only
    /// the metadata transaction can fail the call: once the rows are gone,
    /// a file that cannot be removed is an orphan that recovery removes
    /// ([`remove_unnamed`]).
    fn remove_rows(&mut self, rows: &[Row], op: &'static str) -> Result<(), StoreError> {
        if rows.is_empty() {
            return Ok(());
        }
        self.db_mut(op)?
            .transaction(|txn| {
                for row in rows {
                    txn.delete_row(row.partition, &row.key)?;
                }
                Ok(())
            })
            .map_err(as_unavailable)?;
        for row in rows {
            remove_unnamed(
                &self.layout.file_path(&FileId::from_bytes(row.file_id)),
                false,
            );
        }
        Ok(())
    }

    /// `StorageDriver::list`: every entry, without content. A row that does
    /// not open is removed with its file and is not listed.
    pub(super) fn list_entries(&mut self) -> Result<Vec<EntrySummary>, StoreError> {
        let rows = self
            .db_mut(OP_LIST)?
            .transaction(|txn| match txn.rows() {
                Ok(rows) => Ok(rows),
                Err(error) if error.kind() == StoreErrorKind::Corrupted => {
                    txn.remove_undecodable_rows()?;
                    txn.rows()
                }
                Err(error) => Err(error),
            })
            .map_err(as_unavailable)?;
        let mut keys: Vec<(Partition, DataKey)> = Vec::new();
        let mut out = Vec::with_capacity(rows.len());
        let mut broken = Vec::new();
        for row in rows {
            let known = keys
                .iter()
                .position(|(partition, _)| *partition == row.partition);
            let index = if let Some(index) = known {
                index
            } else {
                // A key port that fails removes nothing.
                let data_key = self
                    .keys
                    .data_key(row.partition)
                    .map_err(|error| unavailable(error.op()))?;
                keys.push((row.partition, data_key));
                keys.len().saturating_sub(1)
            };
            let Some((_, data_key)) = keys.get(index) else {
                return Err(unavailable(OP_LIST));
            };
            match open_row(data_key, &row, &self.limits) {
                Ok(content) => {
                    let (meta, fields) = entry_meta(&row, content);
                    out.push(EntrySummary {
                        meta,
                        version: row.version,
                        size_bytes: row.content_bytes.saturating_add(field_bytes(&fields)),
                    });
                }
                Err(error) if error.kind() == StoreErrorKind::Corrupted => broken.push(row),
                Err(error) => return Err(as_unavailable(error)),
            }
        }
        self.remove_rows(&broken, OP_LIST)?;
        Ok(out)
    }
}
