// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Freshness lifetime, current age, lookup use and the stale use after a
//! failure (WP-07 S1 T-03; freshness sections 2 and 3; RFC 9111 sections
//! 4.2.1, 4.2.3, 4.2.4; RFC 5861; DL-182, DL-183, DL-198).
//!
//! Each test starts from an input that passes and changes one thing, so
//! that each comparison and each condition is checked on both sides. The
//! policy matrix (`policy_matrix.rs`) checks the same rules through the
//! parsers; these tests use the fields of `EntryPolicy` directly and also
//! reach values that no parser gives (`u64::MAX`).

#![allow(clippy::arithmetic_side_effects)]

use rivqen_cache::header::CacheControl;
use rivqen_cache::policy::{
    EntryPolicy, Expires, Failure, Use, current_age, freshness_lifetime, lookup_use,
    serve_on_failure,
};
use rivqen_model::time::UnixTime;

/// The response time of every test.
const T: u64 = 1_700_000_000;

fn at(seconds: u64) -> UnixTime {
    UnixTime(seconds)
}

/// A response at `T` with `Date` equal to `T`, no `Age`, no request delay,
/// no `Expires`, no manifest policy and the given `Cache-Control`.
fn policy(cache_control: CacheControl) -> EntryPolicy {
    EntryPolicy {
        cache_control,
        expires: Expires::Absent,
        date: Some(at(T)),
        age: None,
        request_time: at(T),
        response_time: at(T),
        ttl_seconds: None,
        offline: false,
    }
}

fn max_age(seconds: u64) -> CacheControl {
    CacheControl {
        max_age: Some(seconds),
        ..CacheControl::default()
    }
}

/// `max-age=600, stale-while-revalidate=30`.
fn swr_policy() -> EntryPolicy {
    policy(CacheControl {
        stale_while_revalidate: Some(30),
        ..max_age(600)
    })
}

/// `max-age=600, stale-if-error=1200` with `offline: true`: served on a
/// network error up to the age 1800.
fn sie_policy() -> EntryPolicy {
    EntryPolicy {
        offline: true,
        ..policy(CacheControl {
            stale_if_error: Some(1200),
            ..max_age(600)
        })
    }
}

fn serve_network(policy: &EntryPolicy, now: u64) -> bool {
    serve_on_failure(policy, at(now), Failure::Network, false)
}

// ---- Lifetime ------------------------------------------------------------

#[test]
fn policy_lifetime_max_age_is_the_lifetime() {
    assert_eq!(freshness_lifetime(&policy(max_age(600))), 600);
    assert_eq!(freshness_lifetime(&policy(max_age(0))), 0);
}

#[test]
fn policy_lifetime_max_age_wins_over_expires() {
    let p = EntryPolicy {
        expires: Expires::At(at(T + 3600)),
        ..policy(max_age(60))
    };
    assert_eq!(freshness_lifetime(&p), 60);
    let invalid = EntryPolicy {
        expires: Expires::Invalid,
        ..policy(max_age(60))
    };
    assert_eq!(freshness_lifetime(&invalid), 60);
}

#[test]
fn policy_lifetime_expires_minus_date() {
    let p = EntryPolicy {
        expires: Expires::At(at(T + 3600)),
        // The lifetime uses Date, not the response time.
        date: Some(at(T + 100)),
        ..policy(CacheControl::default())
    };
    assert_eq!(freshness_lifetime(&p), 3500);
}

#[test]
fn policy_lifetime_expires_one_second_after_date() {
    let p = EntryPolicy {
        expires: Expires::At(at(T + 1)),
        ..policy(CacheControl::default())
    };
    assert_eq!(freshness_lifetime(&p), 1);
}

#[test]
fn policy_lifetime_expires_not_after_date_is_zero() {
    for expires in [T, T - 1, 0] {
        let p = EntryPolicy {
            expires: Expires::At(at(expires)),
            ..policy(CacheControl::default())
        };
        assert_eq!(freshness_lifetime(&p), 0, "Expires {expires}");
    }
}

#[test]
fn policy_lifetime_expires_without_date_is_zero() {
    let p = EntryPolicy {
        expires: Expires::At(at(T + 3600)),
        date: None,
        ..policy(CacheControl::default())
    };
    assert_eq!(freshness_lifetime(&p), 0);
}

#[test]
fn policy_lifetime_invalid_expires_is_zero() {
    let p = EntryPolicy {
        expires: Expires::Invalid,
        ..policy(CacheControl::default())
    };
    assert_eq!(freshness_lifetime(&p), 0);
}

#[test]
fn policy_lifetime_no_explicit_freshness_is_zero() {
    let p = policy(CacheControl {
        public: true,
        stale_while_revalidate: Some(60),
        stale_if_error: Some(60),
        ..CacheControl::default()
    });
    assert_eq!(freshness_lifetime(&p), 0);
}

#[test]
fn policy_lifetime_invalid_freshness_is_zero() {
    let cc = CacheControl {
        invalid_freshness: true,
        ..max_age(600)
    };
    assert_eq!(freshness_lifetime(&policy(cc)), 0);
    let with_expires = EntryPolicy {
        expires: Expires::At(at(T + 3600)),
        ..policy(CacheControl {
            invalid_freshness: true,
            ..CacheControl::default()
        })
    };
    assert_eq!(freshness_lifetime(&with_expires), 0);
}

#[test]
fn policy_lifetime_ttl_bounds_the_lifetime() {
    let bounded = |ttl| {
        freshness_lifetime(&EntryPolicy {
            ttl_seconds: Some(ttl),
            ..policy(max_age(600))
        })
    };
    assert_eq!(bounded(300), 300);
    assert_eq!(bounded(599), 599);
    assert_eq!(bounded(600), 600);
    assert_eq!(bounded(601), 600);
    assert_eq!(bounded(0), 0);
    assert_eq!(bounded(u32::MAX), 600);
}

#[test]
fn policy_lifetime_ttl_never_creates_a_lifetime() {
    let p = EntryPolicy {
        ttl_seconds: Some(300),
        ..policy(CacheControl::default())
    };
    assert_eq!(freshness_lifetime(&p), 0);
}

#[test]
fn policy_lifetime_large_values_do_not_overflow() {
    let p = EntryPolicy {
        expires: Expires::At(at(u64::MAX)),
        date: Some(at(0)),
        ..policy(CacheControl::default())
    };
    assert_eq!(freshness_lifetime(&p), u64::MAX);
    assert_eq!(freshness_lifetime(&policy(max_age(u64::MAX))), u64::MAX);
}

// ---- Age -----------------------------------------------------------------

#[test]
fn policy_age_is_the_resident_time_without_other_inputs() {
    let p = policy(max_age(600));
    assert_eq!(current_age(&p, at(T)), Some(0));
    assert_eq!(current_age(&p, at(T + 300)), Some(300));
}

#[test]
fn policy_age_none_when_now_is_before_the_response_time() {
    let p = policy(max_age(600));
    assert_eq!(current_age(&p, at(T - 1)), None);
    assert_eq!(current_age(&p, at(0)), None);
}

#[test]
fn policy_age_apparent_age_from_date() {
    let p = EntryPolicy {
        date: Some(at(T - 100)),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&p, at(T + 40)), Some(140));
}

#[test]
fn policy_age_date_after_response_gives_apparent_age_zero() {
    let p = EntryPolicy {
        date: Some(at(T + 50)),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&p, at(T + 100)), Some(100));
}

#[test]
fn policy_age_absent_date_gives_apparent_age_zero() {
    let p = EntryPolicy {
        date: None,
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&p, at(T + 7)), Some(7));
}

#[test]
fn policy_age_field_and_response_delay_add() {
    let p = EntryPolicy {
        age: Some(10),
        request_time: at(T - 5),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&p, at(T)), Some(15));
    assert_eq!(current_age(&p, at(T + 1)), Some(16));
}

#[test]
fn policy_age_response_delay_alone() {
    let p = EntryPolicy {
        request_time: at(T - 4),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&p, at(T + 15)), Some(19));
}

#[test]
fn policy_age_request_after_response_gives_delay_zero() {
    let p = EntryPolicy {
        request_time: at(T + 30),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&p, at(T + 10)), Some(10));
}

#[test]
fn policy_age_initial_age_is_the_larger_value_not_the_sum() {
    // Age 300 is larger than the apparent age 100.
    let by_age = EntryPolicy {
        date: Some(at(T - 100)),
        age: Some(300),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&by_age, at(T + 299)), Some(599));
    // The apparent age 100 is larger than Age 20.
    let by_date = EntryPolicy {
        date: Some(at(T - 100)),
        age: Some(20),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&by_date, at(T + 49)), Some(149));
    // Equal values.
    let equal = EntryPolicy {
        date: Some(at(T - 100)),
        age: Some(100),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&equal, at(T)), Some(100));
}

#[test]
fn policy_age_saturates_at_the_largest_value() {
    let p = EntryPolicy {
        age: Some(u64::MAX),
        request_time: at(T - 5),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&p, at(T)), Some(u64::MAX));
    assert_eq!(current_age(&p, at(T + 1)), Some(u64::MAX));
    let resident = EntryPolicy {
        age: Some(u64::MAX - 1),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&resident, at(T + 1)), Some(u64::MAX));
    assert_eq!(current_age(&resident, at(T + 2)), Some(u64::MAX));
    let at_end = EntryPolicy {
        response_time: at(u64::MAX),
        request_time: at(0),
        date: Some(at(0)),
        ..policy(max_age(600))
    };
    assert_eq!(current_age(&at_end, at(u64::MAX)), Some(u64::MAX));
}

#[test]
fn policy_age_large_age_makes_the_entry_stale() {
    let p = EntryPolicy {
        age: Some(u64::MAX),
        ..swr_policy()
    };
    assert_eq!(lookup_use(&p, at(T), false), Use::Revalidate);
}

// ---- Use -----------------------------------------------------------------

#[test]
fn policy_use_fresh_while_age_is_below_the_lifetime() {
    let p = policy(max_age(600));
    assert_eq!(lookup_use(&p, at(T), false), Use::Fresh);
    assert_eq!(lookup_use(&p, at(T + 599), false), Use::Fresh);
    assert_eq!(lookup_use(&p, at(T + 600), false), Use::Revalidate);
}

#[test]
fn policy_use_lifetime_zero_is_never_fresh() {
    assert_eq!(
        lookup_use(&policy(max_age(0)), at(T), false),
        Use::Revalidate
    );
    assert_eq!(
        lookup_use(&policy(CacheControl::default()), at(T), false),
        Use::Revalidate
    );
}

#[test]
fn policy_use_no_cache_revalidates_a_fresh_entry() {
    let p = policy(CacheControl {
        no_cache: true,
        ..max_age(600)
    });
    assert_eq!(lookup_use(&p, at(T + 10), false), Use::Revalidate);
}

#[test]
fn policy_use_no_store_revalidates_a_fresh_entry() {
    let p = policy(CacheControl {
        no_store: true,
        ..max_age(600)
    });
    assert_eq!(lookup_use(&p, at(T + 10), false), Use::Revalidate);
}

#[test]
fn policy_use_must_revalidate_has_no_effect_while_fresh() {
    let p = policy(CacheControl {
        must_revalidate: true,
        ..max_age(600)
    });
    assert_eq!(lookup_use(&p, at(T + 300), false), Use::Fresh);
}

#[test]
fn policy_use_now_before_response_time_revalidates() {
    assert_eq!(lookup_use(&swr_policy(), at(T - 1), false), Use::Revalidate);
}

#[test]
fn policy_use_stale_window_is_inclusive() {
    let p = swr_policy();
    assert_eq!(lookup_use(&p, at(T + 599), false), Use::Fresh);
    assert_eq!(
        lookup_use(&p, at(T + 600), false),
        Use::StaleWhileRevalidate
    );
    assert_eq!(
        lookup_use(&p, at(T + 630), false),
        Use::StaleWhileRevalidate
    );
    assert_eq!(lookup_use(&p, at(T + 631), false), Use::Revalidate);
}

#[test]
fn policy_use_window_starts_at_the_bounded_lifetime() {
    let p = EntryPolicy {
        ttl_seconds: Some(100),
        ..swr_policy()
    };
    assert_eq!(lookup_use(&p, at(T + 99), false), Use::Fresh);
    assert_eq!(
        lookup_use(&p, at(T + 100), false),
        Use::StaleWhileRevalidate
    );
    assert_eq!(
        lookup_use(&p, at(T + 130), false),
        Use::StaleWhileRevalidate
    );
    assert_eq!(lookup_use(&p, at(T + 131), false), Use::Revalidate);
    assert_eq!(lookup_use(&p, at(T + 610), false), Use::Revalidate);
}

#[test]
fn policy_use_ttl_zero_has_no_window() {
    // Manifest and patch section 1.4: with `0` the client revalidates before
    // it shows the snapshot. The twin with `1` keeps the window.
    let zero = EntryPolicy {
        ttl_seconds: Some(0),
        ..swr_policy()
    };
    assert_eq!(lookup_use(&zero, at(T), false), Use::Revalidate);
    assert_eq!(lookup_use(&zero, at(T + 10), false), Use::Revalidate);
    assert_eq!(lookup_use(&zero, at(T + 30), false), Use::Revalidate);
    let one = EntryPolicy {
        ttl_seconds: Some(1),
        ..swr_policy()
    };
    assert_eq!(lookup_use(&one, at(T), false), Use::Fresh);
    assert_eq!(
        lookup_use(&one, at(T + 10), false),
        Use::StaleWhileRevalidate
    );
    assert_eq!(
        lookup_use(&one, at(T + 31), false),
        Use::StaleWhileRevalidate
    );
    assert_eq!(lookup_use(&one, at(T + 32), false), Use::Revalidate);
}

#[test]
fn policy_use_ttl_zero_does_not_change_serve_on_failure() {
    // The rule of `ttl_seconds` `0` is for the lookup only (rule 6 of
    // freshness section 2 is not changed).
    let zero = EntryPolicy {
        ttl_seconds: Some(0),
        ..sie_policy()
    };
    assert!(serve_network(&zero, T + 10));
    assert!(serve_network(&zero, T + 1200));
    assert!(!serve_network(&zero, T + 1201));
}

#[test]
fn policy_use_sensitive_fresh_stays_fresh() {
    let p = swr_policy();
    assert_eq!(lookup_use(&p, at(T), true), Use::Fresh);
    assert_eq!(lookup_use(&p, at(T + 599), true), Use::Fresh);
    let must = policy(CacheControl {
        must_revalidate: true,
        ..max_age(600)
    });
    assert_eq!(lookup_use(&must, at(T + 300), true), Use::Fresh);
}

#[test]
fn policy_use_sensitive_inside_the_window_revalidates() {
    // Freshness section 3: a stale sensitive page is not shown before the
    // request. The same entry on a route that is not sensitive has the
    // window.
    let p = swr_policy();
    for now in [T + 600, T + 615, T + 630] {
        assert_eq!(lookup_use(&p, at(now), true), Use::Revalidate);
        assert_eq!(lookup_use(&p, at(now), false), Use::StaleWhileRevalidate);
    }
    assert_eq!(lookup_use(&p, at(T + 631), true), Use::Revalidate);
    assert_eq!(lookup_use(&p, at(T - 1), true), Use::Revalidate);
}

#[test]
fn policy_use_window_zero_is_only_the_end_of_the_lifetime() {
    let p = policy(CacheControl {
        stale_while_revalidate: Some(0),
        ..max_age(600)
    });
    assert_eq!(
        lookup_use(&p, at(T + 600), false),
        Use::StaleWhileRevalidate
    );
    assert_eq!(lookup_use(&p, at(T + 601), false), Use::Revalidate);
}

#[test]
fn policy_use_max_age_zero_has_a_window() {
    let p = policy(CacheControl {
        stale_while_revalidate: Some(60),
        ..max_age(0)
    });
    assert_eq!(lookup_use(&p, at(T), false), Use::StaleWhileRevalidate);
    assert_eq!(lookup_use(&p, at(T + 60), false), Use::StaleWhileRevalidate);
    assert_eq!(lookup_use(&p, at(T + 61), false), Use::Revalidate);
}

#[test]
fn policy_use_expires_with_date_has_a_window() {
    let swr = CacheControl {
        stale_while_revalidate: Some(60),
        ..CacheControl::default()
    };
    // Expires equal to Date: explicit lifetime 0.
    let p = EntryPolicy {
        expires: Expires::At(at(T)),
        ..policy(swr)
    };
    assert_eq!(lookup_use(&p, at(T + 10), false), Use::StaleWhileRevalidate);
    // The same Expires without Date: no explicit freshness.
    let no_date = EntryPolicy { date: None, ..p };
    assert_eq!(lookup_use(&no_date, at(T + 10), false), Use::Revalidate);
    // Expires that is not valid: no explicit freshness.
    let invalid = EntryPolicy {
        expires: Expires::Invalid,
        ..p
    };
    assert_eq!(lookup_use(&invalid, at(T + 10), false), Use::Revalidate);
}

#[test]
fn policy_use_no_window_without_explicit_freshness() {
    let p = policy(CacheControl {
        stale_while_revalidate: Some(60),
        ..CacheControl::default()
    });
    assert_eq!(lookup_use(&p, at(T + 30), false), Use::Revalidate);
}

#[test]
fn policy_use_no_window_with_invalid_freshness() {
    let p = policy(CacheControl {
        stale_while_revalidate: Some(60),
        invalid_freshness: true,
        ..max_age(0)
    });
    assert_eq!(lookup_use(&p, at(T + 30), false), Use::Revalidate);
}

#[test]
fn policy_use_no_window_with_no_cache_or_must_revalidate() {
    for cc in [
        CacheControl {
            no_cache: true,
            ..swr_policy().cache_control
        },
        CacheControl {
            must_revalidate: true,
            ..swr_policy().cache_control
        },
    ] {
        assert_eq!(lookup_use(&policy(cc), at(T + 610), false), Use::Revalidate);
    }
}

#[test]
fn policy_use_stale_if_error_and_offline_do_not_change_a_lookup() {
    let p = sie_policy();
    assert_eq!(lookup_use(&p, at(T + 700), false), Use::Revalidate);
}

#[test]
fn policy_use_window_sum_saturates() {
    let p = policy(CacheControl {
        stale_while_revalidate: Some(u64::MAX),
        ..max_age(600)
    });
    assert_eq!(
        lookup_use(&p, at(u64::MAX), false),
        Use::StaleWhileRevalidate,
        "lifetime plus window is larger than every age"
    );
}

// ---- Failure ---------------------------------------------------------------

#[test]
fn policy_failure_serves_inside_stale_if_error() {
    let p = sie_policy();
    assert!(serve_network(&p, T));
    assert!(serve_network(&p, T + 900));
    assert!(serve_network(&p, T + 1800));
    assert!(!serve_network(&p, T + 1801));
}

#[test]
fn policy_failure_needs_offline() {
    let p = EntryPolicy {
        offline: false,
        ..sie_policy()
    };
    assert!(!serve_network(&p, T + 900));
}

#[test]
fn policy_failure_needs_stale_if_error() {
    let p = EntryPolicy {
        offline: true,
        ..policy(max_age(600))
    };
    assert!(!serve_network(&p, T + 900));
    assert!(!serve_network(&p, T + 10), "also while fresh");
}

#[test]
fn policy_failure_tls_on_a_sensitive_route_never_serves() {
    let p = sie_policy();
    assert!(!serve_on_failure(&p, at(T + 900), Failure::Tls, true));
    assert!(serve_on_failure(&p, at(T + 900), Failure::Tls, false));
    assert!(serve_on_failure(&p, at(T + 900), Failure::Network, true));
    assert!(serve_on_failure(&p, at(T + 900), Failure::Network, false));
}

#[test]
fn policy_failure_no_serve_with_no_cache_must_revalidate_or_no_store() {
    let base = sie_policy();
    for cc in [
        CacheControl {
            no_cache: true,
            ..base.cache_control
        },
        CacheControl {
            must_revalidate: true,
            ..base.cache_control
        },
        CacheControl {
            no_store: true,
            ..base.cache_control
        },
    ] {
        let p = EntryPolicy {
            cache_control: cc,
            ..base
        };
        assert!(!serve_network(&p, T + 900), "{cc:?}");
    }
}

#[test]
fn policy_failure_needs_explicit_freshness() {
    let no_lifetime = EntryPolicy {
        cache_control: CacheControl {
            stale_if_error: Some(1200),
            ..CacheControl::default()
        },
        ..sie_policy()
    };
    assert!(!serve_network(&no_lifetime, T + 900));
    let invalid = EntryPolicy {
        cache_control: CacheControl {
            invalid_freshness: true,
            ..sie_policy().cache_control
        },
        ..sie_policy()
    };
    assert!(!serve_network(&invalid, T + 10));
    let by_expires = EntryPolicy {
        expires: Expires::At(at(T + 600)),
        ..no_lifetime
    };
    assert!(serve_network(&by_expires, T + 1800));
    assert!(!serve_network(&by_expires, T + 1801));
}

#[test]
fn policy_failure_now_before_response_time_never_serves() {
    let p = sie_policy();
    assert!(!serve_network(&p, T - 1));
    assert!(serve_network(&p, T));
}

#[test]
fn policy_failure_window_starts_at_the_bounded_lifetime() {
    let p = EntryPolicy {
        ttl_seconds: Some(100),
        cache_control: CacheControl {
            stale_if_error: Some(30),
            ..max_age(600)
        },
        ..sie_policy()
    };
    assert!(serve_network(&p, T + 130));
    assert!(!serve_network(&p, T + 131));
}

#[test]
fn policy_failure_age_counts_in_the_staleness() {
    let p = EntryPolicy {
        age: Some(300),
        ..sie_policy()
    };
    assert!(serve_network(&p, T + 1500));
    assert!(!serve_network(&p, T + 1501));
}

#[test]
fn policy_failure_window_sum_saturates() {
    let p = EntryPolicy {
        cache_control: CacheControl {
            stale_if_error: Some(u64::MAX),
            ..max_age(600)
        },
        ..sie_policy()
    };
    assert!(serve_network(&p, u64::MAX));
}
