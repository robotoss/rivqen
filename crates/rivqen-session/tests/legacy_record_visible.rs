// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The visible record of an entry (CL-12, CL-40, H-160; WP-08 S2 T-02):
//! a `Vary` mismatch is another variant that stays, every other failed
//! check is a damaged entry that the session removes.
//!
//! Fixed seeds; `PROPTEST_CASES` raises the case count.
#![cfg(feature = "legacy")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use rivqen_cache::key::{Protocol, VaryDigest, VaryEntry};
use rivqen_cache::txn::{EntryVersion, Epoch, Expect, StoredEntry, TxnOp};
use rivqen_integrity::legacy::sha1_hex;
use rivqen_model::http::FieldLine;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::{PageBytes, Secret};
use rivqen_model::time::UnixTime;
use rivqen_session::legacy::record::{
    NewRecord, NotVisible, RequestValues, WriteBasis, check_visible, check_visible_in, commit,
    vary_entries, visible,
};
use rivqen_session::legacy::types::VaryKey;
use rivqen_template::legacy::DataBlock;

const PAGE: &str = "<html><p>9.99</p></html>";
const TEMPLATE: &str = "<html><p>{price}</p></html>";

fn vary_key() -> VaryKey {
    VaryKey(Secret::new([0x42; 32]))
}

fn line(name: &str, value: &str) -> FieldLine {
    FieldLine::new(name, value)
}

fn basis() -> WriteBasis {
    WriteBasis {
        partition: PartitionId::from_bytes([1; 32]),
        key: CacheKey::from_bytes([2; 32]),
        partition_epoch: Epoch(0),
        site_epoch: Epoch(0),
        expect: Expect::Absent,
    }
}

fn values<'a>(session: &'a [FieldLine], platform: &'a [FieldLine]) -> RequestValues<'a> {
    RequestValues {
        session_fields: session,
        platform_fields: platform,
    }
}

/// The entry that a commit of `page`, `template`, `data` and `vary` stores,
/// as a driver gives it back.
fn stored(page: &str, template: &str, data: &[DataBlock], vary: &[VaryEntry]) -> StoredEntry {
    let fields = [line("content-type", "text/html; charset=utf-8")];
    let txn = commit(
        &basis(),
        &NewRecord {
            host: "shop.example",
            vary,
            page,
            template,
            data,
            fields: &fields,
            etag: sha1_hex(page.as_bytes()),
            template_tag: sha1_hex(template.as_bytes()),
            response_fields: &[],
            request_time: UnixTime(10),
            response_time: UnixTime(11),
        },
    );
    let TxnOp::Replace { meta, body } = txn.op else {
        panic!("a commit is a replace");
    };
    StoredEntry {
        meta: *meta,
        version: EntryVersion(7),
        body: *body,
    }
}

fn price() -> Vec<DataBlock> {
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

/// An entry with `Vary: cookie, accept-language` for the cookie `sid=1`
/// and no `accept-language`.
fn cookie_entry() -> (StoredEntry, Vec<FieldLine>) {
    let session = vec![line("cookie", "sid=1")];
    let vary = vary_entries(
        &[line("vary", "cookie, accept-language")],
        &values(&session, &[]),
        &vary_key(),
    )
    .unwrap();
    (stored(PAGE, TEMPLATE, &price(), &vary), session)
}

fn check(
    entry: &StoredEntry,
    session: &[FieldLine],
    platform: &[FieldLine],
) -> Result<(), NotVisible> {
    check_visible(entry, &values(session, platform), &vary_key()).map(|_| ())
}

#[test]
fn legacy_record_visible_gives_the_stored_record() {
    let (entry, session) = cookie_entry();
    let record = visible(&entry, &values(&session, &[]), &vary_key()).unwrap();
    assert_eq!(record.version, EntryVersion(7));
    assert_eq!(record.page, PAGE);
    assert_eq!(record.template, TEMPLATE);
    assert_eq!(record.data, price());
    assert_eq!(
        record.fields,
        [line("content-type", "text/html; charset=utf-8")]
    );
    assert_eq!(record.vary, entry.meta.vary);
    assert_eq!(record.etag, sha1_hex(PAGE.as_bytes()));
    assert_eq!(record.template_tag, sha1_hex(TEMPLATE.as_bytes()));
    assert_eq!(
        check_visible(&entry, &values(&session, &[]), &vary_key()),
        Ok(record)
    );
}

#[test]
fn legacy_record_visible_page_fields_pass_the_filter() {
    // DL-229 (d): a stored line that is not a valid field line or whose
    // name never goes to the web view is not in the record; a clean record
    // keeps its lines.
    let mut entry = stored(PAGE, TEMPLATE, &[], &[]);
    entry.body.fields = vec![
        line("Set-Cookie", "sid=1"),
        line("Connection", "close"),
        line("keep-alive", "timeout=5"),
        line("x-bad", "a\u{0}b"),
        line("X-Page", "1"),
        line("content-type", "text/html; charset=utf-8"),
    ];
    let record = check_visible(&entry, &values(&[], &[]), &vary_key()).unwrap();
    assert_eq!(
        record.fields,
        [
            line("content-type", "text/html; charset=utf-8"),
            line("x-page", "1")
        ]
    );
    let clean = stored(PAGE, TEMPLATE, &[], &[]);
    let record = check_visible(&clean, &values(&[], &[]), &vary_key()).unwrap();
    assert_eq!(record.fields, clean.body.fields);
}

#[test]
fn legacy_record_visible_in_another_partition_or_key_is_damaged() {
    // DL-229 (f): the entry must be the one of the reader's partition and
    // key; a mismatch is damage, before the `Vary` match.
    let (entry, session) = cookie_entry();
    let own = basis();
    let check_in = |partition: PartitionId, cache_key: CacheKey, session: &[FieldLine]| {
        check_visible_in(
            &entry,
            partition,
            cache_key,
            &values(session, &[]),
            &vary_key(),
        )
        .map(|_| ())
    };
    assert_eq!(check_in(own.partition, own.key, &session), Ok(()));
    let other_partition = PartitionId::from_bytes([9; 32]);
    let other_key = CacheKey::from_bytes([9; 32]);
    for (partition, cache_key) in [
        (other_partition, own.key),
        (own.partition, other_key),
        (other_partition, other_key),
    ] {
        assert_eq!(
            check_in(partition, cache_key, &session),
            Err(NotVisible::Damaged)
        );
        // Also for a request of another `Vary` value.
        assert_eq!(check_in(partition, cache_key, &[]), Err(NotVisible::Damaged));
    }
    assert_eq!(
        check_in(own.partition, own.key, &[]),
        Err(NotVisible::OtherVariant)
    );
}

#[test]
fn legacy_record_visible_without_vary_for_every_request() {
    let entry = stored(PAGE, TEMPLATE, &[], &[]);
    assert_eq!(check(&entry, &[], &[]), Ok(()));
    assert_eq!(
        check(&entry, &[line("cookie", "any")], &[line("x", "y")]),
        Ok(())
    );
}

#[test]
fn legacy_record_visible_finds_the_value_in_the_platform_fields() {
    let platform = vec![line("Accept-Language", "de")];
    let vary = vary_entries(
        &[line("vary", "accept-language")],
        &values(&[], &platform),
        &vary_key(),
    )
    .unwrap();
    let entry = stored(PAGE, TEMPLATE, &[], &vary);
    assert_eq!(check(&entry, &[], &platform), Ok(()));
    assert_eq!(check(&entry, &platform, &[]), Ok(()));
    assert_eq!(
        check(&entry, &[], &[line("accept-language", "fr")]),
        Err(NotVisible::OtherVariant)
    );
}

#[test]
fn legacy_record_visible_other_value_is_another_variant() {
    let (entry, _) = cookie_entry();
    assert_eq!(
        check(&entry, &[line("cookie", "sid=2")], &[]),
        Err(NotVisible::OtherVariant)
    );
    // An extra line of the name changes the value.
    assert_eq!(
        check(&entry, &[line("cookie", "sid=1")], &[line("cookie", "x=1")]),
        Err(NotVisible::OtherVariant)
    );
}

#[test]
fn legacy_record_visible_absent_and_present_values_differ() {
    let (entry, _) = cookie_entry();
    // The stored cookie is present; the request has none.
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::OtherVariant));
    // The stored accept-language is absent; the request has one.
    assert_eq!(
        check(
            &entry,
            &[line("cookie", "sid=1")],
            &[line("accept-language", "")]
        ),
        Err(NotVisible::OtherVariant)
    );
}

#[test]
fn legacy_record_visible_needs_the_same_key() {
    let (entry, session) = cookie_entry();
    let other = VaryKey(Secret::new([0x43; 32]));
    assert_eq!(
        check_visible(&entry, &values(&session, &[]), &other).map(|_| ()),
        Err(NotVisible::OtherVariant)
    );
}

#[test]
fn legacy_record_visible_other_protocol_is_damaged() {
    let mut entry = stored(PAGE, TEMPLATE, &[], &[]);
    entry.meta.protocol = Protocol::Rqp;
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));
}

#[test]
fn legacy_record_visible_vary_of_another_form_is_damaged() {
    let digest = Some(VaryDigest::from_bytes([3; 32]));
    let entry_of = |names: &[&str]| {
        let vary: Vec<VaryEntry> = names
            .iter()
            .map(|name| VaryEntry {
                name: (*name).to_owned(),
                value: digest,
            })
            .collect();
        stored(PAGE, TEMPLATE, &[], &vary)
    };
    for names in [
        &["Cookie"][..],
        &[""],
        &["co okie"],
        &["template-tag"],
        &["accept-diff"],
        &["if-none-match"],
        &["x-b", "x-a"],
        &["x-a", "x-a"],
        &["x-a", "é"],
    ] {
        assert_eq!(
            check(&entry_of(names), &[], &[]),
            Err(NotVisible::Damaged),
            "{names:?}"
        );
    }
    // The form is checked before the values: with the right form the same
    // request gives another variant.
    assert_eq!(
        check(&entry_of(&["x-a", "x-b"]), &[], &[]),
        Err(NotVisible::OtherVariant)
    );
    assert_eq!(
        check(&entry_of(&["!#$%&'*+-.^_`|~09az"]), &[], &[]),
        Err(NotVisible::OtherVariant)
    );
}

#[test]
fn legacy_record_visible_wrong_etag_or_template_tag_is_damaged() {
    let base = stored(PAGE, TEMPLATE, &[], &[]);
    let mut entry = base.clone();
    entry.meta.validators.etag = Some(sha1_hex(b"other").as_str().to_owned());
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));

    let mut entry = base.clone();
    entry.meta.validators.etag = None;
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));

    let mut entry = base.clone();
    entry.meta.validators.etag = Some(sha1_hex(PAGE.as_bytes()).as_str().to_ascii_uppercase());
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));

    let mut entry = base.clone();
    entry.meta.validators.template_tag = Some(sha1_hex(PAGE.as_bytes()).as_str().to_owned());
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));

    let mut entry = base;
    entry.meta.validators.template_tag = None;
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));
}

#[test]
fn legacy_record_visible_changed_page_or_template_is_damaged() {
    let base = stored(PAGE, TEMPLATE, &[], &[]);
    let mut entry = base.clone();
    entry.body.page = PageBytes::new(b"<html><p>1.00</p></html>".to_vec());
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));

    let mut entry = base;
    entry.body.template = Some(PageBytes::new(b"<html>{x}</html>".to_vec()));
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));
}

#[test]
fn legacy_record_visible_text_that_is_not_utf8_is_damaged() {
    let bad = vec![0x3c, 0xff, 0x3e];
    let base = stored(PAGE, TEMPLATE, &price(), &[]);

    // The tags match the bytes, so only the UTF-8 check fails.
    let mut entry = base.clone();
    entry.body.page = PageBytes::new(bad.clone());
    entry.meta.validators.etag = Some(sha1_hex(&bad).as_str().to_owned());
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));

    let mut entry = base.clone();
    entry.body.template = Some(PageBytes::new(bad.clone()));
    entry.meta.validators.template_tag = Some(sha1_hex(&bad).as_str().to_owned());
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));

    let mut entry = base.clone();
    entry.body.data[1].1 = PageBytes::new(bad);
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));

    let mut entry = base;
    entry.body.template = None;
    assert_eq!(check(&entry, &[], &[]), Err(NotVisible::Damaged));
}

#[test]
fn legacy_record_visible_vary_mismatch_comes_before_the_content_checks() {
    // Another variant stays in the store even when its content is damaged:
    // a request of other values never removes the entry of a value.
    let (mut entry, _) = cookie_entry();
    entry.body.page = PageBytes::new(b"damaged".to_vec());
    assert_eq!(
        check(&entry, &[line("cookie", "sid=2")], &[]),
        Err(NotVisible::OtherVariant)
    );
    assert_eq!(
        check(&entry, &[line("cookie", "sid=1")], &[]),
        Err(NotVisible::Damaged)
    );
}

#[test]
fn legacy_record_visible_is_check_visible_without_the_reason() {
    let (entry, session) = cookie_entry();
    let request = values(&session, &[]);
    assert_eq!(
        visible(&entry, &request, &vary_key()),
        check_visible(&entry, &request, &vary_key()).ok()
    );
    let other = [line("cookie", "sid=2")];
    assert_eq!(visible(&entry, &values(&other, &[]), &vary_key()), None);
}

// ---- properties ----

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

/// Field lines with a few names in mixed case, so that names repeat.
fn fields() -> impl Strategy<Value = Vec<FieldLine>> {
    prop::collection::vec(
        (
            prop_oneof![
                Just("cookie"),
                Just("Cookie"),
                Just("accept-language"),
                Just("X-A")
            ],
            "[ -~]{0,6}",
        )
            .prop_map(|(name, value)| line(name, &value)),
        0..4,
    )
}

fn vary_value() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("cookie".to_owned()),
        Just("Cookie, accept-language".to_owned()),
        Just("x-a, cookie, template-tag".to_owned()),
    ]
}

proptest! {
    #![proptest_config(config(256, 0x7e02_0001))]

    /// A committed record is visible to the request values of its commit
    /// and gives back its content.
    #[test]
    fn legacy_record_visible_prop_commit_round_trip(
        page in any::<String>(),
        template in any::<String>(),
        data in prop::collection::vec(("[{}a-z]{0,5}", any::<String>()), 0..4),
        session in fields(),
        platform in fields(),
        vary_field in vary_value(),
    ) {
        let data: Vec<DataBlock> = data
            .into_iter()
            .map(|(key, value)| DataBlock { key, value })
            .collect();
        let request = values(&session, &platform);
        let vary = vary_entries(&[line("vary", &vary_field)], &request, &vary_key()).unwrap();
        let entry = stored(&page, &template, &data, &vary);
        let record = check_visible(&entry, &request, &vary_key()).unwrap();
        prop_assert_eq!(record.page, page);
        prop_assert_eq!(record.template, template);
        prop_assert_eq!(record.data, data);
        prop_assert_eq!(record.vary, vary);
    }

    /// A request whose value of a `Vary` name differs never sees the entry.
    #[test]
    fn legacy_record_visible_prop_other_value_never_sees_the_entry(
        session in fields(),
        other in fields(),
        vary_field in vary_value(),
    ) {
        let request = values(&session, &[]);
        let vary = vary_entries(&[line("vary", &vary_field)], &request, &vary_key()).unwrap();
        let entry = stored(PAGE, TEMPLATE, &[], &vary);
        let other_vary =
            vary_entries(&[line("vary", &vary_field)], &values(&other, &[]), &vary_key()).unwrap();
        let result = check(&entry, &other, &[]);
        if other_vary == vary {
            prop_assert_eq!(result, Ok(()));
        } else {
            prop_assert_eq!(result, Err(NotVisible::OtherVariant));
        }
    }

    /// Any stored content gives a result, never a panic; a visible record
    /// always has the SHA-1 of its page and template as its tags (CL-40).
    #[test]
    fn legacy_record_visible_prop_any_entry_gives_a_checked_result(
        page in prop::collection::vec(any::<u8>(), 0..16),
        template in prop::option::of(prop::collection::vec(any::<u8>(), 0..16)),
        etag in prop::option::of("[0-9a-fA-F]{0,41}"),
        names in prop::collection::vec("[ -~]{0,4}", 0..4),
        session in fields(),
    ) {
        let mut entry = stored(PAGE, TEMPLATE, &[], &[]);
        entry.body.page = PageBytes::new(page);
        entry.body.template = template.map(PageBytes::new);
        entry.meta.validators.etag = etag;
        entry.meta.vary = names
            .into_iter()
            .map(|name| VaryEntry { name, value: None })
            .collect();
        let stored_etag = entry.meta.validators.etag.clone();
        if let Ok(record) = check_visible(&entry, &values(&session, &[]), &vary_key()) {
            prop_assert_eq!(Some(record.etag.as_str()), stored_etag.as_deref());
            prop_assert_eq!(record.etag, sha1_hex(record.page.as_bytes()));
            prop_assert_eq!(record.template_tag, sha1_hex(record.template.as_bytes()));
        }
    }
}
