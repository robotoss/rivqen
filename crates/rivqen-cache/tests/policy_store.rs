// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The store decision and the legacy `cache-offline` outcome (WP-07 S1 T-03;
//! freshness section 2 rule 1; R-CACHE-02, R-CACHE-03; legacy wire section
//! 7; legacy client CL-13, CL-15, CL-18, CL-19, CL-25, CL-30, CL-39, CL-44).

use rivqen_cache::header::{CacheControl, Vary, parse_cache_control};
use rivqen_cache::key::Protocol;
use rivqen_cache::partition::Credentials;
use rivqen_cache::policy::{SkipReason, StoreDecision, StoreInput, store_decision};
use rivqen_model::http::FieldLine;

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
    protocol: Protocol,
}

/// A legacy `200` without credentials and without directives: stored. The
/// tests of the RQP consent set `protocol` to `Protocol::Rqp`.
fn base() -> Exchange {
    Exchange {
        status: 200,
        request: CacheControl::default(),
        response: CacheControl::default(),
        vary: Vary::None,
        credentials: Credentials::Absent,
        set_cookie: false,
        authenticated_cache: false,
        protocol: Protocol::Legacy,
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
        protocol: exchange.protocol,
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

// ---- Consent of the server for an RQP response (H-150, DL-207) -------------

fn rqp_credentialed(response: CacheControl) -> Exchange {
    Exchange {
        protocol: Protocol::Rqp,
        credentials: Credentials::Present,
        authenticated_cache: true,
        response,
        ..base()
    }
}

/// `private` and `public` as the parser gives them for a valid line: a
/// `private` member sets both `private` flags.
fn directive(private: bool, public: bool) -> CacheControl {
    CacheControl {
        private,
        valid_private: private,
        public,
        ..CacheControl::default()
    }
}

#[test]
fn policy_store_rqp_consent_needs_private_or_public() {
    for response in [
        CacheControl::default(),
        CacheControl {
            max_age: Some(600),
            ..CacheControl::default()
        },
        no_cache_only(),
    ] {
        let e = rqp_credentialed(response);
        assert_eq!(
            decide(&e),
            skip(SkipReason::Credentialed),
            "{:?}",
            e.response
        );
    }
    for (private, public) in [(true, false), (false, true), (true, true)] {
        let e = rqp_credentialed(directive(private, public));
        assert_eq!(decide(&e), StoreDecision::Store, "{private} {public}");
    }
}

fn no_cache_only() -> CacheControl {
    CacheControl {
        no_cache: true,
        ..CacheControl::default()
    }
}

#[test]
fn policy_store_rqp_consent_needs_the_app_setting_too() {
    let e = Exchange {
        authenticated_cache: false,
        ..rqp_credentialed(directive(true, false))
    };
    assert_eq!(decide(&e), skip(SkipReason::Credentialed));
}

#[test]
fn policy_store_rqp_consent_covers_unknown_credentials_and_set_cookie() {
    let unknown = Exchange {
        credentials: Credentials::Unknown,
        ..rqp_credentialed(directive(true, false))
    };
    assert_eq!(decide(&unknown), StoreDecision::Store);
    let unknown_bare = Exchange {
        credentials: Credentials::Unknown,
        ..rqp_credentialed(CacheControl::default())
    };
    assert_eq!(decide(&unknown_bare), skip(SkipReason::Credentialed));
    let cookie = Exchange {
        credentials: Credentials::Absent,
        set_cookie: true,
        ..rqp_credentialed(CacheControl::default())
    };
    assert_eq!(decide(&cookie), skip(SkipReason::Credentialed));
    let cookie_private = Exchange {
        response: directive(true, false),
        ..cookie
    };
    assert_eq!(decide(&cookie_private), StoreDecision::Store);
}

#[test]
fn policy_store_rqp_consent_needs_a_valid_directive() {
    // A syntax error clears `public` (and every other directive).
    let response = parse_cache_control(&[FieldLine::new("Cache-Control", "public, max-age =5")]);
    assert!(!response.public, "the parser clears public");
    let e = rqp_credentialed(response);
    assert_eq!(decide(&e), skip(SkipReason::Credentialed));
}

#[test]
fn policy_store_rqp_consent_needs_a_valid_private() {
    // Security review L2, H-150: `private` that only the DL-088 split found
    // (a broken line) restricts, but is no consent.
    let restrictive_only = CacheControl {
        private: true,
        ..CacheControl::default()
    };
    assert_eq!(
        decide(&rqp_credentialed(restrictive_only)),
        skip(SkipReason::Credentialed)
    );
    for value in [
        "private=",
        "no-transform private",
        "max-age=5, no-cache=\"a, private",
        "private, a b",
    ] {
        let response = parse_cache_control(&[FieldLine::new("Cache-Control", value)]);
        assert!(response.private, "{value:?}");
        let e = rqp_credentialed(response);
        assert_eq!(decide(&e), skip(SkipReason::Credentialed), "{value:?}");
    }
    let two_lines = parse_cache_control(&[
        FieldLine::new("Cache-Control", "private"),
        FieldLine::new("Cache-Control", "a b"),
    ]);
    assert_eq!(
        decide(&rqp_credentialed(two_lines)),
        skip(SkipReason::Credentialed)
    );
    // Twins: a valid `private`, bare or qualified, is consent.
    for value in ["private", "private=\"set-cookie\"", "no-transform, private"] {
        let response = parse_cache_control(&[FieldLine::new("Cache-Control", value)]);
        let e = rqp_credentialed(response);
        assert_eq!(decide(&e), StoreDecision::Store, "{value:?}");
    }
    // Legacy mode does not read the directive (CL-25).
    let legacy = Exchange {
        protocol: Protocol::Legacy,
        ..rqp_credentialed(restrictive_only)
    };
    assert_eq!(decide(&legacy), StoreDecision::Store);
}

#[test]
fn policy_store_rqp_without_credentials_needs_no_directive() {
    let e = Exchange {
        credentials: Credentials::Absent,
        ..rqp_credentialed(CacheControl::default())
    };
    assert_eq!(decide(&e), StoreDecision::Store);
}

#[test]
fn policy_store_rqp_consent_keeps_the_rule_order() {
    let consent = directive(true, false);
    let no_store_e = Exchange {
        response: CacheControl {
            no_store: true,
            ..consent
        },
        ..rqp_credentialed(consent)
    };
    assert_eq!(decide(&no_store_e), skip(SkipReason::NoStore));
    let vary = Exchange {
        vary: Vary::Any,
        ..rqp_credentialed(consent)
    };
    assert_eq!(decide(&vary), skip(SkipReason::VaryAny));
    let status = Exchange {
        status: 404,
        ..rqp_credentialed(consent)
    };
    assert_eq!(decide(&status), skip(SkipReason::Status));
    let credentialed_first = Exchange {
        status: 404,
        ..rqp_credentialed(CacheControl::default())
    };
    assert_eq!(decide(&credentialed_first), skip(SkipReason::Credentialed));
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
