// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The two plain-text codecs (ADR-011 section 3): the byte layout, the
//! round trip, and the rejection of every input that is not exactly one
//! value within the limits.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_cache::driver::{StoreError, StoreErrorKind};
use rivqen_cache::header::CacheControl;
use rivqen_cache::key::{Partition, Protocol, VaryDigest, VaryEntry};
use rivqen_cache::policy::{EntryPolicy, Expires};
use rivqen_cache::txn::{EntryMeta, SnapshotBody, Validators};
use rivqen_integrity::{PageRevision, Sha256Digest, TemplateRevision, b64u_encode};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::CacheKey;
use rivqen_model::limits::ResourceLimits;
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, MAX_BLOCKS, Mode, Policy, SnapshotMeta,
};
use rivqen_store::crypto::{
    DecodeLimits, FILE_FORMAT, FileContent, ROW_FORMAT, RowContent, decode_file, decode_row,
    encode_file, encode_row,
};

fn page(text: &str) -> PageBytes {
    PageBytes::new(text.as_bytes().to_vec())
}

fn file_error() -> StoreError {
    StoreError::new(StoreErrorKind::Corrupted, "decode_file")
}

fn row_error() -> StoreError {
    StoreError::new(StoreErrorKind::Corrupted, "decode_row")
}

fn limits() -> DecodeLimits {
    DecodeLimits::default()
}

/// A byte string with its length as 8 bytes big-endian.
fn lp(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn u64be(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn legacy_file() -> FileContent {
    FileContent {
        page: page("P!"),
        template: Some(page("T")),
        data: vec![("k".to_owned(), page("vv"))],
    }
}

/// The bytes of `legacy_file`, written by hand from the layout.
fn legacy_file_bytes() -> Vec<u8> {
    let mut out = vec![FILE_FORMAT];
    lp(&mut out, b"P!");
    out.push(1);
    lp(&mut out, b"T");
    u64be(&mut out, 1);
    lp(&mut out, b"k");
    lp(&mut out, b"vv");
    out
}

#[test]
fn crypto_codec_file_layout() {
    assert_eq!(legacy_file().encode(), legacy_file_bytes());
    assert_eq!(
        decode_file(&legacy_file_bytes(), &limits()).unwrap(),
        legacy_file()
    );

    // RQP: no template, no data blocks.
    let rqp = FileContent {
        page: page("<html>"),
        template: None,
        data: Vec::new(),
    };
    let mut expected = vec![FILE_FORMAT];
    lp(&mut expected, b"<html>");
    expected.push(0);
    u64be(&mut expected, 0);
    assert_eq!(rqp.encode(), expected);
    assert_eq!(decode_file(&expected, &limits()).unwrap(), rqp);
}

#[test]
fn crypto_codec_file_has_no_fields() {
    let body = legacy_file().into_body(vec![FieldLine::new("content-type", "text/html")]);
    assert_eq!(body.fields.len(), 1);
    assert_eq!(encode_file(&body), legacy_file_bytes());
    let bare = SnapshotBody {
        fields: Vec::new(),
        ..body.clone()
    };
    assert_eq!(encode_file(&bare), encode_file(&body));
    let back = decode_file(&encode_file(&body), &limits())
        .unwrap()
        .into_body(body.fields.clone());
    assert_eq!(back, body);
}

#[test]
fn crypto_codec_file_debug_has_sizes_only() {
    let text = format!("{:?}", legacy_file());
    assert_eq!(
        text,
        "FileContent { page: PageBytes(len=2), template: Some(PageBytes(len=1)), blocks: 1, .. }"
    );
}

#[test]
fn crypto_codec_file_rejects_every_prefix_and_a_trailing_byte() {
    let bytes = legacy_file_bytes();
    for len in 0..bytes.len() {
        assert_eq!(
            decode_file(&bytes[..len], &limits()),
            Err(file_error()),
            "{len}"
        );
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(decode_file(&longer, &limits()), Err(file_error()));
}

#[test]
fn crypto_codec_file_rejects_another_format() {
    for format in [0, 2, 0xff] {
        let mut bytes = legacy_file_bytes();
        bytes[0] = format;
        assert_eq!(decode_file(&bytes, &limits()), Err(file_error()));
    }
}

#[test]
fn crypto_codec_file_rejects_a_bad_tag() {
    let mut bytes = legacy_file_bytes();
    // The tag of the template: after the format byte and the page.
    let at = 1 + 8 + 2;
    assert_eq!(bytes[at], 1);
    bytes[at] = 2;
    assert_eq!(decode_file(&bytes, &limits()), Err(file_error()));
}

#[test]
fn crypto_codec_file_rejects_a_key_that_is_not_utf8() {
    let mut out = vec![FILE_FORMAT];
    lp(&mut out, b"");
    out.push(0);
    u64be(&mut out, 1);
    lp(&mut out, &[0xff]);
    lp(&mut out, b"");
    assert_eq!(decode_file(&out, &limits()), Err(file_error()));
}

#[test]
fn crypto_codec_file_rejects_lengths_over_the_input() {
    // A page length of u64::MAX, of the remaining input plus one, and a
    // block count larger than the remaining input can hold.
    for len in [u64::MAX, 3] {
        let mut out = vec![FILE_FORMAT];
        u64be(&mut out, len);
        out.extend_from_slice(b"ab");
        assert_eq!(decode_file(&out, &limits()), Err(file_error()), "{len}");
    }
    let mut out = vec![FILE_FORMAT];
    lp(&mut out, b"");
    out.push(0);
    u64be(&mut out, 2);
    lp(&mut out, b"");
    lp(&mut out, b"");
    // Two blocks need 32 bytes; there are 16.
    assert_eq!(decode_file(&out, &limits()), Err(file_error()));
}

/// A file with a page of `page` bytes, a template of `template` bytes and
/// `blocks` blocks with keys of `key` bytes and values of `value` bytes.
fn sized_file(page: usize, template: usize, blocks: usize, key: usize, value: usize) -> Vec<u8> {
    FileContent {
        page: PageBytes::new(vec![b'p'; page]),
        template: Some(PageBytes::new(vec![b't'; template])),
        data: (0..blocks)
            .map(|_| ("k".repeat(key), PageBytes::new(vec![b'v'; value])))
            .collect(),
    }
    .encode()
}

#[test]
fn crypto_codec_file_limits() {
    let small = DecodeLimits {
        max_document_bytes: 10,
        max_blocks: 3,
        max_block_key_bytes: 4,
        max_block_bytes: 5,
        ..DecodeLimits::default()
    };
    // Each value at its limit passes.
    assert!(decode_file(&sized_file(10, 10, 3, 4, 5), &small).is_ok());
    // One more of any of them fails.
    for bytes in [
        sized_file(11, 10, 3, 4, 5),
        sized_file(10, 11, 3, 4, 5),
        sized_file(10, 10, 4, 4, 5),
        sized_file(10, 10, 3, 5, 5),
        sized_file(10, 10, 3, 4, 6),
    ] {
        assert_eq!(decode_file(&bytes, &small), Err(file_error()));
        assert!(decode_file(&bytes, &limits()).is_ok());
    }
}

#[test]
fn crypto_codec_limits_of_the_core() {
    let core = ResourceLimits::default();
    let ours = DecodeLimits::default();
    assert_eq!(ours, DecodeLimits::from_resource_limits(&core));
    assert_eq!(ours.max_document_bytes, core.max_document_bytes);
    assert_eq!(ours.max_blocks, u64::from(core.max_blocks));
    assert_eq!(ours.max_block_key_bytes, 128);
    assert_eq!(ours.max_block_bytes, core.max_block_bytes);
    assert_eq!(ours.max_header_bytes, core.max_header_bytes);
    assert_eq!(ours.max_headers_total_bytes, core.max_headers_total_bytes);
    let rqp_larger = ResourceLimits {
        max_block_key_bytes_legacy: 10,
        max_block_key_bytes_rqp: 20,
        ..core
    };
    assert_eq!(
        DecodeLimits::from_resource_limits(&rqp_larger).max_block_key_bytes,
        20
    );
}

fn sample_policy() -> EntryPolicy {
    EntryPolicy {
        cache_control: CacheControl {
            no_store: false,
            no_cache: true,
            private: false,
            public: true,
            must_revalidate: false,
            max_age: Some(60),
            stale_while_revalidate: None,
            stale_if_error: Some(5),
            invalid_freshness: false,
        },
        expires: Expires::At(UnixTime(1000)),
        date: Some(UnixTime(990)),
        age: None,
        request_time: UnixTime(980),
        response_time: UnixTime(985),
        ttl_seconds: Some(30),
        offline: true,
    }
}

/// The host of the sample rows.
const HOST: &str = "shop.example.com";
/// The store time of the sample rows.
const STORED_AT: u64 = 1_700_000_000;
/// The offset of the protocol byte: the format byte, the host with its
/// length, the store time.
const PROTOCOL_AT: usize = 1 + 8 + HOST.len() + 8;

/// The head of a row plain text up to the protocol byte, by hand.
fn row_head(protocol: u8) -> Vec<u8> {
    let mut out = vec![ROW_FORMAT];
    lp(&mut out, HOST.as_bytes());
    u64be(&mut out, STORED_AT);
    out.push(protocol);
    assert_eq!(out.len(), PROTOCOL_AT + 1);
    out
}

fn sample_row() -> RowContent {
    RowContent {
        host: HOST.to_owned(),
        stored_at: UnixTime(STORED_AT),
        protocol: Protocol::Legacy,
        vary: vec![
            VaryEntry {
                name: "accept-language".to_owned(),
                value: Some(VaryDigest::from_bytes([7; 32])),
            },
            VaryEntry {
                name: "x".to_owned(),
                value: None,
            },
        ],
        validators: Validators {
            etag: Some("abc".to_owned()),
            template_tag: None,
            rqp: None,
        },
        policy: sample_policy(),
        fields: vec![FieldLine::new("content-type", "text/html")],
    }
}

/// The policy part of `sample_policy`, by hand.
fn sample_policy_bytes(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0, 1, 0, 1, 0]);
    out.push(1);
    u64be(out, 60);
    out.push(0);
    out.push(1);
    u64be(out, 5);
    out.push(0);
    out.push(2);
    u64be(out, 1000);
    out.push(1);
    u64be(out, 990);
    out.push(0);
    u64be(out, 980);
    u64be(out, 985);
    out.push(1);
    out.extend_from_slice(&30_u32.to_be_bytes());
    out.push(1);
}

/// The bytes of `sample_row`, written by hand from the layout.
fn sample_row_bytes() -> Vec<u8> {
    let mut out = row_head(0);
    u64be(&mut out, 2);
    lp(&mut out, b"accept-language");
    out.push(1);
    out.extend_from_slice(&[7; 32]);
    lp(&mut out, b"x");
    out.push(0);
    out.push(1);
    lp(&mut out, b"abc");
    out.push(0);
    out.push(0);
    sample_policy_bytes(&mut out);
    u64be(&mut out, 1);
    lp(&mut out, b"content-type");
    lp(&mut out, b"text/html");
    out
}

#[test]
fn crypto_codec_row_layout() {
    assert_eq!(ROW_FORMAT, 2);
    assert_eq!(sample_row().encode(), sample_row_bytes());
    assert_eq!(
        decode_row(&sample_row_bytes(), &limits()).unwrap(),
        sample_row()
    );
}

#[test]
fn crypto_codec_row_of_entry_meta() {
    let row = sample_row();
    let meta = EntryMeta {
        key: CacheKey::from_bytes([3; 32]),
        partition: Partition::Public,
        host: HOST.to_owned(),
        protocol: row.protocol,
        vary: row.vary.clone(),
        validators: row.validators.clone(),
        policy: row.policy,
        stored_at: UnixTime(STORED_AT),
        last_used_at: UnixTime(2),
    };
    // The address and `last_used_at` are not in the row; the host and the
    // store time are.
    assert_eq!(encode_row(&meta, &row.fields), sample_row_bytes());
}

#[test]
fn crypto_codec_row_format_1_does_not_decode() {
    // The format 1 layout had the protocol right after the format byte.
    let mut old = vec![1, 0];
    old.extend_from_slice(&sample_row_bytes()[PROTOCOL_AT + 1..]);
    assert_eq!(decode_row(&old, &limits()), Err(row_error()));
    let mut bytes = sample_row_bytes();
    bytes[0] = 1;
    assert_eq!(decode_row(&bytes, &limits()), Err(row_error()));
}

#[test]
fn crypto_codec_row_host_and_store_time() {
    // The host is at most 253 bytes (`meta::MAX_HOST_BYTES`), with any
    // decode limits; the store time takes every `u64`.
    let mut row = sample_row();
    row.stored_at = UnixTime(u64::MAX);
    row.host = "a".repeat(253);
    assert_eq!(decode_row(&row.encode(), &limits()).unwrap(), row);
    let huge = DecodeLimits {
        max_header_bytes: u64::MAX,
        max_headers_total_bytes: u64::MAX,
        ..limits()
    };
    row.host.push('a');
    assert_eq!(decode_row(&row.encode(), &huge), Err(row_error()));
    // An empty host decodes: the clear column decides what it must be.
    row.host.clear();
    assert_eq!(decode_row(&row.encode(), &limits()).unwrap(), row);
    // A host that is not UTF-8.
    let mut bytes = sample_row_bytes();
    bytes[1 + 8] = 0xff;
    assert_eq!(decode_row(&bytes, &limits()), Err(row_error()));
}

#[test]
fn crypto_codec_row_round_trip_of_each_variant() {
    let mut rqp = sample_row();
    rqp.protocol = Protocol::Rqp;
    rqp.vary.clear();
    rqp.validators = Validators::default();
    rqp.fields.clear();
    rqp.policy.expires = Expires::Invalid;
    rqp.policy.ttl_seconds = None;
    rqp.policy.offline = false;
    let mut flags = sample_row();
    flags.policy.cache_control = CacheControl {
        no_store: true,
        no_cache: false,
        private: true,
        public: false,
        must_revalidate: true,
        max_age: None,
        stale_while_revalidate: Some(u64::MAX),
        stale_if_error: None,
        invalid_freshness: true,
    };
    flags.policy.expires = Expires::Absent;
    flags.policy.date = None;
    flags.policy.age = Some(7);
    flags.validators.etag = None;
    flags.validators.template_tag = Some(String::new());
    for row in [sample_row(), rqp, flags] {
        let bytes = row.encode();
        assert_eq!(decode_row(&bytes, &limits()).unwrap(), row);
    }
}

#[test]
fn crypto_codec_row_rejects_every_prefix_and_a_trailing_byte() {
    let bytes = sample_row_bytes();
    for len in 0..bytes.len() {
        assert_eq!(
            decode_row(&bytes[..len], &limits()),
            Err(row_error()),
            "{len}"
        );
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(decode_row(&longer, &limits()), Err(row_error()));
}

/// `sample_row_bytes` with the byte at `at` set to `value` is rejected.
fn assert_bad_byte(at: usize, value: u8) {
    let mut bytes = sample_row_bytes();
    bytes[at] = value;
    assert_eq!(decode_row(&bytes, &limits()), Err(row_error()), "{at}");
}

#[test]
fn crypto_codec_row_rejects_unknown_tags() {
    // Format, protocol, the digest tag of the first Vary entry.
    assert_bad_byte(0, 0);
    assert_bad_byte(0, 3);
    assert_bad_byte(PROTOCOL_AT, 2);
    let vary_tag = PROTOCOL_AT + 1 + 8 + 8 + 15;
    assert_eq!(sample_row_bytes()[vary_tag], 1);
    assert_bad_byte(vary_tag, 2);
    // The etag option, the rqp option and each boolean and option of the
    // policy: 2 is never valid.
    let etag = vary_tag + 1 + 32 + 8 + 1 + 1;
    assert_eq!(sample_row_bytes()[etag], 1);
    assert_bad_byte(etag, 2);
    let policy = etag + 1 + 8 + 3 + 1 + 1;
    for at in [0, 1, 2, 3, 4, 5, 14, 15, 24] {
        assert_bad_byte(policy + at, 2);
    }
    // The expires tag: 3.
    assert_eq!(sample_row_bytes()[policy + 25], 2);
    assert_bad_byte(policy + 25, 3);
}

#[test]
fn crypto_codec_row_rejects_text_that_is_not_utf8() {
    let mut bytes = sample_row_bytes();
    let at = bytes.len() - 1;
    bytes[at] = 0xff;
    assert_eq!(decode_row(&bytes, &limits()), Err(row_error()));
}

#[test]
fn crypto_codec_row_rejects_counts_over_the_input() {
    let mut bytes = row_head(0);
    u64be(&mut bytes, u64::MAX);
    assert_eq!(decode_row(&bytes, &limits()), Err(row_error()));
    let mut bytes = sample_row_bytes();
    // The field count: 3 lines need at least 48 bytes after it; there
    // are 37.
    let at = bytes.len() - (8 + 12 + 8 + 9) - 8;
    assert_eq!(bytes[at + 7], 1);
    bytes[at + 7] = 3;
    assert_eq!(decode_row(&bytes, &limits()), Err(row_error()));
}

fn row_content_with(vary: &[&str], fields: &[(&str, &str)]) -> RowContent {
    let mut row = sample_row();
    row.vary = vary
        .iter()
        .map(|name| VaryEntry {
            name: (*name).to_owned(),
            value: None,
        })
        .collect();
    row.fields = fields
        .iter()
        .map(|(name, value)| FieldLine::new(*name, *value))
        .collect();
    row
}

/// `row` decodes with `limits` and `fits` says so, or neither.
fn assert_fits(row: &RowContent, limits: &DecodeLimits, expected: bool) {
    assert_eq!(row.fits(limits), expected, "{row:?}");
    assert_eq!(
        decode_row(&row.encode(), limits).is_ok(),
        expected,
        "{row:?}"
    );
}

#[test]
fn crypto_codec_row_limits() {
    let small = DecodeLimits {
        max_header_bytes: 6,
        max_headers_total_bytes: 10,
        ..DecodeLimits::default()
    };
    // At the limits: a field line of 6 bytes, 10 bytes of lines, 10 bytes
    // of Vary names, a validator of 6 bytes.
    assert_fits(
        &row_content_with(&["abcdef", "ghij"], &[("ab", "cdef"), ("a", "b")]),
        &small,
        true,
    );
    for row in [
        // A Vary name of 7 bytes.
        row_content_with(&["abcdefg"], &[]),
        // Vary names of 11 bytes together.
        row_content_with(&["abcdef", "ghijk"], &[]),
        // A field line of 7 bytes: each part is within the limit.
        row_content_with(&[], &[("abc", "defg")]),
        // A name or a value of 7 bytes.
        row_content_with(&[], &[("abcdefg", "")]),
        row_content_with(&[], &[("", "abcdefg")]),
        // Lines of 11 bytes together.
        row_content_with(&[], &[("abc", "def"), ("ab", "cde")]),
    ] {
        assert_fits(&row, &small, false);
        assert_fits(&row, &limits(), true);
    }
    // The etag and the template tag (6 bytes is the limit).
    let mut row = sample_row();
    row.vary.clear();
    row.fields.clear();
    row.validators.etag = Some("abcdef".to_owned());
    assert_fits(&row, &small, true);
    row.validators.etag = Some("abcdefg".to_owned());
    assert_fits(&row, &small, false);
    row.validators.etag = None;
    row.validators.template_tag = Some("abcdef".to_owned());
    assert_fits(&row, &small, true);
    row.validators.template_tag = Some("abcdefg".to_owned());
    assert_fits(&row, &small, false);
}

#[test]
fn crypto_codec_row_fits_host_and_blocks() {
    let mut row = sample_row();
    row.host = "a".repeat(253);
    assert_fits(&row, &limits(), true);
    row.host.push('a');
    assert_fits(&row, &limits(), false);
    // The blocks of an RQP snapshot: the limit of the caller.
    let mut row = rqp_content(BlockFormat::Html, false);
    let one = DecodeLimits {
        max_blocks: 1,
        ..limits()
    };
    assert_fits(&row, &one, true);
    let snapshot = row.validators.rqp.as_mut().unwrap();
    let block = snapshot.blocks[0].clone();
    snapshot.blocks.push(block);
    assert_fits(&row, &one, false);
    assert_fits(&row, &limits(), true);
}

fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::from_b64u(&b64u_encode(&[byte; 32])).unwrap()
}

/// A row whose validators hold an RQP snapshot with `count` blocks, written
/// by hand; `id` and `format` are the bytes of each block id and format.
/// With `members`, the snapshot has `policy` (public, 600 s, offline) and
/// `limits` (65 536 bytes).
fn rqp_row_with(id: &[u8], format: u8, count: u64, members: bool) -> Vec<u8> {
    let mut out = row_head(1);
    u64be(&mut out, 0);
    out.push(0);
    out.push(0);
    out.push(1);
    out.extend_from_slice(&[1; 32]);
    out.extend_from_slice(&[2; 32]);
    u64be(&mut out, count);
    for _ in 0..count {
        lp(&mut out, id);
        out.push(format);
        out.extend_from_slice(&[3; 32]);
    }
    if members {
        out.extend_from_slice(&[1, 1, 1, 1]);
        out.extend_from_slice(&600_u32.to_be_bytes());
        out.extend_from_slice(&[1, 1]);
        out.extend_from_slice(&[1, 1]);
        out.extend_from_slice(&65_536_u32.to_be_bytes());
    } else {
        out.extend_from_slice(&[0, 0]);
    }
    sample_policy_bytes(&mut out);
    u64be(&mut out, 0);
    out
}

fn rqp_row(id: &[u8], format: u8, count: u64) -> Vec<u8> {
    rqp_row_with(id, format, count, false)
}

/// The value of `rqp_row_with(b"main", format, 1, members)`.
fn rqp_content(format: BlockFormat, members: bool) -> RowContent {
    RowContent {
        host: HOST.to_owned(),
        stored_at: UnixTime(STORED_AT),
        protocol: Protocol::Rqp,
        vary: Vec::new(),
        validators: Validators {
            etag: None,
            template_tag: None,
            rqp: Some(SnapshotMeta {
                template_revision: TemplateRevision::from_digest(digest(1)),
                page_revision: PageRevision::from_digest(digest(2)),
                blocks: vec![BlockDescriptor {
                    id: BlockId::parse("main").unwrap(),
                    format,
                    sha256: digest(3),
                }],
                policy: members.then_some(Policy {
                    mode: Some(Mode::Public),
                    ttl_seconds: Some(600),
                    offline: Some(true),
                }),
                limits: members.then_some(Limits {
                    max_patch_bytes: Some(65_536),
                }),
            }),
        },
        policy: sample_policy(),
        fields: Vec::new(),
    }
}

#[test]
fn crypto_codec_row_rqp_layout() {
    for (format, byte) in [(BlockFormat::Html, 0), (BlockFormat::Json, 1)] {
        for members in [false, true] {
            let bytes = rqp_row_with(b"main", byte, 1, members);
            let row = rqp_content(format, members);
            assert_eq!(row.encode(), bytes);
            assert_eq!(decode_row(&bytes, &limits()).unwrap(), row);
        }
    }
}

#[test]
fn crypto_codec_row_rqp_members_of_each_value() {
    let mut row = rqp_content(BlockFormat::Html, true);
    let snapshot = row.validators.rqp.as_mut().unwrap();
    snapshot.policy = Some(Policy {
        mode: Some(Mode::Private),
        ttl_seconds: None,
        offline: Some(false),
    });
    snapshot.limits = Some(Limits {
        max_patch_bytes: None,
    });
    snapshot.blocks.clear();
    assert_eq!(decode_row(&row.encode(), &limits()).unwrap(), row);
    row.validators.rqp.as_mut().unwrap().policy = Some(Policy::default());
    assert_eq!(decode_row(&row.encode(), &limits()).unwrap(), row);
}

#[test]
fn crypto_codec_row_rejects_a_bad_rqp_block() {
    let limits = limits();
    // The same row with a valid block decodes.
    assert!(decode_row(&rqp_row(b"main", 0, 1), &limits).is_ok());
    assert!(decode_row(&rqp_row(b"a-1_b", 1, 1), &limits).is_ok());
    assert!(decode_row(&rqp_row(&[b'a'; 64], 0, 1), &limits).is_ok());
    // A block id that is not valid (markup M-15), too long, not UTF-8.
    for id in [&b"Main"[..], b"-a", b"", &[b'a'; 65], &[0xff]] {
        assert_eq!(decode_row(&rqp_row(id, 0, 1), &limits), Err(row_error()));
    }
    // A block format that is not 00 or 01.
    assert_eq!(
        decode_row(&rqp_row(b"main", 2, 1), &limits),
        Err(row_error())
    );
    // A tag of the mode, the policy or the limits that is not 00 or 01.
    let good = rqp_row_with(b"main", 0, 1, true);
    let policy_at = PROTOCOL_AT + 1 + 8 + 3 + 64 + 8 + (8 + 4 + 1 + 32);
    for (at, value) in [
        (0, 2),
        (1, 2),
        (2, 2),
        (3, 2),
        (8, 2),
        (9, 2),
        (10, 2),
        (11, 2),
    ] {
        let mut bad = good.clone();
        assert_eq!(bad[policy_at + at], 1, "{at}");
        bad[policy_at + at] = value;
        assert_eq!(decode_row(&bad, &limits), Err(row_error()), "{at}");
    }
}

#[test]
fn crypto_codec_row_rqp_block_count_limits() {
    // The limit of the caller.
    let one = DecodeLimits {
        max_blocks: 1,
        ..limits()
    };
    assert!(decode_row(&rqp_row(b"main", 0, 1), &one).is_ok());
    assert_eq!(decode_row(&rqp_row(b"main", 0, 2), &one), Err(row_error()));
    // MAX_BLOCKS of rqp/1 also holds when the caller allows more.
    let many = DecodeLimits {
        max_blocks: 1000,
        ..limits()
    };
    let max = MAX_BLOCKS as u64;
    assert!(decode_row(&rqp_row(b"main", 0, max), &many).is_ok());
    assert_eq!(
        decode_row(&rqp_row(b"main", 0, max + 1), &many),
        Err(row_error())
    );
}

#[test]
fn crypto_codec_empty_items_fill_the_input_exactly() {
    // The last list of each format, with items of the smallest size: the
    // count check allows a count that needs exactly the remaining bytes.
    let file = FileContent {
        page: page(""),
        template: None,
        data: vec![(String::new(), page("")), (String::new(), page(""))],
    };
    assert_eq!(decode_file(&file.encode(), &limits()).unwrap(), file);

    let mut row = sample_row();
    row.fields = vec![FieldLine::new("", ""), FieldLine::new("", "")];
    assert_eq!(decode_row(&row.encode(), &limits()).unwrap(), row);
    row.vary = vec![VaryEntry {
        name: String::new(),
        value: None,
    }];
    row.fields.clear();
    assert_eq!(decode_row(&row.encode(), &limits()).unwrap(), row);
}
