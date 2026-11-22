// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The plain text of an entry file (ADR-011 section 3): the content of one
//! entry version. The page fields are not in the file: they are in the row,
//! because `UpdateFields` changes them and a file is never changed (ADR-004
//! section 1).
//!
//! ```text
//! FILE_FORMAT (u8)
//! page          bytes   (at most max_document_bytes)
//! template      option of bytes (at most max_document_bytes)
//! data          count   (at most max_blocks), then for each block:
//!   key         text    (at most max_block_key_bytes)
//!   value       bytes   (at most max_block_bytes)
//! ```

use std::fmt;

use rivqen_cache::driver::StoreError;
use rivqen_cache::txn::SnapshotBody;
use rivqen_model::http::FieldLine;
use rivqen_model::safe::PageBytes;

use super::codec::{Bad, DecodeLimits, LEN_BYTES, Reader, Writer};
use super::corrupted;

/// The format byte of the file plain text.
pub const FILE_FORMAT: u8 = 1;

/// The content of an entry file: [`SnapshotBody`] without the page fields.
///
/// [`fmt::Debug`] writes sizes only.
#[derive(Clone, PartialEq, Eq)]
pub struct FileContent {
    /// The document as the web view gets it.
    pub page: PageBytes,
    /// The template (legacy) or nothing.
    pub template: Option<PageBytes>,
    /// The data blocks in document order, key and value.
    pub data: Vec<(String, PageBytes)>,
}

impl FileContent {
    /// The plain text of this content.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        write_file(&self.page, self.template.as_ref(), &self.data)
    }

    /// The entry body: this content and the page `fields` of the row.
    #[must_use]
    pub fn into_body(self, fields: Vec<FieldLine>) -> SnapshotBody {
        SnapshotBody {
            page: self.page,
            template: self.template,
            data: self.data,
            fields,
        }
    }
}

impl fmt::Debug for FileContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileContent")
            .field("page", &self.page)
            .field("template", &self.template)
            .field("blocks", &self.data.len())
            .finish_non_exhaustive()
    }
}

/// The file plain text of `body`. `body.fields` is not written.
#[must_use]
pub fn encode_file(body: &SnapshotBody) -> Vec<u8> {
    write_file(&body.page, body.template.as_ref(), &body.data)
}

/// Reads the file plain text.
///
/// # Errors
///
/// [`rivqen_cache::driver::StoreErrorKind::Corrupted`] with no reason when
/// the bytes are not one value of the format: another format byte, a length
/// over the remaining input or over `limits`, a tag that is not `00` or
/// `01`, a key that is not UTF-8, or a trailing byte.
pub fn decode_file(bytes: &[u8], limits: &DecodeLimits) -> Result<FileContent, StoreError> {
    read_file(bytes, limits).map_err(|Bad| corrupted("decode_file"))
}

fn write_file(
    page: &PageBytes,
    template: Option<&PageBytes>,
    data: &[(String, PageBytes)],
) -> Vec<u8> {
    let mut size = page
        .len()
        .saturating_add(template.map_or(0, PageBytes::len))
        .saturating_add(4 * LEN_BYTES);
    for (key, value) in data {
        size = size
            .saturating_add(key.len())
            .saturating_add(value.len())
            .saturating_add(2 * LEN_BYTES);
    }
    let mut w = Writer::with_capacity(size);
    w.u8(FILE_FORMAT);
    w.bytes(page.as_slice());
    w.option(template, |w, template| w.bytes(template.as_slice()));
    w.len(data.len());
    for (key, value) in data {
        w.text(key);
        w.bytes(value.as_slice());
    }
    w.into_bytes()
}

fn read_file(bytes: &[u8], limits: &DecodeLimits) -> Result<FileContent, Bad> {
    let mut r = Reader::new(bytes);
    if r.u8()? != FILE_FORMAT {
        return Err(Bad);
    }
    let page = PageBytes::new(r.bytes(limits.max_document_bytes)?.to_vec());
    let template = r.option(|r| {
        r.bytes(limits.max_document_bytes)
            .map(|bytes| PageBytes::new(bytes.to_vec()))
    })?;
    // A block takes at least its two lengths.
    let count = r.count(limits.max_blocks, 2 * LEN_BYTES)?;
    let mut data = Vec::with_capacity(count);
    for _ in 0..count {
        let key = r.text(limits.max_block_key_bytes)?;
        let value = PageBytes::new(r.bytes(limits.max_block_bytes)?.to_vec());
        data.push((key, value));
    }
    r.finish()?;
    Ok(FileContent {
        page,
        template,
        data,
    })
}
