// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `RqpConfig::from_json`: `policy` and `limits` of manifest and patch §1.4 and
//! §1.5, the server limit `maxPatchBytes`; the manifest members and the policy
//! line that a configuration writes (H-55, H-103).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use rivqen_reference::rqp::config::{DEFAULT_MAX_PATCH_BYTES, Mode, RqpConfig};
use serde_json::{Value, json};

fn ok(v: &Value) -> RqpConfig {
    RqpConfig::from_json(v).unwrap_or_else(|e| panic!("{v}: {}", e.reason))
}

#[test]
fn rqp_config_empty_object_is_the_default() {
    let c = ok(&json!({}));
    assert_eq!(c, RqpConfig::default());
    assert_eq!(c.mode(), Mode::Private);
    assert_eq!(c.ttl_seconds(), 0);
    assert!(!c.offline());
    assert_eq!(c.max_patch_bytes(), DEFAULT_MAX_PATCH_BYTES);
    assert_eq!(c.max_patch_bytes(), 262_144);
    assert_eq!(c.manifest_hint(), None);
    assert_eq!(c.patch_limit(), 262_144);
    assert_eq!(c.policy_line_text(), None);
    assert!(!c.writes_policy());
}

#[test]
fn rqp_config_every_fixture_server_config_is_accepted() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rqp/protocol");
    let mut seen = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path().join("expected.json");
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let expected: Value = serde_json::from_str(&text).unwrap();
        if let Some(config) = expected.get("server_config") {
            ok(config);
            seen += 1;
        }
    }
    assert!(seen >= 4, "{seen}");
}

#[test]
fn rqp_config_reads_policy_and_limits() {
    let c = ok(&json!({
        "policy": {"mode": "public", "ttl_seconds": 31_536_000, "offline": true},
        "limits": {"max_patch_bytes": 1024},
        "maxPatchBytes": 5_242_880
    }));
    assert_eq!(c.mode(), Mode::Public);
    assert_eq!(c.ttl_seconds(), 31_536_000);
    assert!(c.offline());
    assert_eq!(c.max_patch_bytes(), 5_242_880);
    assert_eq!(c.manifest_hint(), Some(1024));
    assert_eq!(c.patch_limit(), 1024);
    assert_eq!(
        c.policy_line_text().as_deref(),
        Some("policy;mode=public;ttl_seconds=31536000;offline=true;limits;max_patch_bytes=1024")
    );
}

#[test]
fn rqp_config_default_members_are_not_written() {
    // H-103: FX-RQ-P36-POLICY-DEFAULTS.
    let c = ok(&json!({
        "policy": {"mode": "private", "ttl_seconds": 0, "offline": false},
        "limits": {}
    }));
    assert_eq!(c, RqpConfig::default());
    assert_eq!(c.policy_line_text(), None);
    let one = ok(&json!({"policy": {"offline": true}}));
    assert_eq!(
        one.policy_line_text().as_deref(),
        Some("policy;offline=true")
    );
    let mode = ok(&json!({"policy": {"mode": "public"}}));
    assert_eq!(
        mode.policy_line_text().as_deref(),
        Some("policy;mode=public")
    );
}

#[test]
fn rqp_config_hint_not_smaller_than_the_server_limit_is_not_written() {
    // RQP-27 (SHOULD): FX-RQ-P36-LIMITS-SERVER-LIMIT.
    let equal = ok(&json!({"limits": {"max_patch_bytes": 262_144}}));
    assert_eq!(equal.manifest_hint(), None);
    assert_eq!(equal.policy_line_text(), None);
    assert_eq!(equal.patch_limit(), 262_144);
    let below = ok(&json!({"limits": {"max_patch_bytes": 262_143}}));
    assert_eq!(below.manifest_hint(), Some(262_143));
    assert_eq!(below.patch_limit(), 262_143);
    let server = ok(&json!({"limits": {"max_patch_bytes": 4096}, "maxPatchBytes": 2000}));
    assert_eq!(server.manifest_hint(), None);
    assert_eq!(server.patch_limit(), 2000);
    let server_at = ok(&json!({"limits": {"max_patch_bytes": 4096}, "maxPatchBytes": 4097}));
    assert_eq!(server_at.manifest_hint(), Some(4096));
}

#[test]
fn rqp_config_wrong_types_and_values_are_errors() {
    for bad in [
        json!(null),
        json!([]),
        json!("x"),
        json!({"other": 1}),
        json!({"policy": []}),
        json!({"policy": {"mode": "shared"}}),
        json!({"policy": {"mode": 1}}),
        json!({"policy": {"ttl_seconds": -1}}),
        json!({"policy": {"ttl_seconds": 31_536_001}}),
        json!({"policy": {"ttl_seconds": 60.0}}),
        json!({"policy": {"ttl_seconds": "60"}}),
        json!({"policy": {"offline": "true"}}),
        json!({"policy": {"extra": true}}),
        json!({"limits": null}),
        json!({"limits": {"max_patch_bytes": 1023}}),
        json!({"limits": {"max_patch_bytes": 5_242_881}}),
        json!({"limits": {"max_patch_bytes": 1e4}}),
        json!({"limits": {"other": 1}}),
        json!({"maxPatchBytes": 0}),
        json!({"maxPatchBytes": 5_242_881}),
        json!({"maxPatchBytes": "1"}),
    ] {
        let e = RqpConfig::from_json(&bad).expect_err(&bad.to_string());
        assert_ne!(e.reason, "");
    }
    for good in [
        json!({"policy": {"ttl_seconds": 0}}),
        json!({"limits": {"max_patch_bytes": 5_242_880}}),
        json!({"maxPatchBytes": 1}),
        json!({"policy": {}}),
    ] {
        ok(&good);
    }
}
