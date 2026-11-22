// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the partition rules (cache identity section 3; RSK-06):
//! every fact that is not known to allow `public` gives the partition of the
//! request, no function ever gives another scoped partition, and a site
//! match is a suffix at a label boundary.
//!
//! Fixed seeds; `PROPTEST_CASES` raises the case count.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_cache::key::Partition;
use rivqen_cache::partition::{
    Credentials, StoreFacts, delta_partition, readable, site_matches, store_partition, valid_host,
};
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

fn credentials() -> impl Strategy<Value = Credentials> {
    prop_oneof![
        Just(Credentials::Absent),
        Just(Credentials::Present),
        Just(Credentials::Unknown),
    ]
}

fn facts() -> impl Strategy<Value = StoreFacts> {
    (any::<[bool; 8]>(), credentials()).prop_map(|(b, credentials)| StoreFacts {
        public_enabled: b[0],
        origin_public: b[1],
        redirected: b[2],
        fields_visible: b[3],
        private: b[4],
        explicit_public: b[5],
        set_cookie: b[6],
        credentials,
        manifest_public: b[7],
    })
}

fn id() -> impl Strategy<Value = PartitionId> {
    any::<[u8; 32]>().prop_map(PartitionId::from_bytes)
}

fn partition() -> impl Strategy<Value = Partition> {
    prop_oneof![Just(Partition::Public), id().prop_map(Partition::Scoped)]
}

/// True when one condition of rule 3 that both a full document and a delta
/// need is false.
fn a_shared_condition_fails(f: &StoreFacts) -> bool {
    !f.public_enabled
        || !f.origin_public
        || f.redirected
        || !f.fields_visible
        || f.private
        || f.set_cookie
        || !f.manifest_public
}

proptest! {
    #![proptest_config(config(1024, 0x0701_0002))]

    #[test]
    fn partition_prop_fail_closed(request in id(), f in facts()) {
        let scoped = Partition::Scoped(request);
        if a_shared_condition_fails(&f) {
            prop_assert_eq!(store_partition(request, &f), scoped);
        }
        // Credentials not known to be absent, and no explicit public.
        if f.credentials != Credentials::Absent && !f.explicit_public {
            prop_assert_eq!(store_partition(request, &f), scoped);
        }
        // Public needs every condition: the converse of the lines above.
        if store_partition(request, &f) == Partition::Public {
            prop_assert!(!a_shared_condition_fails(&f));
            prop_assert!(f.explicit_public || f.credentials == Credentials::Absent);
        }
    }

    #[test]
    fn partition_prop_public_when_every_condition_holds(
        request in id(),
        explicit_public in any::<bool>(),
        credentials in credentials(),
    ) {
        let f = StoreFacts {
            public_enabled: true,
            origin_public: true,
            redirected: false,
            fields_visible: true,
            private: false,
            explicit_public,
            set_cookie: false,
            credentials,
            manifest_public: true,
        };
        let expected = if explicit_public || credentials == Credentials::Absent {
            Partition::Public
        } else {
            Partition::Scoped(request)
        };
        prop_assert_eq!(store_partition(request, &f), expected);
        let delta = if credentials == Credentials::Absent {
            Partition::Public
        } else {
            Partition::Scoped(request)
        };
        prop_assert_eq!(delta_partition(Partition::Public, request, &f), delta);
    }

    #[test]
    fn partition_prop_results_are_public_or_the_request(
        request in id(),
        snapshot in partition(),
        f in facts(),
    ) {
        for result in [store_partition(request, &f), delta_partition(snapshot, request, &f)] {
            prop_assert!(
                result == Partition::Public || result == Partition::Scoped(request)
            );
        }
    }

    #[test]
    fn partition_prop_delta_fail_closed(
        request in id(),
        snapshot in partition(),
        f in facts(),
    ) {
        let result = delta_partition(snapshot, request, &f);
        if result == Partition::Public {
            prop_assert_eq!(snapshot, Partition::Public);
            prop_assert_eq!(f.credentials, Credentials::Absent);
            prop_assert!(!a_shared_condition_fails(&f));
        }
        // The explicit public of a delta never counts.
        let flipped = StoreFacts { explicit_public: !f.explicit_public, ..f };
        prop_assert_eq!(result, delta_partition(snapshot, request, &flipped));
        // A delta is never more public than a full document with the same facts.
        if result == Partition::Public {
            prop_assert_eq!(store_partition(request, &f), Partition::Public);
        }
    }

    #[test]
    fn partition_prop_readable_never_crosses_partitions(
        owner in id(),
        request in id(),
        enabled in any::<bool>(),
    ) {
        prop_assert_eq!(readable(Partition::Scoped(owner), request, enabled), owner == request);
        prop_assert_eq!(readable(Partition::Public, request, enabled), enabled);
    }

    #[test]
    fn partition_prop_site_matches_is_a_label_suffix(
        labels in proptest::collection::vec("[a-c]{1,3}", 1..5),
        prefix in proptest::collection::vec("[a-c]{1,3}", 0..3),
        glued in "[a-c]{1,3}",
    ) {
        let domain = labels.join(".");
        let mut host_labels = prefix.clone();
        host_labels.extend(labels.iter().cloned());
        let host = host_labels.join(".");
        prop_assert!(site_matches(&host, &domain));
        // Text glued to the first label without a dot is another site.
        let glued_host = format!("{glued}{domain}");
        prop_assert!(!site_matches(&glued_host, &domain));
        // A host never matches a longer domain that ends with it.
        if !prefix.is_empty() {
            prop_assert!(!site_matches(&domain, &host));
        }
    }

    #[test]
    fn partition_prop_site_leading_dots_do_not_change_the_match(
        host in "[a-c.]{0,12}",
        domain in "[a-c][a-c.]{0,8}",
        dots in 1_usize..6,
    ) {
        // The cookie `Domain` form with any number of leading dots names the
        // same site (a site clear clears more, never less).
        let dotted = format!("{}{domain}", ".".repeat(dots));
        prop_assert_eq!(site_matches(&host, &dotted), site_matches(&host, &domain));
    }

    #[test]
    fn partition_prop_valid_host_names_are_cleared_by_every_parent(
        labels in proptest::collection::vec("[a-z0-9-]{1,12}", 1..6),
        port in 0_u16..,
    ) {
        let host = labels.join(".");
        prop_assert!(valid_host(&host));
        // A site clear of the host or of any parent domain removes it.
        for start in 0..labels.len() {
            prop_assert!(site_matches(&host, &labels[start..].join(".")));
        }
        // The forms that a clear could miss are refused.
        let refused = [
            format!("{host}:{port}"),
            format!("{host}."),
            format!(".{host}"),
            format!("https://{host}/"),
        ];
        for other in &refused {
            prop_assert!(!valid_host(other), "{:?}", other);
        }
        if host.bytes().any(|b| b.is_ascii_lowercase()) {
            prop_assert!(!valid_host(&host.to_ascii_uppercase()));
        }
    }

    #[test]
    fn partition_prop_valid_host_any_text_never_panics(host in ".{0,64}|[\\[\\]0-9a-fA-F:.]{0,48}") {
        // A valid host is lower-case ASCII of a bounded size, with a colon
        // only inside brackets.
        if valid_host(&host) {
            prop_assert!(host.is_ascii());
            prop_assert!(!host.bytes().any(|b| b.is_ascii_uppercase()));
            prop_assert!(host.len() <= 253);
            if host.contains(':') {
                prop_assert!(host.starts_with('[') && host.ends_with(']'));
                prop_assert!(host.len() <= 41);
            }
        }
    }

    #[test]
    fn partition_prop_site_any_text_never_panics(host in ".{0,16}", domain in "[.a]{0,3}|.{0,16}") {
        // A domain of dots only (also empty) matches nothing.
        if site_matches(&host, &domain) {
            prop_assert!(domain.chars().any(|c| c != '.'));
        }
    }
}
