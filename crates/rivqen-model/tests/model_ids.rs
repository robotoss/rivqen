// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! Identifier and revision tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use proptest::prelude::*;
use rivqen_model::ids::{Generation, PartitionId, RequestId, Revision, SessionId};

#[test]
fn model_ids_session_debug_shows_first_four_bytes_only() {
    let mut bytes = [0xee_u8; 16];
    bytes[..4].copy_from_slice(&[0x01, 0x02, 0x03, 0x04]);
    let id = SessionId::from_bytes(bytes);
    assert_eq!(format!("{id:?}"), "SessionId(01020304)");
    assert_eq!(id.as_bytes(), &bytes);
}

#[test]
fn model_ids_partition_debug_shows_first_eight_hex_only() {
    let mut bytes = [0xee_u8; 32];
    bytes[..4].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    let id = PartitionId::from_bytes(bytes);
    assert_eq!(format!("{id:?}"), "PartitionId(deadbeef)");
    assert_eq!(id.as_bytes(), &bytes);
}

#[test]
fn model_ids_counters_step_by_one_and_stop_at_the_end() {
    assert_eq!(Generation::FIRST.next(), Some(Generation::new(1)));
    assert_eq!(Generation::new(u64::MAX).next(), None);
    assert_eq!(RequestId::new(41).next().map(RequestId::get), Some(42));
    assert_eq!(RequestId::new(u64::MAX).next(), None);
    assert!(Generation::new(2) > Generation::new(1));
}

#[test]
fn model_ids_revision_accepts_rqp_and_legacy_forms() {
    let rqp = format!("t1.{}", "A".repeat(43));
    assert_eq!(Revision::new(&rqp).unwrap().as_str(), rqp);
    let legacy = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
    assert_eq!(Revision::new(legacy).unwrap().to_string(), legacy);
}

#[test]
fn model_ids_revision_limit_is_128_bytes() {
    assert!(Revision::new(&"a".repeat(128)).is_ok());
    let err = Revision::new(&"a".repeat(129)).unwrap_err();
    assert_eq!(err.reason, "too long");
}

#[test]
fn model_ids_revision_refuses_empty_space_control_and_non_ascii() {
    for bad in ["", "a b", " a", "a\n", "a\u{7f}", "é", "a\0"] {
        assert!(Revision::new(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn model_ids_error_does_not_echo_the_input() {
    let err = Revision::new("secret value").unwrap_err();
    let text = format!("{err} {err:?}");
    assert!(!text.contains("secret"));
}

proptest! {
    #[test]
    fn model_ids_revision_accepts_exactly_short_visible_ascii(s in "\\PC{0,200}") {
        let expected = !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| (0x21..=0x7e).contains(&b));
        prop_assert_eq!(Revision::new(&s).is_ok(), expected);
    }

    #[test]
    fn model_ids_revision_accepts_all_visible_ascii_up_to_128(s in "[!-~]{1,128}") {
        prop_assert!(Revision::new(&s).is_ok());
    }

    #[test]
    fn model_ids_revision_refuses_visible_ascii_over_128(s in "[!-~]{129,200}") {
        prop_assert!(Revision::new(&s).is_err());
    }
}

#[test]
fn model_ids_value_error_display_names_kind_and_reason() {
    let e = Revision::new("").unwrap_err();
    assert_eq!(e.to_string(), "invalid Revision: empty");
}

#[test]
fn model_ids_generation_get_returns_the_number() {
    assert_eq!(Generation::new(7).get(), 7);
    assert_eq!(Generation::new(u64::MAX).get(), u64::MAX);
    assert_eq!(Generation::FIRST.get(), 0);
}
