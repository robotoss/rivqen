// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Properties of the policy functions (WP-07 S1 T-03; SEC-06): `no-store`
//! never stores, the use of an entry only gets stricter as time passes, a
//! larger `ttl_seconds` never gives a stricter result, and no input panics
//! (also `u64::MAX` in every time and number).

#![allow(clippy::arithmetic_side_effects)]

use proptest::prelude::*;
use rivqen_cache::header::{CacheControl, Vary};
use rivqen_cache::partition::Credentials;
use rivqen_cache::policy::{
    EntryPolicy, Expires, Failure, SkipReason, StoreDecision, StoreInput, Use, current_age,
    freshness_lifetime, lookup_use, serve_on_failure, store_decision,
};
use rivqen_model::time::UnixTime;

const T: u64 = 1_700_000_000;

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 1024,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// Seconds: small values, the boundaries of the parsers and of `u64`.
fn seconds() -> impl Strategy<Value = u64> {
    prop_oneof![
        0_u64..2_000,
        Just(1_u64 << 31),
        Just(u64::MAX - 1),
        Just(u64::MAX),
        any::<u64>(),
    ]
}

/// A time near `T`, or anywhere.
fn time() -> impl Strategy<Value = UnixTime> {
    prop_oneof![
        4 => (T - 3_000..T + 3_000).prop_map(UnixTime),
        1 => Just(UnixTime(0)),
        1 => Just(UnixTime(u64::MAX)),
        1 => any::<u64>().prop_map(UnixTime),
    ]
}

prop_compose! {
    fn cache_control()(
        flags in prop::array::uniform6(any::<bool>()),
        max_age in prop::option::of(seconds()),
        stale_while_revalidate in prop::option::of(seconds()),
        stale_if_error in prop::option::of(seconds()),
    ) -> CacheControl {
        CacheControl {
            no_store: flags[0],
            no_cache: flags[1],
            private: flags[2],
            public: flags[3],
            must_revalidate: flags[4],
            max_age,
            stale_while_revalidate,
            stale_if_error,
            invalid_freshness: flags[5],
        }
    }
}

fn expires() -> impl Strategy<Value = Expires> {
    prop_oneof![
        Just(Expires::Absent),
        Just(Expires::Invalid),
        time().prop_map(Expires::At),
    ]
}

prop_compose! {
    fn entry_policy()(
        cache_control in cache_control(),
        expires in expires(),
        date in prop::option::of(time()),
        age in prop::option::of(seconds()),
        request_time in time(),
        response_time in time(),
        ttl_seconds in prop::option::of(prop_oneof![0_u32..2_000, Just(31_536_000), any::<u32>()]),
        offline in any::<bool>(),
    ) -> EntryPolicy {
        EntryPolicy {
            cache_control,
            expires,
            date,
            age,
            request_time,
            response_time,
            ttl_seconds,
            offline,
        }
    }
}

fn failure() -> impl Strategy<Value = Failure> {
    prop_oneof![Just(Failure::Network), Just(Failure::Tls)]
}

fn credentials() -> impl Strategy<Value = Credentials> {
    prop_oneof![
        Just(Credentials::Absent),
        Just(Credentials::Present),
        Just(Credentials::Unknown),
    ]
}

fn vary() -> impl Strategy<Value = Vary> {
    prop_oneof![
        Just(Vary::None),
        Just(Vary::Any),
        Just(Vary::Names(vec!["accept-language".to_owned()])),
    ]
}

/// Fresh, then stale-while-revalidate, then revalidate.
fn rank(entry_use: Use) -> u8 {
    match entry_use {
        Use::Fresh => 0,
        Use::StaleWhileRevalidate => 1,
        Use::Revalidate => 2,
    }
}

/// `ttl_seconds` as a bound: `None` is larger than every value.
fn bound(ttl: Option<u32>) -> u64 {
    ttl.map_or(u64::MAX, u64::from)
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn policy_prop_no_input_panics(
        policy in entry_policy(),
        now in time(),
        failure in failure(),
        sensitive in any::<bool>(),
    ) {
        let lifetime = freshness_lifetime(&policy);
        let age = current_age(&policy, now);
        let entry_use = lookup_use(&policy, now);
        let serve = serve_on_failure(&policy, now, failure, sensitive);
        if now < policy.response_time {
            prop_assert_eq!(age, None);
            prop_assert_eq!(entry_use, Use::Revalidate);
            prop_assert!(!serve);
        }
        if entry_use == Use::Fresh {
            prop_assert!(age.is_some_and(|age| age < lifetime));
        }
        if let Some(ttl) = policy.ttl_seconds {
            prop_assert!(lifetime <= u64::from(ttl));
        }
    }

    #[test]
    fn policy_prop_no_store_never_stores(
        status in any::<u16>(),
        mut request in cache_control(),
        mut response in cache_control(),
        in_request in any::<bool>(),
        vary in vary(),
        credentials in credentials(),
        set_cookie in any::<bool>(),
        authenticated_cache in any::<bool>(),
    ) {
        if in_request {
            request.no_store = true;
        } else {
            response.no_store = true;
        }
        let decision = store_decision(&StoreInput {
            status,
            request: &request,
            response: &response,
            vary: &vary,
            credentials,
            set_cookie,
            authenticated_cache,
        });
        prop_assert_eq!(decision, StoreDecision::Skip(SkipReason::NoStore));
    }

    #[test]
    fn policy_prop_store_needs_every_condition(
        status in prop_oneof![Just(200_u16), any::<u16>()],
        request in cache_control(),
        response in cache_control(),
        vary in vary(),
        credentials in credentials(),
        set_cookie in any::<bool>(),
        authenticated_cache in any::<bool>(),
    ) {
        let decision = store_decision(&StoreInput {
            status,
            request: &request,
            response: &response,
            vary: &vary,
            credentials,
            set_cookie,
            authenticated_cache,
        });
        let credentialed = credentials != Credentials::Absent || set_cookie;
        let store = status == 200
            && !request.no_store
            && !response.no_store
            && vary != Vary::Any
            && (!credentialed || authenticated_cache);
        prop_assert_eq!(decision == StoreDecision::Store, store);
    }

    #[test]
    fn policy_prop_use_is_monotonic_in_now(
        policy in entry_policy(),
        first in seconds(),
        more in seconds(),
    ) {
        // From the response time on; before it the entry is stale (DL-183).
        let earlier = policy.response_time.plus(first);
        let later = earlier.plus(more);
        let before = rank(lookup_use(&policy, earlier));
        let after = rank(lookup_use(&policy, later));
        prop_assert!(before <= after, "{} then {}", before, after);
    }

    #[test]
    fn policy_prop_serve_on_failure_ends_and_never_comes_back(
        policy in entry_policy(),
        first in seconds(),
        more in seconds(),
        failure in failure(),
        sensitive in any::<bool>(),
    ) {
        let earlier = policy.response_time.plus(first);
        let later = earlier.plus(more);
        if serve_on_failure(&policy, later, failure, sensitive) {
            prop_assert!(serve_on_failure(&policy, earlier, failure, sensitive));
        }
    }

    #[test]
    fn policy_prop_larger_ttl_never_gives_a_stricter_result(
        policy in entry_policy(),
        small in prop::option::of(any::<u32>()),
        large in prop::option::of(any::<u32>()),
        now in time(),
        failure in failure(),
        sensitive in any::<bool>(),
    ) {
        let (small, large) = if bound(small) <= bound(large) { (small, large) } else { (large, small) };
        let shorter = EntryPolicy { ttl_seconds: small, ..policy };
        let longer = EntryPolicy { ttl_seconds: large, ..policy };
        prop_assert!(freshness_lifetime(&shorter) <= freshness_lifetime(&longer));
        prop_assert!(rank(lookup_use(&longer, now)) <= rank(lookup_use(&shorter, now)));
        if serve_on_failure(&shorter, now, failure, sensitive) {
            prop_assert!(serve_on_failure(&longer, now, failure, sensitive));
        }
    }

    #[test]
    fn policy_prop_directives_that_forbid_reuse(
        policy in entry_policy(),
        now in time(),
        failure in failure(),
        sensitive in any::<bool>(),
    ) {
        let cc = policy.cache_control;
        if cc.no_store || cc.no_cache {
            prop_assert_eq!(lookup_use(&policy, now), Use::Revalidate);
        }
        if cc.no_store || cc.no_cache || cc.must_revalidate || !policy.offline {
            prop_assert!(!serve_on_failure(&policy, now, failure, sensitive));
        }
        if cc.invalid_freshness {
            prop_assert_eq!(freshness_lifetime(&policy), 0);
            prop_assert_ne!(lookup_use(&policy, now), Use::StaleWhileRevalidate);
            prop_assert!(!serve_on_failure(&policy, now, failure, sensitive));
        }
        if failure == Failure::Tls && sensitive {
            prop_assert!(!serve_on_failure(&policy, now, failure, sensitive));
        }
    }
}

#[cfg(feature = "legacy")]
mod legacy {
    use proptest::prelude::*;
    use rivqen_cache::header::CacheOffline;
    use rivqen_cache::policy::legacy::legacy_cache_outcome;
    use rivqen_cache::policy::{SkipReason, StoreDecision};

    fn offline() -> impl Strategy<Value = CacheOffline> {
        prop_oneof![
            Just(CacheOffline::True),
            Just(CacheOffline::False),
            Just(CacheOffline::Store),
            Just(CacheOffline::Http),
        ]
    }

    fn decision() -> impl Strategy<Value = StoreDecision> {
        prop_oneof![
            Just(StoreDecision::Store),
            Just(StoreDecision::Skip(SkipReason::NoStore)),
            Just(StoreDecision::Skip(SkipReason::VaryAny)),
            Just(StoreDecision::Skip(SkipReason::Credentialed)),
            Just(StoreDecision::Skip(SkipReason::Status)),
            Just(StoreDecision::Skip(SkipReason::TransientStatus)),
        ]
    }

    proptest! {
        #![proptest_config(super::config())]

        #[test]
        fn policy_prop_legacy_commit_needs_store(
            offline in offline(),
            store in decision(),
            period in any::<u64>(),
        ) {
            let outcome = legacy_cache_outcome(offline, store, period);
            if outcome.commit {
                prop_assert_eq!(store, StoreDecision::Store);
                prop_assert!(matches!(offline, CacheOffline::True | CacheOffline::Store));
                prop_assert!(!outcome.remove_entry);
            }
            if outcome.disable_key_ms.is_some() {
                prop_assert_eq!(outcome.disable_key_ms, Some(period));
                prop_assert!(outcome.remove_entry);
                prop_assert!(!outcome.commit);
            }
        }
    }
}
