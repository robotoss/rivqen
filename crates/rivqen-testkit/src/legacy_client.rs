// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The runner of the canonical legacy client fixtures (WP-08 S2, T-03):
//! it drives `rivqen_session::legacy::engine::LegacyEngine` with the steps
//! of `case.json` and the bytes of `exchanges.json`, executes the effects on
//! a fake web view, a fake network and the fake store, and builds the
//! projection of DL-110 that `expected.json` holds.
//!
//! Owner: WP-08 S2, lane L3.
//!
//! - [`fixture_ids`]: every `FX-LEG-CL-*` directory of `fixtures/legacy`
//!   without `--` (the canonical client fixtures; the `--ANDROID` overrides
//!   are not run, DL-215).
//! - [`load`]: `case.json` ([`CaseSpec`]), the `client` member of
//!   `expected.json` ([`Projection`]) and `exchanges.json`
//!   ([`crate::exchange`]).
//! - [`run`]: the projection of one run on a new [`LegacyEngine`] with
//!   [`legacy_config`]; [`run_with`] takes another [`SessionEngine`].
//! - [`compare`]: the differences of DL-110 in the order rules of the
//!   README.
//!
//! The runner never panics on a fixture: what it cannot run or map is a
//! problem of the projection ([`Projection::problems`]), and each problem is
//! a difference. It reads files only below the fixture root: a fixture id
//! is checked (`FX-LEG-CL-`, then `A-Z`, `0-9`, `-`, no `--`) before it
//! becomes a path, and page ids are checked by [`crate::exchange`].

use std::fs;
use std::path::Path;

use rivqen_proto_legacy::body::{BodyLimits, DATA_BODY_DEPTH};
use rivqen_session::legacy::engine::LegacyEngine;
use rivqen_session::legacy::types::{LegacyConfig, Presentation};
use rivqen_template::limits::TemplateLimits;
use serde_json::Value;

use crate::exchange::{self, ClientExchanges, MAX_BODY_BYTES};
use crate::fixtures::{FixtureError, load_json};
use crate::fixtures_root;

mod case;
mod compare;
mod driver;
mod json;
mod projection;

pub use case::{CaseConfig, CaseSpec, HoldPoint, Mode, Step};
pub use compare::{Difference, compare};
pub use driver::{
    MAX_PROBLEMS, MAX_STREAM_BYTES, MAX_TICKS_PER_SETTLE, MAX_WORK_PER_STEP, PLATFORM_FIELD,
    SessionEngine, WALL_BASE, partition_id, vary_key,
};
pub use projection::{
    Action, CookieCall, InterceptView, Pair, Phase, Projection, RecordView, RequestView,
    ResponseView, ResultView, SessionView, Snapshot,
};

/// The prefix of a client fixture directory.
pub const PREFIX: &str = "FX-LEG-CL-";
/// At most this many bytes in a `case.json` or an `expected.json`.
pub const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;

/// One canonical client fixture, loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientFixture {
    /// The fixture id (the directory name).
    pub id: String,
    /// What the runner reads of `case.json`.
    pub case: CaseSpec,
    /// The `client` member of `expected.json`.
    pub expected: Projection,
    /// The stored exchanges and `cache_before` records.
    pub exchanges: ClientExchanges,
}

/// True for the id of a canonical client fixture: `FX-LEG-CL-`, then
/// `A-Z`, `0-9` and `-`, without `--`.
#[must_use]
pub fn is_canonical_id(id: &str) -> bool {
    id.len() <= 128
        && id.strip_prefix(PREFIX).is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
        })
        && !id.contains("--")
}

/// The ids of the canonical client fixtures of `fixtures/legacy`, sorted.
///
/// # Errors
/// [`FixtureError::Io`] when the directory cannot be read.
pub fn fixture_ids() -> Result<Vec<String>, FixtureError> {
    fixture_ids_at(&fixtures_root().join("legacy"))
}

/// The ids of the canonical client fixtures below `root`: every directory
/// whose name is a canonical id ([`is_canonical_id`]), sorted.
///
/// # Errors
/// [`FixtureError::Io`] when the directory cannot be read.
pub fn fixture_ids_at(root: &Path) -> Result<Vec<String>, FixtureError> {
    let io = |source| FixtureError::Io {
        path: root.to_path_buf(),
        source,
    };
    let mut out = Vec::new();
    for entry in fs::read_dir(root).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_canonical_id(&name) && entry.file_type().map_err(io)?.is_dir() {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

/// Load the canonical client fixture `id` of `fixtures/legacy`.
///
/// # Errors
/// See [`load_at`].
pub fn load(id: &str) -> Result<ClientFixture, FixtureError> {
    load_at(&fixtures_root().join("legacy"), id)
}

/// Load the canonical client fixture `id` below `root` (pages in
/// `root/pages`).
///
/// # Errors
/// [`FixtureError::Shape`] for an id that is not canonical, a file over
/// [`MAX_JSON_BYTES`] or a member that breaks the README form;
/// [`FixtureError::Io`] and [`FixtureError::Json`] as the loaders.
pub fn load_at(root: &Path, id: &str) -> Result<ClientFixture, FixtureError> {
    if !is_canonical_id(id) {
        return Err(FixtureError::Shape {
            path: root.to_path_buf(),
            what: "not the id of a canonical client fixture".to_owned(),
        });
    }
    let dir = root.join(id);
    let case_path = dir.join("case.json");
    let case =
        CaseSpec::from_json(&bounded_json(&case_path)?).map_err(|what| shape(&case_path, what))?;
    let expected_path = dir.join("expected.json");
    let expected_json = bounded_json(&expected_path)?;
    let expected =
        expected_client(&expected_json, id).map_err(|what| shape(&expected_path, what))?;
    let exchanges = exchange::load_dir(&dir, &root.join("pages"))?;
    if case.id != id {
        return Err(shape(
            &case_path,
            format!("`id` is `{}`, not `{id}`", case.id),
        ));
    }
    Ok(ClientFixture {
        id: id.to_owned(),
        case,
        expected,
        exchanges,
    })
}

fn shape(path: &Path, what: String) -> FixtureError {
    FixtureError::Shape {
        path: path.to_path_buf(),
        what,
    }
}

fn bounded_json(path: &Path) -> Result<Value, FixtureError> {
    let size = fs::metadata(path)
        .map_err(|source| FixtureError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len();
    if size > MAX_JSON_BYTES {
        return Err(shape(
            path,
            format!("the file is over {MAX_JSON_BYTES} bytes"),
        ));
    }
    load_json(path)
}

/// The `client` member of a canonical client `expected.json` whose `id` is
/// `id`.
///
/// # Errors
/// The member that breaks the README form.
pub fn expected_client(value: &Value, id: &str) -> Result<Projection, String> {
    if value.get("id").and_then(Value::as_str) != Some(id) {
        return Err(format!("`id` is not `{id}`"));
    }
    if value.get("kind").and_then(Value::as_str) != Some("client")
        || value.get("profile").and_then(Value::as_str) != Some("legacy-canonical")
    {
        return Err("not a canonical client expected.json".to_owned());
    }
    Projection::from_json(value.get("client").ok_or("`client` is missing")?)
}

/// The session configuration of a case (CL-03): the presentation of the
/// mode, `accept_diff`, `authenticated_cache`, `unavailable_time_ms`; the
/// template limits of resource limits section 1; a data body of at most
/// [`MAX_BODY_BYTES`] bytes (the case limit) and the depth of a data body.
#[must_use]
pub fn legacy_config(case: &CaseSpec) -> LegacyConfig {
    LegacyConfig {
        presentation: match case.mode {
            Mode::Quick => Presentation::Quick,
            Mode::Standard => Presentation::Standard,
        },
        accept_diff: case.config.accept_diff,
        authenticated_cache: case.config.authenticated_cache,
        unavailable_time_ms: case.config.unavailable_time_ms,
        template: TemplateLimits::default(),
        body: BodyLimits::new(MAX_BODY_BYTES, DATA_BODY_DEPTH),
    }
}

/// Run `fixture` on a new [`LegacyEngine`] with [`legacy_config`].
#[must_use]
pub fn run(fixture: &ClientFixture) -> Projection {
    let mut engine = LegacyEngine::new(legacy_config(&fixture.case));
    run_with(fixture, &mut engine)
}

/// Run `fixture` on `engine` (a new engine: no session open).
#[must_use]
pub fn run_with<E: SessionEngine>(fixture: &ClientFixture, engine: &mut E) -> Projection {
    driver::Runner::new(fixture, engine).run()
}
