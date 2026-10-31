// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Server configuration of the RQP engine: the `server_config` of a control case
//! (CONTRACT.md §2; manifest and patch §1.4 `policy`, §1.5 `limits`) and the
//! server limit `maxPatchBytes`. Lane T-06.

/// The RQP server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RqpConfig {}

/// A `server_config` that the engine cannot use (the control endpoint answers
/// `400 BAD_CASE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// What is wrong, without input values (no secrets in logs).
    pub reason: &'static str,
}

impl RqpConfig {
    /// Read a `server_config` JSON object. `{}` gives the defaults.
    ///
    /// # Errors
    /// [`ConfigError`] when a member has a wrong type or value.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, ConfigError> {
        // Stub of the contract (WP-03 S1); lane T-06 replaces it.
        if value.is_object() {
            Ok(Self::default())
        } else {
            Err(ConfigError {
                reason: "server_config is not an object",
            })
        }
    }
}
