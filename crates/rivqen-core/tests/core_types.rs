// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! No public type of the facade prints a planted marker or a configured value
//! (ERR-06, errors-logging section 4; the pattern of rivqen-model
//! `model_debug`). The placeholder payloads have no public constructor; their
//! `Debug` is checked inside the crate (`core_action_placeholders_*`,
//! `core_event_placeholders_*`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rivqen_core::action::Outcome;
use rivqen_core::config::{ConfigError, Engine, EngineConfig, Violation};
use rivqen_core::event::Event;
use rivqen_model::limits::{Limit, ResourceLimits};
use rivqen_model::time::Instant;

const MARKER: &str = "PLANTEDMARKER";
/// A configured value; it must not appear in an error text.
const PLANTED_VALUE: u64 = 4_242_424_242;

fn assert_clean(what: &str, text: &str) {
    assert!(!text.contains(MARKER), "{what} leaks the marker: {text}");
    assert!(
        !text.contains(&PLANTED_VALUE.to_string()),
        "{what} leaks the configured value: {text}"
    );
}

#[test]
fn core_types_debug_has_no_markers() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let limits = ResourceLimits {
        timeout_total_ms: PLANTED_VALUE,
        ..ResourceLimits::default()
    };
    let err = Engine::new(EngineConfig::new(limits)).unwrap_err();
    let cause = *ConfigError::of(&err).unwrap();

    let mut debug: Vec<(&str, String)> = vec![
        ("Engine", format!("{engine:?}")),
        ("EngineConfig", format!("{:?}", EngineConfig::default())),
        ("RivqenError", format!("{err:?}")),
        ("ConfigError", format!("{cause:?}")),
        (
            "Violation",
            format!("{:?}", Violation::Above(Limit::TimeoutTotal)),
        ),
        ("Outcome", format!("{:?}", Outcome::FirstLoad)),
        ("Outcome", format!("{:?}", Outcome::CacheHitNotModified)),
        ("Outcome", format!("{:?}", Outcome::DataUpdated)),
        ("Outcome", format!("{:?}", Outcome::TemplateChanged)),
        ("Outcome", format!("{:?}", Outcome::ServedStale)),
    ];
    for event in [
        Event::WebViewReady,
        Event::CacheResult(None),
        Event::HttpCompleted,
        Event::BridgeReady,
        Event::PageRendered,
        Event::NavigationCancelled,
        Event::PartitionChanged,
        Event::Tick(Instant(1)),
    ] {
        debug.push(("Event", format!("{event:?}")));
    }
    for (what, text) in &debug {
        assert_clean(what, text);
    }

    let display = [
        ("RivqenError", err.to_string()),
        ("ConfigError", cause.to_string()),
    ];
    for (what, text) in &display {
        assert_clean(what, text);
    }
    assert_eq!(cause.limit(), Limit::TimeoutTotal);
}
