// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The plain text of the `sealed` column of a row (ADR-004 section 3,
//! ADR-011 section 3): what the driver needs only with the key. The
//! address, the host, the version, the file id, the size and the times are
//! clear columns and are not here.
//!
//! ```text
//! ROW_FORMAT (u8)
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
//!   no_store, no_cache, private, public, must_revalidate    bool each
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

use rivqen_cache::driver::StoreError;
use rivqen_cache::header::CacheControl;
use rivqen_cache::key::{Protocol, VaryDigest, VaryEntry};
use rivqen_cache::policy::{EntryPolicy, Expires};
use rivqen_cache::txn::{EntryMeta, Validators};
use rivqen_model::http::FieldLine;
use rivqen_model::time::UnixTime;
use rivqen_proto::types::{
    BlockFormat, BlockId, Limits, MAX_BLOCK_ID_BYTES, MAX_BLOCKS, Mode, Policy, SnapshotMeta,
};

use super::codec::{Bad, Budget, DecodeLimits, LEN_BYTES, Reader, Writer};
use super::corrupted;

/// The format byte of the row plain text.
pub const ROW_FORMAT: u8 = 1;

/// The length of a digest in the row.
const DIGEST_BYTES: usize = 32;

/// The sealed part of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowContent {
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
        write_row(
            self.protocol,
            &self.vary,
            &self.validators,
            &self.policy,
            &self.fields,
        )
    }
}

/// The row plain text of `meta` and the page `fields`. The address, the
/// host and the times of `meta` are not written.
#[must_use]
pub fn encode_row(meta: &EntryMeta, fields: &[FieldLine]) -> Vec<u8> {
    write_row(
        meta.protocol,
        &meta.vary,
        &meta.validators,
        &meta.policy,
        fields,
    )
}

/// Reads the row plain text.
///
/// # Errors
///
/// [`rivqen_cache::driver::StoreErrorKind::Corrupted`] with no reason when
/// the bytes are not one value of the format: another format byte, an
/// unknown tag, a length or a count over the remaining input or over
/// `limits`, a text that is not UTF-8, a block id that is not valid, or a
/// trailing byte.
///
/// Known gap: a row whose validators hold an RQP snapshot is `Corrupted`
/// (see `build_snapshot`).
pub fn decode_row(bytes: &[u8], limits: &DecodeLimits) -> Result<RowContent, StoreError> {
    read_row(bytes, limits).map_err(|Bad| corrupted("decode_row"))
}

fn write_row(
    protocol: Protocol,
    vary: &[VaryEntry],
    validators: &Validators,
    policy: &EntryPolicy,
    fields: &[FieldLine],
) -> Vec<u8> {
    let mut w = Writer::with_capacity(256);
    w.u8(ROW_FORMAT);
    w.u8(match protocol {
        Protocol::Legacy => 0,
        Protocol::Rqp => 1,
    });
    w.len(vary.len());
    for entry in vary {
        w.text(&entry.name);
        w.option(entry.value.as_ref(), |w, digest| w.raw(digest.as_bytes()));
    }
    w.option(validators.etag.as_deref(), Writer::text);
    w.option(validators.template_tag.as_deref(), Writer::text);
    w.option(validators.rqp.as_ref(), write_snapshot);
    write_policy(&mut w, policy);
    w.len(fields.len());
    for line in fields {
        w.text(&line.name);
        w.text(&line.value);
    }
    w.into_bytes()
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
    let rqp = match rqp {
        Some(raw) => Some(build_snapshot(&raw)?),
        None => None,
    };
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

/// The parts of an RQP snapshot as the row holds them.
#[expect(
    dead_code,
    reason = "build_snapshot reads the parts when rivqen-integrity is a dependency (shared file request of T-03)"
)]
struct RawSnapshot {
    template_revision: [u8; DIGEST_BYTES],
    page_revision: [u8; DIGEST_BYTES],
    blocks: Vec<(BlockId, BlockFormat, [u8; DIGEST_BYTES])>,
    policy: Option<Policy>,
    limits: Option<Limits>,
}

fn read_snapshot(r: &mut Reader<'_>, limits: &DecodeLimits) -> Result<RawSnapshot, Bad> {
    let template_revision = r.array::<DIGEST_BYTES>()?;
    let page_revision = r.array::<DIGEST_BYTES>()?;
    let max_blocks = limits
        .max_blocks
        .min(u64::try_from(MAX_BLOCKS).map_err(|_| Bad)?);
    // A block takes at least its id length, its format and its digest.
    let count = r.count(max_blocks, LEN_BYTES + 1 + DIGEST_BYTES)?;
    let mut blocks = Vec::with_capacity(count);
    for _ in 0..count {
        let id = r.text(u64::try_from(MAX_BLOCK_ID_BYTES).map_err(|_| Bad)?)?;
        let id = BlockId::parse(&id).map_err(|_| Bad)?;
        let format = match r.u8()? {
            0 => BlockFormat::Html,
            1 => BlockFormat::Json,
            _ => return Err(Bad),
        };
        blocks.push((id, format, r.array::<DIGEST_BYTES>()?));
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
    Ok(RawSnapshot {
        template_revision,
        page_revision,
        blocks,
        policy,
        limits,
    })
}

/// The snapshot of `raw`.
///
/// Known gap: `SnapshotMeta` holds `rivqen_integrity` types
/// (`TemplateRevision`, `PageRevision`, `Sha256Digest`) that this crate
/// cannot build without a dependency on `rivqen-integrity`, a shared file
/// change. Until then a row with an RQP snapshot does not decode, so the
/// driver treats it as corrupted (fail closed). The format does not change
/// when the dependency is added.
fn build_snapshot(_: &RawSnapshot) -> Result<SnapshotMeta, Bad> {
    Err(Bad)
}

fn read_policy(r: &mut Reader<'_>) -> Result<EntryPolicy, Bad> {
    let cache_control = CacheControl {
        no_store: r.bool()?,
        no_cache: r.bool()?,
        private: r.bool()?,
        public: r.bool()?,
        must_revalidate: r.bool()?,
        max_age: r.option(Reader::u64)?,
        stale_while_revalidate: r.option(Reader::u64)?,
        stale_if_error: r.option(Reader::u64)?,
        invalid_freshness: r.bool()?,
    };
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
