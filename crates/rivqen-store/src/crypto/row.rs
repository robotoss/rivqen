// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The plain text of the `sealed` column of a row (ADR-004 section 3,
//! ADR-011 section 3): what the driver needs only with the key, and the
//! host and the store time of the entry.
//!
//! The host and the store time are also clear columns: the site clear and
//! the quota need them without a key. Their copy under the seal
//! authenticates them: the driver compares both with the clear columns at
//! every open of a row, and a difference is a corrupted entry. The
//! address (partition and key) and the file id are bound through the
//! associated data. `version`, `last_used_at` and `content_bytes` are not
//! in the plain text: the driver compares `content_bytes` with the decoded
//! file, and `version` and `last_used_at` are not authenticated.
//!
//! ```text
//! ROW_FORMAT (u8)     2
//! host                text (at most meta::MAX_HOST_BYTES)
//! stored_at           u64 (the value of the clear column)
//! protocol            u8: 00 legacy, 01 rqp
//! vary                count, then for each entry:
//!   name              text (at most max_header_bytes; all names at most
//!                     max_headers_total_bytes)
//!   digest            option of 32 bytes
//! validators
//!   etag              option of text (at most max_header_bytes)
//!   template_tag      option of text (at most max_header_bytes)
//!   rqp               option of:
//!     template_revision   32 bytes (the SHA-256 digest)
//!     page_revision       32 bytes
//!     blocks              count (at most max_blocks and MAX_BLOCKS), then:
//!       id                text (a block id, markup M-15)
//!       format            u8: 00 html, 01 json
//!       sha256            32 bytes
//!     policy              option of:
//!       mode              option of u8: 00 private, 01 public
//!       ttl_seconds       option of u32
//!       offline           option of bool
//!     limits              option of:
//!       max_patch_bytes   option of u32
//! policy (EntryPolicy)
//!   no_store, no_cache, private, valid_private, public, must_revalidate
//!                     bool each (valid_private only with private)
//!   max_age, stale_while_revalidate, stale_if_error         option of u64 each
//!   invalid_freshness bool
//!   expires           u8: 00 absent, 01 invalid, 02 and u64
//!   date              option of u64
//!   age               option of u64
//!   request_time      u64
//!   response_time     u64
//!   ttl_seconds       option of u32
//!   offline           bool
//! fields              count, then for each field line:
//!   name, value       text each (name and value at most max_header_bytes;
//!                     all lines at most max_headers_total_bytes)
//! ```
//!
//! Only format 2 decodes. [`RowContent::fits`] answers, without an
//! encode, whether the encoding of a value decodes with given limits (and
//! does not set `valid_private` without `private`).

use rivqen_cache::driver::StoreError;
use rivqen_cache::header::CacheControl;
use rivqen_cache::key::{Protocol, VaryDigest, VaryEntry};
use rivqen_cache::policy::{EntryPolicy, Expires};
use rivqen_cache::txn::{EntryMeta, Validators};
use rivqen_integrity::{PageRevision, Sha256Digest, TemplateRevision, b64u_encode};
use rivqen_model::http::FieldLine;
use rivqen_model::time::UnixTime;
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, MAX_BLOCK_ID_BYTES, MAX_BLOCKS, Mode, Policy,
    SnapshotMeta,
};

use super::codec::{Bad, Budget, DecodeLimits, LEN_BYTES, Reader, Writer};
use super::corrupted;
use crate::meta::MAX_HOST_BYTES;

/// The format byte of the row plain text. Format 1 had no host and no
/// store time; it does not decode.
pub const ROW_FORMAT: u8 = 2;

/// The length of a digest in the row.
const DIGEST_BYTES: usize = 32;

/// The sealed part of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowContent {
    /// The host of the entry: the same text as the clear column `host`.
    pub host: String,
    /// When the entry was stored: the same value as the clear column
    /// `stored_at`.
    pub stored_at: UnixTime,
    /// The protocol variant.
    pub protocol: Protocol,
    /// The `Vary` entries of the key.
    pub vary: Vec<VaryEntry>,
    /// The validators.
    pub validators: Validators,
    /// The freshness inputs.
    pub policy: EntryPolicy,
    /// The page fields.
    pub fields: Vec<FieldLine>,
}

impl RowContent {
    /// The plain text of this row content.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        write_row(&self.parts())
    }

    /// True when the plain text of this row content decodes with `limits`:
    /// [`decode_row`] of [`Self::encode`] gives `Ok` exactly when this
    /// gives `true`. One pass over the sizes and the `valid_private` rule,
    /// no encode.
    #[must_use]
    pub fn fits(&self, limits: &DecodeLimits) -> bool {
        check_row(&self.parts(), limits).is_ok()
    }

    fn parts(&self) -> Parts<'_> {
        Parts {
            host: &self.host,
            stored_at: self.stored_at,
            protocol: self.protocol,
            vary: &self.vary,
            validators: &self.validators,
            policy: &self.policy,
            fields: &self.fields,
        }
    }
}

/// The row plain text of `meta` and the page `fields`. The address and
/// `last_used_at` of `meta` are not written; the host and `stored_at` are.
#[must_use]
pub fn encode_row(meta: &EntryMeta, fields: &[FieldLine]) -> Vec<u8> {
    write_row(&Parts {
        host: &meta.host,
        stored_at: meta.stored_at,
        protocol: meta.protocol,
        vary: &meta.vary,
        validators: &meta.validators,
        policy: &meta.policy,
        fields,
    })
}

/// Reads the row plain text.
///
/// # Errors
///
/// [`rivqen_cache::driver::StoreErrorKind::Corrupted`] with no reason when
/// the bytes are not one value of the format: another format byte, an
/// unknown tag, a length or a count over the remaining input or over
/// `limits`, a host over [`MAX_HOST_BYTES`], a text that is not UTF-8, a
/// block id that is not valid, or a trailing byte.
pub fn decode_row(bytes: &[u8], limits: &DecodeLimits) -> Result<RowContent, StoreError> {
    read_row(bytes, limits).map_err(|Bad| corrupted("decode_row"))
}

/// The values of a row plain text, borrowed from a [`RowContent`] or from
/// an [`EntryMeta`] and its fields.
struct Parts<'a> {
    host: &'a str,
    stored_at: UnixTime,
    protocol: Protocol,
    vary: &'a [VaryEntry],
    validators: &'a Validators,
    policy: &'a EntryPolicy,
    fields: &'a [FieldLine],
}

fn write_row(row: &Parts<'_>) -> Vec<u8> {
    let mut w = Writer::with_capacity(256);
    w.u8(ROW_FORMAT);
    w.text(row.host);
    w.u64(row.stored_at.0);
    w.u8(match row.protocol {
        Protocol::Legacy => 0,
        Protocol::Rqp => 1,
    });
    w.len(row.vary.len());
    for entry in row.vary {
        w.text(&entry.name);
        w.option(entry.value.as_ref(), |w, digest| w.raw(digest.as_bytes()));
    }
    w.option(row.validators.etag.as_deref(), Writer::text);
    w.option(row.validators.template_tag.as_deref(), Writer::text);
    w.option(row.validators.rqp.as_ref(), write_snapshot);
    write_policy(&mut w, row.policy);
    w.len(row.fields.len());
    for line in row.fields {
        w.text(&line.name);
        w.text(&line.value);
    }
    w.into_bytes()
}

/// The checks of [`read_row`] on the values: the sizes with the same
/// limits, and `valid_private` only with `private`. Every other part of an
/// encoding always decodes: a text of a `String` is UTF-8, each tag is one
/// that the reader knows, and each count is the number of the items that
/// follow it.
fn check_row(row: &Parts<'_>, limits: &DecodeLimits) -> Result<(), Bad> {
    within(row.host.len(), host_limit()?)?;
    let mut names = Budget::new(limits.max_headers_total_bytes);
    for entry in row.vary {
        within(entry.name.len(), limits.max_header_bytes)?;
        names.take(entry.name.len())?;
    }
    for tag in [&row.validators.etag, &row.validators.template_tag]
        .into_iter()
        .flatten()
    {
        within(tag.len(), limits.max_header_bytes)?;
    }
    if let Some(snapshot) = &row.validators.rqp {
        within(snapshot.blocks.len(), block_limit(limits)?)?;
        for block in &snapshot.blocks {
            let id = block.id.as_str();
            within(id.len(), block_id_limit()?)?;
            BlockId::parse(id).map_err(|_| Bad)?;
        }
    }
    valid_private_holds(&row.policy.cache_control)?;
    let mut total = Budget::new(limits.max_headers_total_bytes);
    for line in row.fields {
        within(line.name.len(), limits.max_header_bytes)?;
        within(line.value.len(), limits.max_header_bytes)?;
        let len = line.name.len().checked_add(line.value.len()).ok_or(Bad)?;
        within(len, limits.max_header_bytes)?;
        total.take(len)?;
    }
    Ok(())
}

/// `Ok` unless `valid_private` is set without `private`: a value that
/// `parse_cache_control` never gives, so it has no encoding.
fn valid_private_holds(cc: &CacheControl) -> Result<(), Bad> {
    if cc.valid_private && !cc.private {
        Err(Bad)
    } else {
        Ok(())
    }
}

/// `Ok` when `len` is at most `limit`.
fn within(len: usize, limit: u64) -> Result<(), Bad> {
    if u64::try_from(len).map_err(|_| Bad)? <= limit {
        Ok(())
    } else {
        Err(Bad)
    }
}

/// [`MAX_HOST_BYTES`] as a limit of the reader.
fn host_limit() -> Result<u64, Bad> {
    u64::try_from(MAX_HOST_BYTES).map_err(|_| Bad)
}

/// [`MAX_BLOCK_ID_BYTES`] as a limit of the reader.
fn block_id_limit() -> Result<u64, Bad> {
    u64::try_from(MAX_BLOCK_ID_BYTES).map_err(|_| Bad)
}

/// The most blocks of an RQP snapshot: the limit of the caller, and at
/// most `MAX_BLOCKS` of rqp/1.
fn block_limit(limits: &DecodeLimits) -> Result<u64, Bad> {
    Ok(limits
        .max_blocks
        .min(u64::try_from(MAX_BLOCKS).map_err(|_| Bad)?))
}

fn write_snapshot(w: &mut Writer, meta: &SnapshotMeta) {
    w.raw(meta.template_revision.digest().as_bytes());
    w.raw(meta.page_revision.digest().as_bytes());
    w.len(meta.blocks.len());
    for block in &meta.blocks {
        w.text(block.id.as_str());
        w.u8(match block.format {
            BlockFormat::Html => 0,
            BlockFormat::Json => 1,
        });
        w.raw(block.sha256.as_bytes());
    }
    w.option(meta.policy.as_ref(), |w, policy| {
        w.option(policy.mode, |w, mode| {
            w.u8(match mode {
                Mode::Private => 0,
                Mode::Public => 1,
            });
        });
        w.option(policy.ttl_seconds, Writer::u32);
        w.option(policy.offline, Writer::bool);
    });
    w.option(meta.limits.as_ref(), |w, limits| {
        w.option(limits.max_patch_bytes, Writer::u32);
    });
}

fn write_policy(w: &mut Writer, policy: &EntryPolicy) {
    let cc = &policy.cache_control;
    w.bool(cc.no_store);
    w.bool(cc.no_cache);
    w.bool(cc.private);
    w.bool(cc.valid_private);
    w.bool(cc.public);
    w.bool(cc.must_revalidate);
    w.option(cc.max_age, Writer::u64);
    w.option(cc.stale_while_revalidate, Writer::u64);
    w.option(cc.stale_if_error, Writer::u64);
    w.bool(cc.invalid_freshness);
    match policy.expires {
        Expires::Absent => w.u8(0),
        Expires::Invalid => w.u8(1),
        Expires::At(time) => {
            w.u8(2);
            w.u64(time.0);
        }
    }
    w.option(policy.date, |w, time| w.u64(time.0));
    w.option(policy.age, Writer::u64);
    w.u64(policy.request_time.0);
    w.u64(policy.response_time.0);
    w.option(policy.ttl_seconds, Writer::u32);
    w.bool(policy.offline);
}

fn read_row(bytes: &[u8], limits: &DecodeLimits) -> Result<RowContent, Bad> {
    let mut r = Reader::new(bytes);
    if r.u8()? != ROW_FORMAT {
        return Err(Bad);
    }
    let host = r.text(host_limit()?)?;
    let stored_at = UnixTime(r.u64()?);
    let protocol = match r.u8()? {
        0 => Protocol::Legacy,
        1 => Protocol::Rqp,
        _ => return Err(Bad),
    };

    // An entry takes at least its name length and its tag.
    let count = r.count(u64::MAX, LEN_BYTES + 1)?;
    let mut names = Budget::new(limits.max_headers_total_bytes);
    let mut vary = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.text(limits.max_header_bytes)?;
        names.take(name.len())?;
        let value = r.option(|r| r.array::<DIGEST_BYTES>().map(VaryDigest::from_bytes))?;
        vary.push(VaryEntry { name, value });
    }

    let etag = r.option(|r| r.text(limits.max_header_bytes))?;
    let template_tag = r.option(|r| r.text(limits.max_header_bytes))?;
    let rqp = r.option(|r| read_snapshot(r, limits))?;
    let policy = read_policy(&mut r)?;

    // A field line takes at least its two lengths.
    let count = r.count(u64::MAX, 2 * LEN_BYTES)?;
    let mut total = Budget::new(limits.max_headers_total_bytes);
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.text(limits.max_header_bytes)?;
        let value = r.text(limits.max_header_bytes)?;
        let line = name.len().checked_add(value.len()).ok_or(Bad)?;
        if u64::try_from(line).map_err(|_| Bad)? > limits.max_header_bytes {
            return Err(Bad);
        }
        total.take(line)?;
        fields.push(FieldLine { name, value });
    }
    r.finish()?;

    Ok(RowContent {
        host,
        stored_at,
        protocol,
        vary,
        validators: Validators {
            etag,
            template_tag,
            rqp,
        },
        policy,
        fields,
    })
}

fn read_snapshot(r: &mut Reader<'_>, limits: &DecodeLimits) -> Result<SnapshotMeta, Bad> {
    let template_revision = TemplateRevision::from_digest(read_digest(r)?);
    let page_revision = PageRevision::from_digest(read_digest(r)?);
    // A block takes at least its id length, its format and its digest.
    let count = r.count(block_limit(limits)?, LEN_BYTES + 1 + DIGEST_BYTES)?;
    let mut blocks = Vec::with_capacity(count);
    for _ in 0..count {
        let id = r.text(block_id_limit()?)?;
        let id = BlockId::parse(&id).map_err(|_| Bad)?;
        let format = match r.u8()? {
            0 => BlockFormat::Html,
            1 => BlockFormat::Json,
            _ => return Err(Bad),
        };
        blocks.push(BlockDescriptor {
            id,
            format,
            sha256: read_digest(r)?,
        });
    }
    let policy = r.option(|r| {
        let mode = r.option(|r| match r.u8()? {
            0 => Ok(Mode::Private),
            1 => Ok(Mode::Public),
            _ => Err(Bad),
        })?;
        let ttl_seconds = r.option(Reader::u32)?;
        let offline = r.option(Reader::bool)?;
        Ok(Policy {
            mode,
            ttl_seconds,
            offline,
        })
    })?;
    let limits = r.option(|r| {
        Ok(Limits {
            max_patch_bytes: r.option(Reader::u32)?,
        })
    })?;
    Ok(SnapshotMeta {
        template_revision,
        page_revision,
        blocks,
        policy,
        limits,
    })
}

/// A SHA-256 digest: 32 bytes. `rivqen-integrity` builds a digest only
/// from its base64url text; every 32 bytes have one, so the error path is
/// not reached.
fn read_digest(r: &mut Reader<'_>) -> Result<Sha256Digest, Bad> {
    let bytes = r.array::<DIGEST_BYTES>()?;
    Sha256Digest::from_b64u(&b64u_encode(&bytes)).map_err(|_| Bad)
}

fn read_policy(r: &mut Reader<'_>) -> Result<EntryPolicy, Bad> {
    let cache_control = CacheControl {
        no_store: r.bool()?,
        no_cache: r.bool()?,
        private: r.bool()?,
        valid_private: r.bool()?,
        public: r.bool()?,
        must_revalidate: r.bool()?,
        max_age: r.option(Reader::u64)?,
        stale_while_revalidate: r.option(Reader::u64)?,
        stale_if_error: r.option(Reader::u64)?,
        invalid_freshness: r.bool()?,
    };
    // The parser never sets `valid_private` without `private`.
    valid_private_holds(&cache_control)?;
    let expires = match r.u8()? {
        0 => Expires::Absent,
        1 => Expires::Invalid,
        2 => Expires::At(UnixTime(r.u64()?)),
        _ => return Err(Bad),
    };
    Ok(EntryPolicy {
        cache_control,
        expires,
        date: r.option(Reader::u64)?.map(UnixTime),
        age: r.option(Reader::u64)?,
        request_time: UnixTime(r.u64()?),
        response_time: UnixTime(r.u64()?),
        ttl_seconds: r.option(Reader::u32)?,
        offline: r.bool()?,
    })
}
