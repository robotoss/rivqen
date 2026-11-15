// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the cache key (cache identity section 1; DL-184; RSK-06):
//! one change of any part changes the key, a shift of bytes between
//! neighbour parts changes the key, equal parts give equal keys, and two
//! partitions never share a key.
//!
//! Fixed seeds; `PROPTEST_CASES` raises the case count.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_cache::key::{KeyParts, Partition, Protocol, VaryDigest, VaryEntry, cache_key};
use rivqen_model::ids::PartitionId;

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

/// Owned key parts, so that a strategy can build them.
#[derive(Debug, Clone, PartialEq)]
struct Owned {
    partition: Partition,
    origin: String,
    url: String,
    vary: Vec<VaryEntry>,
    protocol: Protocol,
    locale: Option<String>,
}

impl Owned {
    fn key(&self) -> rivqen_model::ids::CacheKey {
        cache_key(&KeyParts {
            partition: self.partition,
            origin: &self.origin,
            canonical_url: &self.url,
            vary: &self.vary,
            protocol: self.protocol,
            locale: self.locale.as_deref(),
        })
    }
}

/// Short text with the bytes that a wrong layout would confuse: the tag
/// bytes, digits, separators, and any character.
fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => "[a-c\\x00\\x01\\x02./:?=&]{0,6}",
        1 => any::<String>().prop_map(|s| s.chars().take(8).collect()),
    ]
}

fn partition() -> impl Strategy<Value = Partition> {
    prop_oneof![
        1 => Just(Partition::Public),
        3 => prop_oneof![
            any::<u8>().prop_map(|b| [b; 32]),
            any::<[u8; 32]>(),
        ]
        .prop_map(|b| Partition::Scoped(PartitionId::from_bytes(b))),
    ]
}

fn digest() -> impl Strategy<Value = Option<VaryDigest>> {
    proptest::option::of(any::<u8>().prop_map(|b| VaryDigest::from_bytes([b; 32])))
}

/// Vary entries as the parser gives them: lower-case names, sorted, no
/// name twice.
fn vary() -> impl Strategy<Value = Vec<VaryEntry>> {
    proptest::collection::btree_map("[a-c\\-]{1,4}", digest(), 0..4).prop_map(|map| {
        map.into_iter()
            .map(|(name, value)| VaryEntry { name, value })
            .collect()
    })
}

fn protocol() -> impl Strategy<Value = Protocol> {
    prop_oneof![Just(Protocol::Legacy), Just(Protocol::Rqp)]
}

fn owned() -> impl Strategy<Value = Owned> {
    (
        partition(),
        text(),
        text(),
        vary(),
        protocol(),
        proptest::option::of(text()),
    )
        .prop_map(|(partition, origin, url, vary, protocol, locale)| Owned {
            partition,
            origin,
            url,
            vary,
            protocol,
            locale,
        })
}

/// One part of `base` replaced by the matching part of `other`.
fn with_part(base: &Owned, other: &Owned, part: usize) -> Owned {
    let mut out = base.clone();
    match part {
        0 => out.partition = other.partition,
        1 => out.origin.clone_from(&other.origin),
        2 => out.url.clone_from(&other.url),
        3 => out.vary.clone_from(&other.vary),
        4 => out.protocol = other.protocol,
        _ => out.locale.clone_from(&other.locale),
    }
    out
}

proptest! {
    #![proptest_config(config(512, 0x0701_0001))]

    #[test]
    fn key_prop_same_parts_give_the_same_key(parts in owned()) {
        // A copy in new allocations gives the same key.
        let copy = parts.clone();
        prop_assert_eq!(parts.key(), copy.key());
    }

    #[test]
    fn key_prop_one_changed_part_changes_the_key(
        base in owned(),
        other in owned(),
        part in 0_usize..6,
    ) {
        let changed = with_part(&base, &other, part);
        prop_assume!(changed != base);
        prop_assert_ne!(base.key(), changed.key());
    }

    #[test]
    fn key_prop_origin_and_url_split_differs(
        joined in "[a-c/:]{0,10}",
        i in 0_usize..=10,
        j in 0_usize..=10,
    ) {
        let (i, j) = (i.min(joined.len()), j.min(joined.len()));
        prop_assume!(i != j);
        let at = |n: usize| Owned {
            partition: Partition::Public,
            origin: joined[..n].to_owned(),
            url: joined[n..].to_owned(),
            vary: Vec::new(),
            protocol: Protocol::Rqp,
            locale: None,
        };
        prop_assert_ne!(at(i).key(), at(j).key());
    }

    #[test]
    fn key_prop_vary_name_split_differs(
        joined in "[a-c]{2,8}",
        i in 1_usize..8,
        j in 1_usize..8,
    ) {
        // ("ab","c") and ("a","bc") as the names of two neighbour entries.
        // The key sorts the entries, so a split that gives the same set of
        // names (("a","aa") and ("aa","a")) is the same key by design.
        let (i, j) = (i.min(joined.len() - 1), j.min(joined.len() - 1));
        let names = |n: usize| {
            let mut v = [&joined[..n], &joined[n..]];
            v.sort_unstable();
            v
        };
        prop_assume!(names(i) != names(j));
        let at = |n: usize| Owned {
            partition: Partition::Public,
            origin: String::new(),
            url: String::new(),
            vary: vec![
                VaryEntry { name: joined[..n].to_owned(), value: None },
                VaryEntry { name: joined[n..].to_owned(), value: None },
            ],
            protocol: Protocol::Rqp,
            locale: None,
        };
        prop_assert_ne!(at(i).key(), at(j).key());
    }

    #[test]
    fn key_prop_url_and_locale_cannot_trade_bytes(
        url in text(),
        locale in text(),
        moved in "[a-c]{1,3}",
    ) {
        // Bytes moved from the end of the URL into the locale.
        let a = Owned {
            partition: Partition::Public,
            origin: String::new(),
            url: format!("{url}{moved}"),
            vary: Vec::new(),
            protocol: Protocol::Rqp,
            locale: Some(locale.clone()),
        };
        let b = Owned {
            url,
            locale: Some(format!("{moved}{locale}")),
            ..a.clone()
        };
        prop_assert_ne!(a.key(), b.key());
    }

    #[test]
    fn key_prop_two_partitions_never_share_a_key(a in owned(), b in owned()) {
        // RSK-06: any other parts, also equal ones.
        prop_assume!(a.partition != b.partition);
        prop_assert_ne!(a.key(), b.key());
        let b_same_rest = Owned { partition: b.partition, ..a.clone() };
        prop_assert_ne!(a.key(), b_same_rest.key());
    }

    #[test]
    fn key_prop_vary_order_does_not_count(parts in owned(), rotate in 0_usize..4) {
        let mut shuffled = parts.clone();
        if !shuffled.vary.is_empty() {
            let n = rotate % shuffled.vary.len();
            shuffled.vary.rotate_left(n);
            shuffled.vary.reverse();
        }
        prop_assert_eq!(parts.key(), shuffled.key());
    }
}
