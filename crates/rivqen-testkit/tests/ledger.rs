// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Ledger tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rivqen_testkit::fixtures::load_rqp_protocol;
use rivqen_testkit::ledger::{Check, Ledger, LedgerError, Owner};

#[test]
fn ledger_covers_every_client_fixture() {
    let ledger = Ledger::embedded().unwrap();
    let all = load_rqp_protocol().unwrap();
    ledger
        .verify(all.client.iter().map(|f| f.id.as_str()))
        .unwrap();
    assert_eq!(ledger.entries().count(), 153);
    assert_eq!(ledger.owned_by(Owner::Wp05).count(), 25);
    assert_eq!(ledger.owned_by(Owner::Wp08).count(), 24);
    assert_eq!(ledger.owned_by(Owner::Wp06).count(), 104);
}

#[test]
fn ledger_wp05_set_is_the_outcome_fixtures() {
    let ledger = Ledger::embedded().unwrap();
    let all = load_rqp_protocol().unwrap();
    let wp05: Vec<_> = all
        .client
        .iter()
        .filter(|f| ledger.get(&f.id).unwrap().owner == Owner::Wp05)
        .collect();
    let by = |code: &str| {
        wp05.iter()
            .filter(|f| f.client_result().unwrap().error.as_deref() == Some(code))
            .count()
    };
    assert_eq!(by("INTEGRITY_ERROR"), 13);
    assert_eq!(by("PATCH_INVALID"), 8);
    assert_eq!(by("PATCH_TOO_LARGE"), 1);
    assert_eq!(by("MANIFEST_INVALID"), 3);
}

#[test]
fn ledger_checks_follow_the_fixtures() {
    let ledger = Ledger::embedded().unwrap();
    let all = load_rqp_protocol().unwrap();
    let mut request_fields = 0;
    let mut binding = 0;
    for f in &all.client {
        let e = ledger.get(&f.id).unwrap();
        let has_rq = f
            .expected_request()
            .unwrap()
            .headers
            .iter()
            .any(|h| h.name.to_ascii_lowercase().starts_with("rq-"));
        assert_eq!(e.checks.contains(&Check::RequestFields), has_rq, "{}", f.id);
        let patched = f.client_result().unwrap().outcome == "patched";
        assert_eq!(e.checks.contains(&Check::Binding), patched, "{}", f.id);
        request_fields += usize::from(has_rq);
        binding += usize::from(patched);
    }
    assert_eq!(request_fields, 150);
    assert_eq!(binding, 26);
}

#[test]
fn ledger_rejects_missing_and_extra_entries() {
    let ledger = Ledger::parse("A WP-05 integrity -\nB WP-06 markup -\n").unwrap();
    assert_eq!(
        ledger.verify(["A", "B", "C"]),
        Err(LedgerError::MissingEntry("C".into()))
    );
    assert_eq!(
        ledger.verify(["A"]),
        Err(LedgerError::UnknownEntry("B".into()))
    );
    assert_eq!(ledger.verify(["B", "A"]), Ok(()));
}

#[test]
fn ledger_rejects_malformed_lines() {
    assert!(matches!(
        Ledger::parse("A WP-05 integrity -\nA WP-06 markup -\n"),
        Err(LedgerError::Duplicate(_))
    ));
    assert!(matches!(
        Ledger::parse("# c\n\nA WP-09 x -\n"),
        Err(LedgerError::Line { line: 3, .. })
    ));
    assert!(matches!(
        Ledger::parse("A WP-05 x\n"),
        Err(LedgerError::Line { .. })
    ));
    assert!(matches!(
        Ledger::parse("A WP-05 x - extra\n"),
        Err(LedgerError::Line { .. })
    ));
    assert!(matches!(
        Ledger::parse("A WP-05 x nope\n"),
        Err(LedgerError::Line { .. })
    ));
    let ok = Ledger::parse("A WP-08 normal request-fields,binding\n").unwrap();
    assert_eq!(
        ok.get("A").unwrap().checks,
        [Check::RequestFields, Check::Binding]
    );
}
