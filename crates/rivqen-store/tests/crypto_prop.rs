// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the codecs and the seal (ADR-011 "Verification": round
//! trip; decoders): every value round-trips; any input that decodes is
//! the one encoding of its value; a sealed value opens with its key and
//! associated data only.
//!
//! Fixed seeds; `PROPTEST_CASES` raises the case count.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_cache::header::CacheControl;
use rivqen_cache::key::{Partition, Protocol, VaryDigest, VaryEntry};
use rivqen_cache::policy::{EntryPolicy, Expires};
use rivqen_cache::txn::Validators;
use rivqen_integrity::{PageRevision, TemplateRevision, sha256};
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, Mode, Policy, SnapshotMeta,
};
use rivqen_store::crypto::{
    Aad, DataKey, DecodeLimits, FileContent, FileId, HostKeys, Role, RowContent, decode_file,
    decode_row, open, seal,
};

fn config(cases: u32, seed: u64) -> ProptestConfig {
    ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(cases),
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn bytes(max: usize) -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..max)
}

fn page(max: usize) -> impl Strategy<Value = PageBytes> {
    bytes(max).prop_map(PageBytes::new)
}

fn text(max: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>(), 0..max).prop_map(|c| c.into_iter().collect())
}

fn file_content() -> impl Strategy<Value = FileContent> {
    (
        page(300),
        proptest::option::of(page(100)),
        proptest::collection::vec((text(8), page(40)), 0..6),
    )
        .prop_map(|(page, template, data)| FileContent {
            page,
            template,
            data,
        })
}

fn cache_control() -> impl Strategy<Value = CacheControl> {
    (
        any::<[bool; 6]>(),
        any::<Option<u64>>(),
        any::<Option<u64>>(),
        any::<Option<u64>>(),
    )
        .prop_map(|(flags, max_age, swr, sie)| CacheControl {
            no_store: flags[0],
            no_cache: flags[1],
            private: flags[2],
            public: flags[3],
            must_revalidate: flags[4],
            max_age,
            stale_while_revalidate: swr,
            stale_if_error: sie,
            invalid_freshness: flags[5],
        })
}

fn policy() -> impl Strategy<Value = EntryPolicy> {
    (
        cache_control(),
        prop_oneof![
            Just(Expires::Absent),
            Just(Expires::Invalid),
            any::<u64>().prop_map(|t| Expires::At(UnixTime(t))),
        ],
        any::<Option<u64>>(),
        any::<Option<u64>>(),
        any::<(u64, u64)>(),
        any::<Option<u32>>(),
        any::<bool>(),
    )
        .prop_map(
            |(cache_control, expires, date, age, (request, response), ttl_seconds, offline)| {
                EntryPolicy {
                    cache_control,
                    expires,
                    date: date.map(UnixTime),
                    age,
                    request_time: UnixTime(request),
                    response_time: UnixTime(response),
                    ttl_seconds,
                    offline,
                }
            },
        )
}

fn snapshot() -> impl Strategy<Value = SnapshotMeta> {
    let block = ("[a-z0-9][a-z0-9_-]{0,63}", any::<bool>(), any::<[u8; 4]>()).prop_map(
        |(id, json, seed)| BlockDescriptor {
            id: BlockId::parse(&id).unwrap_or_else(|_| unreachable!("the pattern is M-15")),
            format: if json {
                BlockFormat::Json
            } else {
                BlockFormat::Html
            },
            sha256: sha256(&seed),
        },
    );
    let policy = (
        proptest::option::of(prop_oneof![Just(Mode::Private), Just(Mode::Public)]),
        any::<Option<u32>>(),
        any::<Option<bool>>(),
    )
        .prop_map(|(mode, ttl_seconds, offline)| Policy {
            mode,
            ttl_seconds,
            offline,
        });
    (
        any::<[u8; 4]>(),
        any::<[u8; 4]>(),
        proptest::collection::vec(block, 0..5),
        proptest::option::of(policy),
        proptest::option::of(
            any::<Option<u32>>().prop_map(|max_patch_bytes| Limits { max_patch_bytes }),
        ),
    )
        .prop_map(|(template, page, blocks, policy, limits)| SnapshotMeta {
            template_revision: TemplateRevision::from_digest(sha256(&template)),
            page_revision: PageRevision::from_digest(sha256(&page)),
            blocks,
            policy,
            limits,
        })
}

/// A host: a short name, or one near the limit of 253 bytes.
fn host() -> impl Strategy<Value = String> {
    prop_oneof!["[a-z0-9.-]{0,24}", "[a-z]{250,256}", text(4)]
}

fn row_content() -> impl Strategy<Value = RowContent> {
    (
        (host(), any::<u64>()),
        prop_oneof![Just(Protocol::Legacy), Just(Protocol::Rqp)],
        proptest::collection::vec(
            (text(10), any::<Option<[u8; 32]>>()).prop_map(|(name, value)| VaryEntry {
                name,
                value: value.map(VaryDigest::from_bytes),
            }),
            0..4,
        ),
        proptest::option::of(text(12)),
        proptest::option::of(text(12)),
        proptest::option::of(snapshot()),
        policy(),
        proptest::collection::vec(
            (text(10), text(20)).prop_map(|(n, v)| FieldLine::new(n, v)),
            0..5,
        ),
    )
        .prop_map(
            |((host, stored_at), protocol, vary, etag, template_tag, rqp, policy, fields)| {
                RowContent {
                    host,
                    stored_at: UnixTime(stored_at),
                    protocol,
                    vary,
                    validators: Validators {
                        etag,
                        template_tag,
                        rqp,
                    },
                    policy,
                    fields,
                }
            },
        )
}

/// Input for a decoder: random bytes, or a valid encoding with one byte
/// changed, cut or extended.
fn damaged(valid: impl Strategy<Value = Vec<u8>>) -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        bytes(200),
        (valid, any::<usize>(), any::<u8>(), 0..3_u8).prop_map(|(mut b, at, value, how)| {
            let at = if b.is_empty() { 0 } else { at % b.len() };
            match how {
                0 if !b.is_empty() => b[at] = value,
                1 => b.truncate(at),
                _ => b.push(value),
            }
            b
        }),
    ]
}

fn small_limits() -> DecodeLimits {
    DecodeLimits {
        max_document_bytes: 64,
        max_blocks: 2,
        max_block_key_bytes: 4,
        max_block_bytes: 16,
        max_header_bytes: 16,
        max_headers_total_bytes: 40,
    }
}

proptest! {
    #![proptest_config(config(512, 0x7203))]

    #[test]
    fn crypto_codec_prop_file_round_trip(content in file_content()) {
        let bytes = content.encode();
        prop_assert_eq!(decode_file(&bytes, &DecodeLimits::default()).ok(), Some(content));
    }

    #[test]
    fn crypto_codec_prop_row_round_trip(row in row_content()) {
        let bytes = row.encode();
        let fits = row.fits(&DecodeLimits::default());
        prop_assert_eq!(decode_row(&bytes, &DecodeLimits::default()).ok(), fits.then_some(row));
    }

    /// What `RowContent::fits` accepts, the decoder accepts; what it
    /// refuses, the decoder refuses (the driver stores only what it can
    /// read back).
    #[test]
    fn crypto_codec_prop_row_fits_iff_it_decodes(row in row_content(), small in any::<bool>()) {
        let limits = if small { small_limits() } else { DecodeLimits::default() };
        prop_assert_eq!(row.fits(&limits), decode_row(&row.encode(), &limits).is_ok());
    }

    #[test]
    fn crypto_codec_prop_file_decode_is_canonical(
        input in damaged(file_content().prop_map(|c| c.encode())),
        small in any::<bool>(),
    ) {
        let limits = if small { small_limits() } else { DecodeLimits::default() };
        if let Ok(content) = decode_file(&input, &limits) {
            prop_assert_eq!(content.encode(), input);
        }
    }

    #[test]
    fn crypto_codec_prop_row_decode_is_canonical(
        input in damaged(row_content().prop_map(|r| r.encode())),
        small in any::<bool>(),
    ) {
        let limits = if small { small_limits() } else { DecodeLimits::default() };
        if let Ok(row) = decode_row(&input, &limits) {
            prop_assert_eq!(row.encode(), input);
        }
    }
}

fn partition() -> impl Strategy<Value = Partition> {
    prop_oneof![
        Just(Partition::Public),
        any::<[u8; 32]>().prop_map(|id| Partition::Scoped(PartitionId::from_bytes(id))),
    ]
}

fn aad() -> impl Strategy<Value = Aad> {
    (
        any::<bool>(),
        any::<u32>(),
        partition(),
        any::<[u8; 32]>(),
        any::<[u8; 16]>(),
    )
        .prop_map(|(file, schema, partition, key, id)| {
            Aad::new(
                if file { Role::File } else { Role::Row },
                schema,
                partition,
                &CacheKey::from_bytes(key),
                &FileId::from_bytes(id),
            )
        })
}

proptest! {
    #![proptest_config(config(128, 0x7204))]

    #[test]
    fn crypto_seal_prop_round_trip(
        mut key_bytes in any::<[u8; 32]>(),
        aad in aad(),
        other in aad(),
        plain in bytes(400),
        flip in any::<(usize, u8)>(),
    ) {
        let key = DataKey::from_bytes(&mut key_bytes);
        let mut port = HostKeys::new().map_err(|e| TestCaseError::fail(e.to_string()))?;
        let sealed = seal(&key, &mut port, &aad, &plain)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(open(&key, &aad, &sealed).ok(), Some(plain));
        if other != aad {
            prop_assert!(open(&key, &other, &sealed).is_err());
        }
        let mut changed = sealed.clone();
        let at = flip.0 % changed.len();
        changed[at] ^= flip.1.max(1);
        prop_assert!(open(&key, &aad, &changed).is_err());
    }
}
