// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Test kit of the Rivqen core (DL-146). Used as a dev-dependency only.
//!
//! - [`fixtures`]: loads the committed fixtures of `fixtures/rqp/protocol`,
//!   `fixtures/legacy` and `fixtures/malformed` as data.
//! - [`ledger`]: the owner WP of each fixture (DL-145).
//! - [`markup`]: the RQP markup fixtures (WP-06).
//! - [`functions`]: the legacy function fixtures (WP-06).
//! - [`clock`]: a fake [`rivqen_model::time::Clock`].
//! - [`transport`]: a fake transport with scripted responses.
//!
//! The test kit reads files; the core crates do not.

#![forbid(unsafe_code)]

use std::path::PathBuf;

pub mod clock;
pub mod fixtures;
pub mod functions;
pub mod ledger;
pub mod markup;
pub mod transport;

/// The root directory of the repository.
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// The `fixtures/` directory of the repository.
#[must_use]
pub fn fixtures_root() -> PathBuf {
    repo_root().join("fixtures")
}
