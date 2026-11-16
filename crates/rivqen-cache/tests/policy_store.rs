// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The store decision and the legacy `cache-offline` outcome (WP-07 S1 T-03;
//! freshness section 2 rule 1; R-CACHE-02, R-CACHE-03; legacy wire section
//! 7; legacy client CL-13, CL-15, CL-18, CL-19, CL-25, CL-30, CL-39, CL-44).

use rivqen_cache::header::{CacheControl, Vary};
use rivqen_cache::partition::Credentials;
use rivqen_cache::policy::{SkipReason, StoreDecision, StoreInput, store_decision};

/// The inputs of a store decision that the test can change.
#[derive(Clone)]
#[allow(clippy::struct_excessive_bools)] // one flag per input
struct Exchange {
    status: u16,
    request: CacheControl,
    response: CacheControl,
    vary: Vary,
    credentials: Credentials,
    set_cookie: bool,
    authenticated_cache: bool,
}

/// A `200` without credentials and without directives: stored.
fn base() -> Exchange {
    Exchange {
        status: 200,
        request: CacheControl::default(),
        response: CacheControl::default(),
        vary: Vary::None,
        credentials: Credentials::Absent,
        set_cookie: false,
        authenticated_cache: false,
    }
}

fn decide(exchange: &Exchange) -> StoreDecision {
    store_decision(&StoreInput {
        status: exchange.status,
        request: &exchange.request,
        response: &exchange.response,
        vary: &exchange.vary,
        credentials: exchange.credentials,
        set_cookie: exchange.set_cookie,
        authenticated_cache: exchange.authenticated_cache,
    })
}

fn no_store() -> CacheControl {
    CacheControl {
        no_store: true,
        ..CacheControl::default()
    }
}

fn skip(reason: SkipReason) -> StoreDecision {
    StoreDecision::Skip(reason)
}

// ---- Store -----------------------------------------------------------------

#[test]
fn policy_store_plain_200_is_stored() {
    assert_eq!(decide(&base()), StoreDecision::Store);
}

#[test]
fn policy_store_no_store_in_the_response_skips() {
    let e = Exchange {
        response: no_store(),
        ..base()
    };
    assert_eq!(decide(&e), skip(SkipReason::NoStore));
}

#[test]
fn policy_store_no_store_in_the_request_skips() {
    let e = Exchange {
        request: no_store(),
        ..base()
    };
    assert_eq!(decide(&e), skip(SkipReason::NoStore));
}

#[test]
fn policy_store_vary_any_skips() {
    let e = Exchange {
        vary: Vary::Any,
        ..base()
    };
    assert_eq!(decide(&e), skip(SkipReason::VaryAny));
    let names = Exchange {
        vary: Vary::Names(vec!["accept-encoding".to_owned()]),
        ..base()
    };
    assert_eq!(decide(&names), StoreDecision::Store);
}

#[test]
fn policy_store_credentials_skip_without_the_app_setting() {
    for credentials in [Credentials::Present, Credentials::Unknown] {
        let e = Exchange {
            credentials,
            ..base()
        };
        assert_eq!(
            decide(&e),
            skip(SkipReason::Credentialed),
            "{credentials:?}"
        );
        let allowed = Exchange {
            authenticated_cache: true,
            ..e
        };
        assert_eq!(decide(&allowed), StoreDecision::Store, "{credentials:?}");
    }
}

#[test]
fn policy_store_set_cookie_counts_as_credentials() {
    let e = Exchange {
        set_cookie: true,
        ..base()
    };
    assert_eq!(decide(&e), skip(SkipReason::Credentialed));
    let allowed = Exchange {
        authenticated_cache: true,
        ..e
    };
    assert_eq!(decide(&allowed), StoreDecision::Store);
}

#[test]
fn policy_store_app_setting_alone_changes_nothing_without_credentials() {
    let e = Exchange {
        authenticated_cache: true,
        ..base()
    };
    assert_eq!(decide(&e), StoreDecision::Store);
}

#[test]
fn policy_store_private_and_no_cache_store() {
    for response in [
        CacheControl {
            private: true,
            ..CacheControl::default()
        },
        CacheControl {
            no_cache: true,
            ..CacheControl::default()
        },
        CacheControl {
            must_revalidate: true,
            invalid_freshness: true,
            ..CacheControl::default()
        },
    ] {
        let e = Exchange { response, ..base() };
        assert_eq!(decide(&e), StoreDecision::Store, "{response:?}");
    }
}

#[test]
fn policy_store_status_other_than_200_skips() {
    for status in [100, 199, 201, 203, 204, 206, 300, 301, 302, 304, 307, 308] {
        let e = Exchange { status, ..base() };
        assert_eq!(decide(&e), skip(SkipReason::Status), "status {status}");
    }
    for status in [400, 401, 403, 404, 407, 409, 410, 428, 430, 499, 600, 999] {
        let e = Exchange { status, ..base() };
        assert_eq!(decide(&e), skip(SkipReason::Status), "status {status}");
    }
}

#[test]
fn policy_store_server_errors_are_transient() {
    for status in [408, 429, 500, 502, 503, 504, 599] {
        let e = Exchange { status, ..base() };
        assert_eq!(
            decide(&e),
            skip(SkipReason::TransientStatus),
            "status {status}"
        );
    }
}

#[test]
fn policy_store_rule_order() {
    // no-store before Vary: *, credentials and status.
    let all = Exchange {
        status: 503,
        response: no_store(),
        vary: Vary::Any,
        credentials: Credentials::Present,
        ..base()
    };
    assert_eq!(decide(&all), skip(SkipReason::NoStore));
    // Vary: * before credentials and status.
    let vary = Exchange {
        response: CacheControl::default(),
        ..all.clone()
    };
    assert_eq!(decide(&vary), skip(SkipReason::VaryAny));
    // Credentials before status.
    let credentials = Exchange {
        vary: Vary::None,
        ..vary
    };
    assert_eq!(decide(&credentials), skip(SkipReason::Credentialed));
    let status = Exchange {
        credentials: Credentials::Absent,
        ..credentials
    };
    assert_eq!(decide(&status), skip(SkipReason::TransientStatus));
}

// ---- Legacy ----------------------------------------------------------------

#[cfg(feature = "legacy")]
mod legacy {
    use rivqen_cache::header::CacheOffline;
    use rivqen_cache::policy::legacy::{LegacyCacheOutcome, legacy_cache_outcome};
    use rivqen_cache::policy::{SkipReason, StoreDecision};

    const PERIOD: u64 = 21_600_000;

    const COMMIT: LegacyCacheOutcome = LegacyCacheOutcome {
        commit: true,
        remove_entry: false,
        disable_key_ms: None,
    };
    const REMOVE: LegacyCacheOutcome = LegacyCacheOutcome {
        commit: false,
        remove_entry: true,
        disable_key_ms: None,
    };
    const KEEP: LegacyCacheOutcome = LegacyCacheOutcome {
        commit: false,
        remove_entry: false,
        disable_key_ms: None,
    };

    fn disable(ms: u64) -> LegacyCacheOutcome {
        LegacyCacheOutcome {
            commit: false,
            remove_entry: true,
            disable_key_ms: Some(ms),
        }
    }

    const ALL_VALUES: [CacheOffline; 4] = [
        CacheOffline::True,
        CacheOffline::False,
        CacheOffline::Store,
        CacheOffline::Http,
    ];

    /// The skips that do not come from the status.
    const RESPONSE_SKIPS: [SkipReason; 3] = [
        SkipReason::NoStore,
        SkipReason::VaryAny,
        SkipReason::Credentialed,
    ];

    #[test]
    fn policy_legacy_true_and_store_commit() {
        for offline in [CacheOffline::True, CacheOffline::Store] {
            assert_eq!(
                legacy_cache_outcome(offline, StoreDecision::Store, PERIOD),
                COMMIT,
                "{offline:?}"
            );
        }
    }

    #[test]
    fn policy_legacy_false_removes() {
        assert_eq!(
            legacy_cache_outcome(CacheOffline::False, StoreDecision::Store, PERIOD),
            REMOVE
        );
    }

    #[test]
    fn policy_legacy_response_skip_wins_over_true_and_store() {
        for reason in RESPONSE_SKIPS {
            for offline in [CacheOffline::True, CacheOffline::Store, CacheOffline::False] {
                assert_eq!(
                    legacy_cache_outcome(offline, StoreDecision::Skip(reason), PERIOD),
                    REMOVE,
                    "{offline:?} {reason:?}"
                );
            }
        }
    }

    #[test]
    fn policy_legacy_http_removes_and_disables() {
        assert_eq!(
            legacy_cache_outcome(CacheOffline::Http, StoreDecision::Store, PERIOD),
            disable(PERIOD)
        );
        assert_eq!(
            legacy_cache_outcome(CacheOffline::Http, StoreDecision::Store, 60_000),
            disable(60_000)
        );
        assert_eq!(
            legacy_cache_outcome(CacheOffline::Http, StoreDecision::Store, u64::MAX),
            disable(u64::MAX)
        );
    }

    #[test]
    fn policy_legacy_http_wins_over_no_store_vary_and_credentials() {
        for reason in RESPONSE_SKIPS {
            assert_eq!(
                legacy_cache_outcome(CacheOffline::Http, StoreDecision::Skip(reason), PERIOD),
                disable(PERIOD),
                "{reason:?}"
            );
        }
    }

    #[test]
    fn policy_legacy_status_removes_before_cache_offline() {
        for offline in ALL_VALUES {
            assert_eq!(
                legacy_cache_outcome(offline, StoreDecision::Skip(SkipReason::Status), PERIOD),
                REMOVE,
                "{offline:?}"
            );
        }
    }

    #[test]
    fn policy_legacy_transient_status_keeps_the_entry() {
        for offline in ALL_VALUES {
            assert_eq!(
                legacy_cache_outcome(
                    offline,
                    StoreDecision::Skip(SkipReason::TransientStatus),
                    PERIOD
                ),
                KEEP,
                "{offline:?}"
            );
        }
    }
}
