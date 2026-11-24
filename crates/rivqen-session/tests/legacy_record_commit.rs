// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The writes of a legacy session (CL-19, CL-26, CL-38, H-160, DL-220,
//! legacy client section 9.12; WP-08 S2 T-02): the content and the policy
//! of a commit, the disable, and the round trip through the fake driver of
//! `rivqen-testkit`.
#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use rivqen_cache::driver::StorageDriver;
use rivqen_cache::header::{CacheControl, parse_cache_control, parse_http_date};
use rivqen_cache::key::{Partition, Protocol, VaryEntry};
use rivqen_cache::policy::{EntryPolicy, Expires};
use rivqen_cache::txn::{
    CacheTransaction, EntryVersion, Epoch, Expect, Preconditions, Refused, TxnOp, TxnOutcome,
    Validators, Writer,
};
use rivqen_integrity::legacy::sha1_hex;
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::{PageBytes, Secret};
use rivqen_model::time::UnixTime;
use rivqen_session::legacy::record::{
    NewRecord, NotVisible, RequestValues, WriteBasis, check_visible, commit, disable, entry_key,
    remove, vary_entries, visible,
};
use rivqen_session::legacy::types::VaryKey;
use rivqen_template::legacy::DataBlock;
use rivqen_testkit::store::FakeStore;

const PAGE: &str = "<html><p>9.99</p><p>Tea</p></html>";
const TEMPLATE: &str = "<html><p>{price}</p><p>{name}</p></html>";
const HOST: &str = "shop.example";
const NOW: UnixTime = UnixTime(1_800_000_000);

fn line(name: &str, value: &str) -> FieldLine {
    FieldLine::new(name, value)
}

fn vary_key() -> VaryKey {
    VaryKey(Secret::new([9; 32]))
}

fn me() -> PartitionId {
    PartitionId::from_bytes([1; 32])
}

fn key() -> CacheKey {
    entry_key(
        me(),
        &rivqen_url::parse("https://shop.example/item?id=7").unwrap(),
    )
}

fn data() -> Vec<DataBlock> {
    vec![
        DataBlock {
            key: "{price}".to_owned(),
            value: "9.99".to_owned(),
        },
        DataBlock {
            key: "{name}".to_owned(),
            value: "Tea".to_owned(),
        },
    ]
}

fn page_fields() -> Vec<FieldLine> {
    vec![
        line("content-type", "text/html; charset=utf-8"),
        line("content-security-policy", "default-src 'self'"),
    ]
}

/// A commit of the sample page with `vary` and `response_fields`.
fn commit_of(
    basis: &WriteBasis,
    vary: &[VaryEntry],
    response_fields: &[FieldLine],
) -> CacheTransaction {
    let data = data();
    let fields = page_fields();
    commit(
        basis,
        &NewRecord {
            host: HOST,
            vary,
            page: PAGE,
            template: TEMPLATE,
            data: &data,
            fields: &fields,
            etag: sha1_hex(PAGE.as_bytes()),
            template_tag: sha1_hex(TEMPLATE.as_bytes()),
            response_fields,
            request_time: NOW,
            response_time: NOW.plus(2),
        },
    )
}

fn basis(expect: Expect) -> WriteBasis {
    WriteBasis {
        partition: me(),
        key: key(),
        partition_epoch: Epoch(4),
        site_epoch: Epoch(6),
        expect,
    }
}

/// A recovered empty store and the basis of a session that read it.
fn store() -> FakeStore {
    let mut store = FakeStore::new();
    store.recover(NOW).unwrap();
    store
}

fn basis_of(store: &mut FakeStore, expect: Expect) -> WriteBasis {
    WriteBasis {
        partition: me(),
        key: key(),
        partition_epoch: store.partition_epoch(Partition::Scoped(me())).unwrap(),
        site_epoch: store.site_epoch().unwrap(),
        expect,
    }
}

fn applied_version(outcome: TxnOutcome) -> EntryVersion {
    match outcome {
        TxnOutcome::Applied {
            version: Some(version),
        } => version,
        other => panic!("not applied with a version: {other:?}"),
    }
}

fn policy_of(response_fields: &[FieldLine]) -> EntryPolicy {
    let txn = commit_of(&basis(Expect::Any), &[], response_fields);
    let TxnOp::Replace { meta, .. } = txn.op else {
        panic!("a commit is a replace");
    };
    meta.policy
}

// ---- commit ----

#[test]
fn legacy_record_commit_is_a_replace_of_the_record() {
    let session = [line("cookie", "sid=1")];
    let values = RequestValues {
        session_fields: &session,
        platform_fields: &[],
    };
    let vary = vary_entries(&[line("vary", "cookie")], &values, &vary_key()).unwrap();
    let basis = basis(Expect::Version(EntryVersion(3)));
    let txn = commit_of(&basis, &vary, &[]);
    assert_eq!(txn.pre, basis.preconditions());
    let TxnOp::Replace { meta, body } = txn.op else {
        panic!("a commit is a replace");
    };
    assert_eq!(meta.key, key());
    assert_eq!(meta.partition, Partition::Scoped(me()));
    assert_eq!(meta.host, HOST);
    assert_eq!(meta.protocol, Protocol::Legacy);
    assert_eq!(meta.vary, vary);
    assert_eq!(
        meta.validators,
        Validators {
            etag: Some(sha1_hex(PAGE.as_bytes()).as_str().to_owned()),
            template_tag: Some(sha1_hex(TEMPLATE.as_bytes()).as_str().to_owned()),
            rqp: None,
        }
    );
    assert_eq!(meta.stored_at, NOW.plus(2));
    assert_eq!(meta.last_used_at, NOW.plus(2));
    assert_eq!(body.page, PageBytes::new(PAGE.as_bytes().to_vec()));
    assert_eq!(
        body.template,
        Some(PageBytes::new(TEMPLATE.as_bytes().to_vec()))
    );
    assert_eq!(
        body.data,
        [
            ("{price}".to_owned(), PageBytes::new(b"9.99".to_vec())),
            ("{name}".to_owned(), PageBytes::new(b"Tea".to_vec())),
        ]
    );
    assert_eq!(body.fields, page_fields());
}

#[test]
fn legacy_record_commit_policy_reads_the_response_fields() {
    let response = [
        line("Cache-Control", "private, max-age=60"),
        line("cache-control", "stale-while-revalidate=30"),
        line("Date", "Sun, 06 Nov 1994 08:49:37 GMT"),
        line("expires", "Sun, 06 Nov 1994 08:50:37 GMT"),
        line("Age", "x, 5"),
        line("set-cookie", "ignored=1"),
    ];
    let policy = policy_of(&response);
    assert_eq!(policy.cache_control, parse_cache_control(&response));
    assert_eq!(policy.cache_control.max_age, Some(60));
    assert_eq!(policy.cache_control.stale_while_revalidate, Some(30));
    assert!(policy.cache_control.private);
    let date = parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
    assert_eq!(policy.date, Some(date));
    assert_eq!(policy.expires, Expires::At(date.plus(60)));
    assert_eq!(policy.age, Some(5));
    assert_eq!(policy.request_time, NOW);
    assert_eq!(policy.response_time, NOW.plus(2));
    assert_eq!(policy.ttl_seconds, None);
    assert!(!policy.offline);
}

#[test]
fn legacy_record_commit_policy_without_fields_has_no_lifetime() {
    let policy = policy_of(&[]);
    assert_eq!(policy.cache_control, CacheControl::default());
    assert_eq!(policy.expires, Expires::Absent);
    assert_eq!(policy.date, None);
    assert_eq!(policy.age, None);
}

#[test]
fn legacy_record_commit_policy_expires_lines_must_agree() {
    let a = "Sun, 06 Nov 1994 08:49:37 GMT";
    let b = "Sun, 06 Nov 1994 08:50:37 GMT";
    let at = parse_http_date(a).unwrap();
    assert_eq!(
        policy_of(&[line("expires", a), line("Expires", a)]).expires,
        Expires::At(at)
    );
    assert_eq!(
        policy_of(&[line("expires", a), line("expires", b)]).expires,
        Expires::Invalid
    );
    assert_eq!(policy_of(&[line("expires", "0")]).expires, Expires::Invalid);
    assert_eq!(
        policy_of(&[line("expires", a), line("expires", "0")]).expires,
        Expires::Invalid
    );
    assert_eq!(
        policy_of(&[line("expires", "0"), line("expires", a)]).expires,
        Expires::Invalid
    );
}

#[test]
fn legacy_record_commit_policy_date_lines_must_agree() {
    let a = "Sun, 06 Nov 1994 08:49:37 GMT";
    let b = "Sun, 06 Nov 1994 08:50:37 GMT";
    let at = parse_http_date(a).unwrap();
    assert_eq!(
        policy_of(&[line("date", a), line("DATE", a)]).date,
        Some(at)
    );
    assert_eq!(policy_of(&[line("date", a), line("date", b)]).date, None);
    assert_eq!(policy_of(&[line("date", "yesterday")]).date, None);
    assert_eq!(policy_of(&[line("date", "x"), line("date", a)]).date, None);
}

#[test]
fn legacy_record_commit_round_trip_through_the_store() {
    let mut store = store();
    let session = [line("cookie", "sid=1")];
    let platform = [line("accept-language", "de")];
    let values = RequestValues {
        session_fields: &session,
        platform_fields: &platform,
    };
    let vary = vary_entries(
        &[line("vary", "Cookie, Accept-Language, Accept-Encoding")],
        &values,
        &vary_key(),
    )
    .unwrap();
    let basis = basis_of(&mut store, Expect::Absent);
    let version = applied_version(store.apply(commit_of(&basis, &vary, &[])).unwrap());

    let entry = store
        .read(Partition::Scoped(me()), &key(), NOW)
        .unwrap()
        .unwrap();
    let record = visible(&entry, &values, &vary_key()).unwrap();
    assert_eq!(record.version, version);
    assert_eq!(record.page, PAGE);
    assert_eq!(record.template, TEMPLATE);
    assert_eq!(record.data, data());
    assert_eq!(record.fields, page_fields());
    assert_eq!(record.vary, vary);
    assert_eq!(record.etag, sha1_hex(PAGE.as_bytes()));
    assert_eq!(record.template_tag, sha1_hex(TEMPLATE.as_bytes()));

    // Another cookie does not see it.
    let other = [line("cookie", "sid=2")];
    let other_values = RequestValues {
        session_fields: &other,
        platform_fields: &platform,
    };
    assert_eq!(
        check_visible(&entry, &other_values, &vary_key()).map(|_| ()),
        Err(NotVisible::OtherVariant)
    );
    // No entry in another partition.
    let theirs = PartitionId::from_bytes([2; 32]);
    assert_eq!(
        store.read(Partition::Scoped(theirs), &key(), NOW).unwrap(),
        None
    );
}

/// The request values of session fields only.
fn of(fields: &[FieldLine]) -> RequestValues<'_> {
    RequestValues {
        session_fields: fields,
        platform_fields: &[],
    }
}

/// H-160: one entry per URL; a commit for other values replaces it.
#[test]
fn legacy_record_commit_for_other_values_replaces_the_entry() {
    let mut store = store();
    let first = &[line("cookie", "sid=1")][..];
    let second = &[line("cookie", "sid=2")][..];
    let response = [line("vary", "cookie")];

    let vary_one = vary_entries(&response, &of(first), &vary_key()).unwrap();
    let basis = basis_of(&mut store, Expect::Absent);
    let v1 = applied_version(
        store
            .apply(commit_of(&basis, &vary_one, &response))
            .unwrap(),
    );

    let entry = store
        .read(Partition::Scoped(me()), &key(), NOW)
        .unwrap()
        .unwrap();
    assert_eq!(
        check_visible(&entry, &of(second), &vary_key()).map(|_| ()),
        Err(NotVisible::OtherVariant)
    );
    // The second session saw the entry of the first one (the same key).
    let vary_two = vary_entries(&response, &of(second), &vary_key()).unwrap();
    let basis = basis_of(&mut store, Expect::Version(v1));
    let v2 = applied_version(
        store
            .apply(commit_of(&basis, &vary_two, &response))
            .unwrap(),
    );
    assert_ne!(v1, v2);

    let entry = store
        .read(Partition::Scoped(me()), &key(), NOW)
        .unwrap()
        .unwrap();
    assert_eq!(entry.version, v2);
    assert!(visible(&entry, &of(second), &vary_key()).is_some());
    assert!(visible(&entry, &of(first), &vary_key()).is_none());
    assert_eq!(store.list().unwrap().len(), 1);
}

// ---- writes ----

#[test]
fn legacy_record_writes_name_the_request_partition() {
    let basis = basis(Expect::Version(EntryVersion(8)));
    let expected = Preconditions {
        entry: Expect::Version(EntryVersion(8)),
        writer: Writer::request(me(), Epoch(4)),
        partition_epoch: Epoch(4),
        site_epoch: Epoch(6),
    };
    for txn in [
        commit_of(&basis, &[], &[]),
        remove(&basis),
        disable(&basis, NOW),
    ] {
        assert_eq!(txn.pre, expected, "{:?}", txn.op);
        let partition = match &txn.op {
            TxnOp::Replace { meta, .. } => meta.partition,
            TxnOp::Remove { partition, .. } | TxnOp::DisableKey { partition, .. } => *partition,
            other => panic!("unexpected op {other:?}"),
        };
        assert_eq!(partition, Partition::Scoped(me()));
    }
}

#[test]
fn legacy_record_writes_disable_the_key_until_the_time() {
    let basis = basis(Expect::Any);
    let until = NOW.plus(21_600);
    let txn = disable(&basis, until);
    assert_eq!(
        txn.op,
        TxnOp::DisableKey {
            partition: Partition::Scoped(me()),
            key: key(),
            until,
        }
    );

    let mut store = store();
    let basis = basis_of(&mut store, Expect::Absent);
    applied_version(store.apply(commit_of(&basis, &[], &[])).unwrap());
    let basis = basis_of(&mut store, Expect::Any);
    let outcome = store.apply(disable(&basis, until)).unwrap();
    assert!(matches!(outcome, TxnOutcome::Applied { .. }), "{outcome:?}");
    let partition = Partition::Scoped(me());
    assert!(store.key_disabled(partition, &key(), NOW).unwrap());
    assert!(
        store
            .key_disabled(partition, &key(), NOW.plus(21_599))
            .unwrap()
    );
    assert!(!store.key_disabled(partition, &key(), until).unwrap());
    // The disable removed the entry.
    assert_eq!(store.read(partition, &key(), until).unwrap(), None);
}

#[test]
fn legacy_record_writes_are_refused_after_a_revoke_of_the_partition() {
    let mut store = store();
    let basis = basis_of(&mut store, Expect::Any);
    let revoke = CacheTransaction {
        op: TxnOp::RevokePartition {
            partition: Partition::Scoped(me()),
        },
        pre: Preconditions {
            entry: Expect::Any,
            writer: Writer::Maintenance,
            partition_epoch: Epoch(0),
            site_epoch: Epoch(0),
        },
    };
    store.apply(revoke).unwrap();
    for txn in [
        commit_of(&basis, &[], &[]),
        remove(&basis),
        disable(&basis, NOW.plus(10)),
    ] {
        assert_eq!(
            store.apply(txn).unwrap(),
            TxnOutcome::Refused(Refused::PartitionRevoked)
        );
    }
    assert_eq!(
        store.read(Partition::Scoped(me()), &key(), NOW).unwrap(),
        None
    );
    assert!(
        !store
            .key_disabled(Partition::Scoped(me()), &key(), NOW)
            .unwrap()
    );
}

#[test]
fn legacy_record_writes_are_refused_after_a_site_clear() {
    let mut store = store();
    let basis = basis_of(&mut store, Expect::Any);
    let clear = CacheTransaction {
        op: TxnOp::ClearSite {
            domain: Some(HOST.to_owned()),
        },
        pre: Preconditions {
            entry: Expect::Any,
            writer: Writer::Maintenance,
            partition_epoch: Epoch(0),
            site_epoch: Epoch(0),
        },
    };
    store.apply(clear).unwrap();
    assert_eq!(
        store.apply(commit_of(&basis, &[], &[])).unwrap(),
        TxnOutcome::Refused(Refused::SiteCleared)
    );
}

#[test]
fn legacy_record_writes_follow_the_expected_entry() {
    let mut store = store();
    let basis = basis_of(&mut store, Expect::Absent);
    let v1 = applied_version(store.apply(commit_of(&basis, &[], &[])).unwrap());
    // A session that saw no entry does not replace the new one.
    assert_eq!(
        store.apply(commit_of(&basis, &[], &[])).unwrap(),
        TxnOutcome::Refused(Refused::Entry)
    );
    let stale = basis_of(&mut store, Expect::Version(EntryVersion(v1.0 + 100)));
    assert_eq!(
        store.apply(remove(&stale)).unwrap(),
        TxnOutcome::Refused(Refused::Entry)
    );
    let current = basis_of(&mut store, Expect::Version(v1));
    assert!(matches!(
        store.apply(remove(&current)).unwrap(),
        TxnOutcome::Applied { .. }
    ));
    assert_eq!(
        store.read(Partition::Scoped(me()), &key(), NOW).unwrap(),
        None
    );
}
