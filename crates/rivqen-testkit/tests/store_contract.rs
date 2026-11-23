// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! The driver contract suite on `FakeStore` (WP-07 S1, T-05; DL-181).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rivqen_testkit::store::contract::{self, Checked};
use rivqen_testkit::store::{FakeStore, StopKind};

fn power_loss() -> FakeStore {
    FakeStore::new()
}

fn process_kill() -> FakeStore {
    FakeStore::new().with_stop_kind(StopKind::ProcessKill)
}

fn passes(result: Checked) {
    if let Err(violation) = result {
        panic!("{violation}");
    }
}

#[test]
fn store_contract_replace_then_read() {
    passes(contract::replace_then_read(&mut power_loss));
}

#[test]
fn store_contract_update_fields() {
    passes(contract::update_fields(&mut power_loss));
}

#[test]
fn store_contract_remove() {
    passes(contract::remove(&mut power_loss));
}

#[test]
fn store_contract_disable_key() {
    passes(contract::disable_key(&mut power_loss));
}

#[test]
fn store_contract_revoke_partition() {
    passes(contract::revoke_partition(&mut power_loss));
}

#[test]
fn store_contract_clear_site_every_partition() {
    passes(contract::clear_site_every_partition(&mut power_loss));
}

#[test]
fn store_contract_clear_site_keeps_other_sites() {
    passes(contract::clear_site_keeps_other_sites(&mut power_loss));
}

#[test]
fn store_contract_clear_site_all() {
    passes(contract::clear_site_all(&mut power_loss));
}

#[test]
fn store_contract_refused_entry() {
    passes(contract::refused_entry(&mut power_loss));
}

#[test]
fn store_contract_refused_partition_revoked() {
    passes(contract::refused_partition_revoked(&mut power_loss));
}

#[test]
fn store_contract_refused_site_cleared() {
    passes(contract::refused_site_cleared(&mut power_loss));
}

#[test]
fn store_contract_refused_request_partition_revoked() {
    passes(contract::refused_request_partition_revoked(&mut power_loss));
}

#[test]
fn store_contract_refused_request_partition_none_is_not_checked() {
    passes(contract::refused_request_partition_none_is_not_checked(
        &mut power_loss,
    ));
}

#[test]
fn store_contract_refusal_order() {
    passes(contract::refusal_order(&mut power_loss));
}

#[test]
fn store_contract_refused_invalid_host() {
    passes(contract::refused_invalid_host(&mut power_loss));
    passes(contract::refused_invalid_host(&mut process_kill));
}

#[test]
fn store_contract_recover_removes_expired_disabled_keys() {
    passes(contract::recover_removes_expired_disabled_keys(
        &mut power_loss,
    ));
    passes(contract::recover_removes_expired_disabled_keys(
        &mut process_kill,
    ));
}

#[test]
fn store_contract_refused_never_for_removals() {
    passes(contract::removals_are_never_refused(&mut power_loss));
}

#[test]
fn store_contract_partitions_never_mix() {
    passes(contract::partitions_never_mix(&mut power_loss));
}

#[test]
fn store_contract_corrupted() {
    passes(contract::corrupted(&mut power_loss));
    passes(contract::corrupted(&mut process_kill));
}

#[test]
fn store_contract_touch() {
    passes(contract::touch(&mut power_loss));
}

#[test]
fn store_contract_run_all() {
    passes(contract::run_all(&mut power_loss));
    passes(contract::run_all(&mut process_kill));
}

#[test]
fn store_contract_violation_names_the_check() {
    let violation = contract::Violation {
        check: "touch",
        detail: "x".to_owned(),
    };
    assert_eq!(violation.to_string(), "touch: x");
}
