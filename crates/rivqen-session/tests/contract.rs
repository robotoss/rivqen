// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Contract of the legacy session (WP-08 S2): what the stubs and the product
//! both guarantee.
#![cfg(feature = "legacy")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use rivqen_cache::txn::{Epoch, Expect, TxnOp, Writer};
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_session::legacy::record::{WriteBasis, remove};
use rivqen_session::legacy::types::{DEFAULT_UNAVAILABLE_TIME_MS, DIFF_MAX_AGE_MS, Effect};

#[test]
fn session_constants_of_the_rules() {
    assert_eq!(DIFF_MAX_AGE_MS, 30_000);
    assert_eq!(DEFAULT_UNAVAILABLE_TIME_MS, 21_600_000);
}

/// DL-220: every write of a session names the request partition.
#[test]
fn session_write_names_the_request_partition() {
    let basis = WriteBasis {
        partition: PartitionId::from_bytes([5; 32]),
        key: CacheKey::from_bytes([6; 32]),
        partition_epoch: Epoch(3),
        site_epoch: Epoch(4),
        expect: Expect::Any,
    };
    let txn = remove(&basis);
    assert_eq!(
        txn.pre.writer,
        Writer::request(PartitionId::from_bytes([5; 32]), Epoch(3))
    );
    assert_eq!(txn.pre.site_epoch, Epoch(4));
    assert!(matches!(txn.op, TxnOp::Remove { .. }));
}

#[test]
fn session_effect_debug_has_no_content() {
    let text = format!(
        "{:?}",
        Effect::Message("{\"result\":\"secret\"}".to_owned())
    );
    assert!(!text.contains("secret"), "{text}");
    let text = format!("{:?}", Effect::StoreCookies(vec!["sid=secret".to_owned()]));
    assert!(!text.contains("secret"), "{text}");
}
