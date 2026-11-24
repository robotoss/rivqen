// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The cache record of a legacy session (WP-08 S2 contract; CL-12, CL-38,
//! CL-40, legacy client section 9.12, H-160).
//!
//! H-160: a URL has one entry in a partition. Its key has no `Vary` entry;
//! the entry stores the `Vary` names of its response with the keyed digests
//! of the request values, and it is visible only to a request with the same
//! values. A commit for other values replaces it.
//!
//! # The value of a `Vary` name (CL-12)
//!
//! The request value of a name is made from the field lines of that name
//! (ASCII case-insensitive): first the lines of
//! [`RequestValues::session_fields`] in their order, then the lines of
//! [`RequestValues::platform_fields`] in their order. Several lines are
//! joined into one value with `", "` (RFC 9110 section 5.3). A name that no
//! line has is absent: its entry has no digest. A line with an empty value
//! is present: an empty value has a digest.
//!
//! # The keyed digest (DL-127, H-147, DL-206)
//!
//! The digest of a present value is HMAC-SHA256 with the 32 bytes of the
//! [`VaryKey`] over this input:
//!
//! ```text
//! len(tag) ‖ tag ‖ len(name) ‖ name ‖ value
//! ```
//!
//! `tag` is the ASCII text `rivqen-vary-v1` (a domain tag: the digest of a
//! value never equals an HMAC of another use of the same key), `name` is the
//! lower-case name, `value` is the exact bytes of the joined value and `len`
//! is a length as 8 bytes big-endian. The name has a length and the value is
//! the last part, so two different `(name, value)` pairs never give one
//! input. An absent value has no digest, so it never equals a present value,
//! also not an empty one.
//!
//! # Resource limits
//!
//! The work is linear in the size of the request fields and of the stored
//! `Vary` entries, plus one sort of the request fields: the request fields
//! are sorted by name once, and each `Vary` name finds its lines with a
//! binary search. The only allocation of the digests is that index (one
//! reference for each request field line). The adapter limits the size of
//! the request and response heads; the store limits the size of an entry.
//!
//! # Freshness inputs of a commit
//!
//! The policy of a commit comes from the response fields with the parsers
//! of `rivqen-cache`: `Cache-Control` ([`parse_cache_control`]), `Age`
//! ([`parse_age`]), `Expires` and `Date` ([`parse_http_date`]). For
//! `Expires` and `Date`, every line of the name must give the same valid
//! date; else `Expires` is not valid (already expired, RFC 9111 section
//! 5.3) and the `Date` is absent (RFC 9111 section 4.2.1: two values make
//! the response stale). Legacy mode has no manifest: no `ttl_seconds`, no
//! `offline`.
//!
//! `Debug` of the types of this module writes no URL, field value, digest,
//! key byte or page byte: only versions, counts and lengths.

use core::cmp::Ordering;
use core::fmt;

use hmac::digest::Key;
use hmac::{Hmac, KeyInit, Mac};
use rivqen_cache::header::{Vary, parse_age, parse_cache_control, parse_http_date, parse_vary};
use rivqen_cache::key::{KeyParts, Partition, Protocol, VaryDigest, VaryEntry, cache_key};
use rivqen_cache::policy::{EntryPolicy, Expires};
use rivqen_cache::txn::{
    CacheTransaction, EntryMeta, EntryVersion, Epoch, Expect, Preconditions, SnapshotBody,
    StoredEntry, TxnOp, Validators, Writer,
};
use rivqen_integrity::legacy::{Sha1Hex, sha1_hex};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;
use rivqen_proto_legacy::fields::{NOT_PAGE_FIELDS, merge_page_fields};
use rivqen_template::legacy::DataBlock;
use rivqen_url::ParsedUrl;
use sha2::Sha256;

use crate::legacy::types::VaryKey;

/// The key of the one entry of `url` in `partition` (H-160): the cache key
/// of identity section 1 with no `Vary` entry, the protocol legacy and no
/// locale.
#[must_use]
pub fn entry_key(partition: PartitionId, url: &ParsedUrl) -> CacheKey {
    let canonical_url = url.cache_url(&[]);
    cache_key(&KeyParts {
        partition: Partition::Scoped(partition),
        origin: url.origin(),
        canonical_url: &canonical_url,
        vary: &[],
        protocol: Protocol::Legacy,
        locale: None,
    })
}

/// The request values that the `Vary` names of a response read (CL-12): the
/// fields of the session request (CL-09) and the fields that the platform
/// adds. The names `accept-diff`, `if-none-match` and `template-tag` do not
/// count.
#[derive(Clone, Copy)]
pub struct RequestValues<'a> {
    /// The legacy request fields of the session, the `cookie` field
    /// included.
    pub session_fields: &'a [FieldLine],
    /// The fields that the platform adds.
    pub platform_fields: &'a [FieldLine],
}

impl fmt::Debug for RequestValues<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestValues")
            .field("session_fields", &self.session_fields.len())
            .field("platform_fields", &self.platform_fields.len())
            .finish()
    }
}

/// A visible entry (CL-38): it passed the `Vary` match (CL-12) and the
/// SHA-1 checks of its page and template (CL-40).
#[derive(Clone, PartialEq, Eq)]
pub struct LegacyRecord {
    /// The version of the stored entry (the precondition of a replace).
    pub version: EntryVersion,
    /// The page.
    pub page: String,
    /// The template.
    pub template: String,
    /// The data blocks in stored order.
    pub data: Vec<DataBlock>,
    /// The page fields (CL-21).
    pub fields: Vec<FieldLine>,
    /// The `Vary` entries of the entry.
    pub vary: Vec<VaryEntry>,
    /// The etag: the SHA-1 of the page.
    pub etag: Sha1Hex,
    /// The template tag: the SHA-1 of the template.
    pub template_tag: Sha1Hex,
}

impl fmt::Debug for LegacyRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyRecord")
            .field("version", &self.version)
            .field("page_len", &self.page.len())
            .field("template_len", &self.template.len())
            .field("data", &self.data.len())
            .field("fields", &self.fields.len())
            .field("vary", &self.vary.len())
            .finish_non_exhaustive()
    }
}

/// The visible record of `entry` for a request with `values` (CL-12,
/// CL-40), or `None`: an entry of another protocol, a `Vary` mismatch, a
/// page or template whose SHA-1 is not its etag or template tag, or text
/// that is not UTF-8. The session removes an entry that fails CL-40.
#[must_use]
pub fn visible(
    entry: &StoredEntry,
    values: &RequestValues<'_>,
    key: &VaryKey,
) -> Option<LegacyRecord> {
    check_visible(entry, values, key).ok()
}

/// Why an entry is not visible, when the session must remove it (CL-40).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotVisible {
    /// The entry is not visible to these request values; it stays.
    OtherVariant,
    /// The entry fails the checks of CL-40; the session removes it.
    Damaged,
}

/// Like [`visible`], with the reason of a `None`.
///
/// The checks, in order; the first one that fails gives the result:
///
/// 1. The protocol of the entry is legacy, and its `Vary` entries have the
///    form that [`vary_entries`] gives (lower-case field names, sorted, no
///    name twice, none of the three legacy names): else
///    [`NotVisible::Damaged`].
/// 2. Each `Vary` entry equals the entry of its name for `values`: the
///    same presence and the same digest. Else
///    [`NotVisible::OtherVariant`].
/// 3. The page, the template and each data value are UTF-8; the etag is the
///    SHA-1 of the page and the template tag is the SHA-1 of the template
///    (CL-40). Else [`NotVisible::Damaged`].
///
/// The page fields of the record are the stored lines after the filter of
/// DL-226 (b) and CL-21 (DL-229 (d)): a stored line that is not a valid
/// field line, or whose name never goes to the web view (`Set-Cookie`, a
/// hop-by-hop field, ...), is not in the record.
///
/// # Errors
///
/// [`NotVisible`] when the entry is not visible.
pub fn check_visible(
    entry: &StoredEntry,
    values: &RequestValues<'_>,
    key: &VaryKey,
) -> Result<LegacyRecord, NotVisible> {
    let meta = &entry.meta;
    if meta.protocol != Protocol::Legacy || !stored_vary_form(&meta.vary) {
        return Err(NotVisible::Damaged);
    }
    let index = RequestIndex::new(values);
    for stored in &meta.vary {
        if !same_value(
            stored.value.as_ref(),
            index.digest(&stored.name, key).as_ref(),
        ) {
            return Err(NotVisible::OtherVariant);
        }
    }

    let body = &entry.body;
    let page = utf8(&body.page)?;
    let template = utf8(body.template.as_ref().ok_or(NotVisible::Damaged)?)?;
    let etag = sha1_hex(page.as_bytes());
    let template_tag = sha1_hex(template.as_bytes());
    if meta.validators.etag.as_deref() != Some(etag.as_str())
        || meta.validators.template_tag.as_deref() != Some(template_tag.as_str())
    {
        return Err(NotVisible::Damaged);
    }
    let data = body
        .data
        .iter()
        .map(|(block_key, value)| {
            Ok(DataBlock {
                key: block_key.clone(),
                value: utf8(value)?,
            })
        })
        .collect::<Result<Vec<_>, NotVisible>>()?;

    Ok(LegacyRecord {
        version: entry.version,
        page,
        template,
        data,
        fields: stored_page_fields(&body.fields),
        vary: meta.vary.clone(),
        etag,
        template_tag,
    })
}

/// The page fields of a stored record as the web view may get them
/// (DL-229 (d)): the filter of DL-226 (b) over the stored lines
/// (`merge_page_fields` with no response line drops a line that is not a
/// valid field line, writes the names in ASCII lowercase and sorts them by
/// name), and no line of a name of `NOT_PAGE_FIELDS` (`Set-Cookie`, the
/// hop-by-hop fields, the legacy control fields, ...). A record that a
/// commit of the session wrote holds only page fields of CL-21, so it does
/// not change. The filter does not read the `sonic-etag-key` and
/// `Connection` lines of the response: the record has no such lines.
pub(crate) fn stored_page_fields(stored: &[FieldLine]) -> Vec<FieldLine> {
    let mut fields = merge_page_fields(stored, &[]);
    fields.retain(|line| !NOT_PAGE_FIELDS.contains(&line.name.as_str()));
    fields
}

/// The `Vary` entries of a commit for a response with `response_fields`
/// (CL-12): the names of every `Vary` line except the three legacy names,
/// in lower case and sorted, each with the keyed digest of the request
/// value, or no value for an absent field. `None` for `Vary: *` (no
/// commit).
///
/// A `Vary` member that is not a field name counts as `*`
/// ([`parse_vary`]): the response is not stored.
#[must_use]
pub fn vary_entries(
    response_fields: &[FieldLine],
    values: &RequestValues<'_>,
    key: &VaryKey,
) -> Option<Vec<VaryEntry>> {
    let names = match parse_vary(response_fields) {
        Vary::Any => return None,
        Vary::None => return Some(Vec::new()),
        Vary::Names(names) => names,
    };
    let index = RequestIndex::new(values);
    Some(
        names
            .into_iter()
            .filter(|name| !is_legacy_request_name(name))
            .map(|name| {
                let value = index.digest(&name, key);
                VaryEntry { name, value }
            })
            .collect(),
    )
}

/// Who writes and what it saw (DL-220, legacy client section 9.12): the
/// request partition of the session and the epochs of its lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteBasis {
    /// The partition of the session.
    pub partition: PartitionId,
    /// The key of the entry (H-160).
    pub key: CacheKey,
    /// The epoch of the partition at the lookup.
    pub partition_epoch: Epoch,
    /// The epoch of the site clear at the lookup.
    pub site_epoch: Epoch,
    /// The state of the entry that the session saw.
    pub expect: Expect,
}

impl WriteBasis {
    /// The preconditions of a write of the session: always a request writer
    /// (DL-220).
    #[must_use]
    pub const fn preconditions(&self) -> Preconditions {
        Preconditions {
            entry: self.expect,
            writer: Writer::request(self.partition, self.partition_epoch),
            partition_epoch: self.partition_epoch,
            site_epoch: self.site_epoch,
        }
    }
}

/// The content of a new record (CL-38) and the inputs of its policy.
#[derive(Clone, Copy)]
pub struct NewRecord<'a> {
    /// The host of the URL (site clear, CL-45).
    pub host: &'a str,
    /// The `Vary` entries ([`vary_entries`]).
    pub vary: &'a [VaryEntry],
    /// The page.
    pub page: &'a str,
    /// The template.
    pub template: &'a str,
    /// The data blocks in order.
    pub data: &'a [DataBlock],
    /// The page fields (CL-21).
    pub fields: &'a [FieldLine],
    /// The etag (the SHA-1 of the page).
    pub etag: Sha1Hex,
    /// The template tag (the SHA-1 of the template).
    pub template_tag: Sha1Hex,
    /// The response field lines (the freshness inputs of the policy).
    pub response_fields: &'a [FieldLine],
    /// When the request was sent.
    pub request_time: UnixTime,
    /// When the response head arrived.
    pub response_time: UnixTime,
}

impl fmt::Debug for NewRecord<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewRecord")
            .field("page_len", &self.page.len())
            .field("template_len", &self.template.len())
            .field("data", &self.data.len())
            .field("fields", &self.fields.len())
            .field("vary", &self.vary.len())
            .finish_non_exhaustive()
    }
}

/// The transaction that stores `record` under the key of `basis` (CL-26):
/// a replace of the entry with the expectation of `basis`. The field update
/// of CL-17 is a commit of the stored record with the merged fields.
///
/// The entry is in the partition of the session, with the protocol legacy,
/// the `Vary` entries, the etag and the template tag of `record`, the
/// policy of its response fields (module documentation), and the response
/// time as the store time and the last use. `record.vary` must be the
/// result of [`vary_entries`]: an entry with other `Vary` entries is never
/// visible ([`check_visible`], step 1).
#[must_use]
pub fn commit(basis: &WriteBasis, record: &NewRecord<'_>) -> CacheTransaction {
    let meta = EntryMeta {
        key: basis.key,
        partition: Partition::Scoped(basis.partition),
        host: record.host.to_owned(),
        protocol: Protocol::Legacy,
        vary: record.vary.to_vec(),
        validators: Validators {
            etag: Some(record.etag.as_str().to_owned()),
            template_tag: Some(record.template_tag.as_str().to_owned()),
            rqp: None,
        },
        policy: policy_of(record),
        stored_at: record.response_time,
        last_used_at: record.response_time,
    };
    let body = SnapshotBody {
        page: text_bytes(record.page),
        template: Some(text_bytes(record.template)),
        data: record
            .data
            .iter()
            .map(|block| (block.key.clone(), text_bytes(&block.value)))
            .collect(),
        fields: record.fields.to_vec(),
    };
    CacheTransaction {
        op: TxnOp::Replace {
            meta: Box::new(meta),
            body: Box::new(body),
        },
        pre: basis.preconditions(),
    }
}

/// The transaction that removes the entry of `basis` (CL-14, CL-17, CL-18,
/// CL-19, CL-27, CL-29, CL-31, CL-40, CL-44).
#[must_use]
pub fn remove(basis: &WriteBasis) -> CacheTransaction {
    CacheTransaction {
        op: TxnOp::Remove {
            partition: Partition::Scoped(basis.partition),
            key: basis.key,
        },
        pre: basis.preconditions(),
    }
}

/// The transaction that disables the key of `basis` until `until` (CL-19).
#[must_use]
pub fn disable(basis: &WriteBasis, until: UnixTime) -> CacheTransaction {
    CacheTransaction {
        op: TxnOp::DisableKey {
            partition: Partition::Scoped(basis.partition),
            key: basis.key,
            until,
        },
        pre: basis.preconditions(),
    }
}

/// The legacy request fields that a `Vary` name never counts (CL-12, P-03).
const LEGACY_REQUEST_NAMES: [&str; 3] = ["accept-diff", "if-none-match", "template-tag"];

/// The domain tag of the HMAC input of a `Vary` digest (module
/// documentation).
const DIGEST_TAG: &[u8] = b"rivqen-vary-v1";

/// The separator of two lines of one name in one value (RFC 9110 section
/// 5.3).
const LINE_SEPARATOR: &[u8] = b", ";

/// HMAC-SHA256.
type HmacSha256 = Hmac<Sha256>;

/// True for `accept-diff`, `if-none-match` and `template-tag`.
fn is_legacy_request_name(name: &str) -> bool {
    LEGACY_REQUEST_NAMES.contains(&name)
}

/// A `tchar` of RFC 9110 section 5.6.2 that is not an upper-case letter.
const fn is_lower_tchar(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
            | b'0'..=b'9'
            | b'a'..=b'z'
    )
}

/// True when `vary` has the form of a result of [`vary_entries`]: each name
/// a lower-case token that is not a legacy request name, the names in
/// strictly increasing byte order.
fn stored_vary_form(vary: &[VaryEntry]) -> bool {
    let names_ok = vary.iter().all(|entry| {
        !entry.name.is_empty()
            && entry.name.bytes().all(is_lower_tchar)
            && !is_legacy_request_name(&entry.name)
    });
    names_ok
        && vary
            .windows(2)
            .all(|pair| matches!(pair, [a, b] if a.name.as_bytes() < b.name.as_bytes()))
}

/// True when the two optional digests are equal: both absent, or both
/// present with the same bytes. The bytes are compared in constant time.
fn same_value(stored: Option<&VaryDigest>, request: Option<&VaryDigest>) -> bool {
    match (stored, request) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let diff = a
                .as_bytes()
                .iter()
                .zip(b.as_bytes())
                .fold(0_u8, |acc, (x, y)| acc | (x ^ y));
            diff == 0
        }
        _ => false,
    }
}

/// The text of stored bytes, or [`NotVisible::Damaged`].
fn utf8(bytes: &PageBytes) -> Result<String, NotVisible> {
    core::str::from_utf8(bytes.as_slice())
        .map(str::to_owned)
        .map_err(|_| NotVisible::Damaged)
}

/// The bytes of a text.
fn text_bytes(text: &str) -> PageBytes {
    PageBytes::new(text.as_bytes().to_vec())
}

/// The order of two field names after ASCII lower case.
fn cmp_lower(a: &str, b: &str) -> Ordering {
    a.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()))
}

/// The request field lines, session fields first, sorted by the lower-case
/// name; lines of one name keep their order (a stable sort).
struct RequestIndex<'a> {
    lines: Vec<&'a FieldLine>,
}

impl<'a> RequestIndex<'a> {
    fn new(values: &RequestValues<'a>) -> Self {
        let mut lines: Vec<&'a FieldLine> = values
            .session_fields
            .iter()
            .chain(values.platform_fields)
            .collect();
        lines.sort_by(|a, b| cmp_lower(&a.name, &b.name));
        Self { lines }
    }

    /// The lines of the lower-case `name`, in request order.
    fn lines_of(&self, name: &str) -> &[&'a FieldLine] {
        let start = self
            .lines
            .partition_point(|line| cmp_lower(&line.name, name) == Ordering::Less);
        let rest = self.lines.get(start..).unwrap_or_default();
        let len = rest.partition_point(|line| cmp_lower(&line.name, name) == Ordering::Equal);
        rest.get(..len).unwrap_or_default()
    }

    /// The keyed digest of the value of the lower-case `name`, or `None`
    /// when the request has no line of that name.
    fn digest(&self, name: &str, key: &VaryKey) -> Option<VaryDigest> {
        let lines = self.lines_of(name);
        if lines.is_empty() {
            return None;
        }
        let mut mac = new_mac(key);
        mac.update(&len_prefix(DIGEST_TAG.len()));
        mac.update(DIGEST_TAG);
        mac.update(&len_prefix(name.len()));
        mac.update(name.as_bytes());
        for (n, line) in lines.iter().enumerate() {
            if n > 0 {
                mac.update(LINE_SEPARATOR);
            }
            mac.update(line.value.as_bytes());
        }
        let bytes: [u8; 32] = mac.finalize().into_bytes().into();
        Some(VaryDigest::from_bytes(bytes))
    }
}

/// HMAC-SHA256 with the 32 bytes of `key`. HMAC pads a key shorter than
/// the block of SHA-256 (64 bytes) with zeros (RFC 2104 section 2), so the
/// padded block gives the HMAC of the 32-byte key, with no error path.
fn new_mac(key: &VaryKey) -> HmacSha256 {
    let mut block = Key::<HmacSha256>::default();
    for (slot, byte) in block.iter_mut().zip(key.0.expose()) {
        *slot = *byte;
    }
    <HmacSha256 as KeyInit>::new(&block)
}

/// `n` as 8 bytes big-endian. Every Rust target has a `usize` of at most 64
/// bits, so the fallback is never used.
fn len_prefix(n: usize) -> [u8; 8] {
    u64::try_from(n).unwrap_or(u64::MAX).to_be_bytes()
}

/// The date of every line of `name`: absent without a line; not valid when
/// a line is not a valid date or two lines differ.
enum DateField {
    Absent,
    Invalid,
    At(UnixTime),
}

fn date_field(fields: &[FieldLine], name: &str) -> DateField {
    let mut found = DateField::Absent;
    for line in fields.iter().filter(|line| line.is(name)) {
        match (parse_http_date(&line.value), &found) {
            (Some(at), DateField::Absent) => found = DateField::At(at),
            (Some(at), DateField::At(first)) if at == *first => {}
            _ => return DateField::Invalid,
        }
    }
    found
}

/// The freshness inputs of a commit (module documentation).
fn policy_of(record: &NewRecord<'_>) -> EntryPolicy {
    let fields = record.response_fields;
    EntryPolicy {
        cache_control: parse_cache_control(fields),
        expires: match date_field(fields, "expires") {
            DateField::Absent => Expires::Absent,
            DateField::Invalid => Expires::Invalid,
            DateField::At(at) => Expires::At(at),
        },
        date: match date_field(fields, "date") {
            DateField::At(at) => Some(at),
            DateField::Absent | DateField::Invalid => None,
        },
        age: parse_age(fields),
        request_time: record.request_time,
        response_time: record.response_time,
        ttl_seconds: None,
        offline: false,
    }
}
