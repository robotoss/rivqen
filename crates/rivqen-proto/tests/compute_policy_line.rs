// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The policy line of manifest-patch section 1.3 (H-55): no bytes without
//! `policy` and `limits`, and the exact text S for every presence combination.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeSet;

use rivqen_integrity::{PageRevision, TemplateRevision};
use rivqen_proto::compute::{page_revision, policy_line};
use rivqen_proto::types::{Limits, Mode, Policy};

const TEMPLATE: &str = "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM";

#[allow(
    clippy::unnecessary_wraps,
    reason = "compared with the result of policy_line"
)]
fn line(s: &str) -> Option<String> {
    Some(format!("PL\t{s}\n"))
}

#[test]
fn compute_policy_line_absent_without_policy_and_limits() {
    assert_eq!(policy_line(None, None), None);
    // Rule 2: P is then the preimage without a policy line (the first KAT set).
    let template = TemplateRevision::parse(TEMPLATE).unwrap();
    let expected = PageRevision::from_digest(rivqen_integrity::sha256(
        format!("rqp-r1\n{TEMPLATE}\n").as_bytes(),
    ));
    assert_eq!(page_revision(&template, &[], None, None), expected);
}

#[test]
fn compute_policy_line_empty_members_write_their_name() {
    let empty_policy = Policy::default();
    let empty_limits = Limits::default();
    assert_eq!(policy_line(Some(&empty_policy), None), line("policy"));
    assert_eq!(policy_line(None, Some(&empty_limits)), line("limits"));
    assert_eq!(
        policy_line(Some(&empty_policy), Some(&empty_limits)),
        line("policy;limits")
    );
}

#[test]
fn compute_policy_line_kat() {
    let policy = Policy {
        mode: None,
        ttl_seconds: Some(60),
        offline: Some(true),
    };
    let limits = Limits {
        max_patch_bytes: Some(65_536),
    };
    assert_eq!(
        policy_line(Some(&policy), Some(&limits)).unwrap(),
        "PL\tpolicy;ttl_seconds=60;offline=true;limits;max_patch_bytes=65536\n"
    );
}

#[test]
fn compute_policy_line_writes_members_not_defaults() {
    // Rule 1: a default value is written when the member is present.
    let defaults = Policy {
        mode: Some(Mode::Private),
        ttl_seconds: Some(0),
        offline: Some(false),
    };
    assert_eq!(
        policy_line(Some(&defaults), None),
        line("policy;mode=private;ttl_seconds=0;offline=false")
    );
    let public = Policy {
        mode: Some(Mode::Public),
        ..Policy::default()
    };
    assert_eq!(policy_line(Some(&public), None), line("policy;mode=public"));
}

#[test]
fn compute_policy_line_decimal_values() {
    for (value, digits) in [
        (0_u32, "0"),
        (7, "7"),
        (10, "10"),
        (31_536_000, "31536000"),
        (u32::MAX, "4294967295"),
    ] {
        let policy = Policy {
            ttl_seconds: Some(value),
            ..Policy::default()
        };
        assert_eq!(
            policy_line(Some(&policy), None),
            line(&format!("policy;ttl_seconds={digits}"))
        );
        let limits = Limits {
            max_patch_bytes: Some(value),
        };
        assert_eq!(
            policy_line(None, Some(&limits)),
            line(&format!("limits;max_patch_bytes={digits}"))
        );
    }
}

#[test]
fn compute_policy_line_longest_line() {
    let policy = Policy {
        mode: Some(Mode::Private),
        ttl_seconds: Some(u32::MAX),
        offline: Some(false),
    };
    let limits = Limits {
        max_patch_bytes: Some(u32::MAX),
    };
    let text = policy_line(Some(&policy), Some(&limits)).unwrap();
    assert_eq!(
        text,
        "PL\tpolicy;mode=private;ttl_seconds=4294967295;offline=false;limits;max_patch_bytes=4294967295\n"
    );
    assert_eq!(text.len(), 94);
}

/// Every presence combination of section 1.3, with values from the closed
/// sets and the range bounds of sections 1.4 and 1.5.
fn combinations() -> Vec<(Option<Policy>, Option<Limits>)> {
    let modes = [None, Some(Mode::Private), Some(Mode::Public)];
    let ttls = [None, Some(0), Some(60), Some(31_536_000)];
    let offlines = [None, Some(false), Some(true)];
    let mut policies = vec![None];
    for mode in modes {
        for ttl_seconds in ttls {
            for offline in offlines {
                policies.push(Some(Policy {
                    mode,
                    ttl_seconds,
                    offline,
                }));
            }
        }
    }
    let limits = [
        None,
        Some(Limits {
            max_patch_bytes: None,
        }),
        Some(Limits {
            max_patch_bytes: Some(1_024),
        }),
        Some(Limits {
            max_patch_bytes: Some(5_242_880),
        }),
    ];
    let mut all = Vec::new();
    for policy in &policies {
        for limits in limits {
            all.push((*policy, limits));
        }
    }
    all
}

/// The table of section 1.3 as a list of items.
fn table_items(policy: Option<&Policy>, limits: Option<&Limits>) -> Vec<String> {
    let mut items = Vec::new();
    if let Some(p) = policy {
        items.push("policy".to_owned());
        match p.mode {
            Some(Mode::Private) => items.push("mode=private".to_owned()),
            Some(Mode::Public) => items.push("mode=public".to_owned()),
            None => {}
        }
        if let Some(ttl) = p.ttl_seconds {
            items.push(format!("ttl_seconds={ttl}"));
        }
        if let Some(offline) = p.offline {
            items.push(format!("offline={offline}"));
        }
    }
    if let Some(l) = limits {
        items.push("limits".to_owned());
        if let Some(max) = l.max_patch_bytes {
            items.push(format!("max_patch_bytes={max}"));
        }
    }
    items
}

#[test]
fn compute_policy_line_every_presence_combination() {
    let all = combinations();
    assert_eq!(all.len(), 148);
    let template = TemplateRevision::parse(TEMPLATE).unwrap();
    let mut lines = BTreeSet::new();
    let mut revisions = BTreeSet::new();
    for (policy, limits) in &all {
        let got = policy_line(policy.as_ref(), limits.as_ref());
        if policy.is_none() && limits.is_none() {
            assert_eq!(got, None);
        } else {
            let items = table_items(policy.as_ref(), limits.as_ref());
            assert_eq!(got, line(&items.join(";")), "{policy:?} {limits:?}");
            let text = got.clone().unwrap();
            // Rule 4: one parse; no tab or line feed inside S.
            let s = &text[3..text.len() - 1];
            assert!(!s.contains(['\t', '\n']), "{s}");
            assert_eq!(s.split(';').count(), items.len());
        }
        lines.insert(got);
        revisions
            .insert(page_revision(&template, &[], policy.as_ref(), limits.as_ref()).to_string());
    }
    // Each combination gives its own line and its own page revision.
    assert_eq!(lines.len(), all.len());
    assert_eq!(revisions.len(), all.len());
}
