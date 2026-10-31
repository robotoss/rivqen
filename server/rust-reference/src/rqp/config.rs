// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Server configuration of the RQP engine: the `server_config` of a control case
//! (CONTRACT.md §2; manifest and patch §1.4 `policy`, §1.5 `limits`) and the
//! server limit `maxPatchBytes` (resource limits; negotiation §4). Lane T-06.
//!
//! The configuration keeps the effective values only. The server writes no
//! manifest member with its default value and no empty `policy` or `limits`
//! (manifest and patch §1.2, H-103), so `"policy": {}`, `"policy":
//! {"mode": "private"}` and no `policy` give the same manifest and the same page
//! revision.
//!
//! `server_config` members (every other member, a wrong type or a value out of
//! range is a [`ConfigError`]; fail closed):
//!
//! | Member | Rule |
//! |---|---|
//! | `policy` | Object: `mode` (`"private"` or `"public"`), `ttl_seconds` (integer 0 to 31 536 000), `offline` (Boolean) (§1.4) |
//! | `limits` | Object: `max_patch_bytes` (integer 1 024 to 5 242 880), the hint of §1.5 |
//! | `maxPatchBytes` | Integer 1 to 5 242 880: the server limit of one envelope (default 262 144) |

use serde_json::{Map, Value};

/// The default of the server limit `maxPatchBytes` (resource limits): 256 KiB.
pub const DEFAULT_MAX_PATCH_BYTES: usize = 262_144;

/// The largest server limit that the configuration accepts: `maxDocumentBytes`
/// (a delta larger than a full document has no use; manifest and patch §1.5).
pub const MAX_PATCH_BYTES_LIMIT: u64 = 5_242_880;

/// The range of `policy.ttl_seconds` (manifest and patch §1.4).
pub const MAX_TTL_SECONDS: u64 = 31_536_000;

/// The range of `limits.max_patch_bytes` (manifest and patch §1.5).
pub const MIN_PATCH_BYTES_HINT: u64 = 1_024;

/// The cache mode of `policy.mode` (manifest and patch §1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `"private"`, the default.
    Private,
    /// `"public"`.
    Public,
}

/// The RQP server configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RqpConfig {
    mode: Mode,
    ttl_seconds: u32,
    offline: bool,
    patch_bytes_hint: Option<u32>,
    max_patch_bytes: usize,
}

impl Default for RqpConfig {
    /// The configuration of `{}`: all policy defaults, no hint, `maxPatchBytes`
    /// 262 144.
    fn default() -> Self {
        Self {
            mode: Mode::Private,
            ttl_seconds: 0,
            offline: false,
            patch_bytes_hint: None,
            max_patch_bytes: DEFAULT_MAX_PATCH_BYTES,
        }
    }
}

/// A `server_config` that the engine cannot use (the control endpoint answers
/// `400 BAD_CASE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// What is wrong, without input values (no secrets in logs).
    pub reason: &'static str,
}

const fn error(reason: &'static str) -> ConfigError {
    ConfigError { reason }
}

/// An integer member in `min..=max`: JSON digits only (manifest and patch §3.2
/// rule 4; `60.0` and `6e1` are not integers for `serde_json`).
fn integer(value: &Value, min: u64, max: u64, reason: &'static str) -> Result<u64, ConfigError> {
    value
        .as_u64()
        .filter(|n| (min..=max).contains(n))
        .ok_or(error(reason))
}

fn object<'a>(
    value: &'a Value,
    reason: &'static str,
) -> Result<&'a Map<String, Value>, ConfigError> {
    value.as_object().ok_or(error(reason))
}

impl RqpConfig {
    /// Read a `server_config` JSON object. `{}` gives the defaults.
    ///
    /// # Errors
    /// [`ConfigError`] when the value is not an object, or a member is unknown,
    /// has a wrong type or a value out of its range.
    pub fn from_json(value: &Value) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        for (name, member) in object(value, "server_config is not an object")? {
            match name.as_str() {
                "policy" => config.read_policy(member)?,
                "limits" => config.read_limits(member)?,
                "maxPatchBytes" => {
                    let n = integer(
                        member,
                        1,
                        MAX_PATCH_BYTES_LIMIT,
                        "server_config.maxPatchBytes is not an integer 1 to 5242880",
                    )?;
                    config.max_patch_bytes = usize::try_from(n)
                        .map_err(|_| error("server_config.maxPatchBytes is too large"))?;
                }
                _ => return Err(error("server_config has an unknown member")),
            }
        }
        Ok(config)
    }

    fn read_policy(&mut self, value: &Value) -> Result<(), ConfigError> {
        for (name, member) in object(value, "server_config.policy is not an object")? {
            match name.as_str() {
                "mode" => {
                    self.mode = match member.as_str() {
                        Some("private") => Mode::Private,
                        Some("public") => Mode::Public,
                        _ => return Err(error("policy.mode is not \"private\" or \"public\"")),
                    }
                }
                "ttl_seconds" => {
                    let n = integer(
                        member,
                        0,
                        MAX_TTL_SECONDS,
                        "policy.ttl_seconds is not an integer 0 to 31536000",
                    )?;
                    self.ttl_seconds =
                        u32::try_from(n).map_err(|_| error("policy.ttl_seconds is too large"))?;
                }
                "offline" => {
                    self.offline = member
                        .as_bool()
                        .ok_or(error("policy.offline is not a Boolean"))?;
                }
                _ => return Err(error("server_config.policy has an unknown member")),
            }
        }
        Ok(())
    }

    fn read_limits(&mut self, value: &Value) -> Result<(), ConfigError> {
        for (name, member) in object(value, "server_config.limits is not an object")? {
            if name != "max_patch_bytes" {
                return Err(error("server_config.limits has an unknown member"));
            }
            let n = integer(
                member,
                MIN_PATCH_BYTES_HINT,
                MAX_PATCH_BYTES_LIMIT,
                "limits.max_patch_bytes is not an integer 1024 to 5242880",
            )?;
            self.patch_bytes_hint =
                Some(u32::try_from(n).map_err(|_| error("limits.max_patch_bytes is too large"))?);
        }
        Ok(())
    }

    /// `policy.mode` (default [`Mode::Private`]).
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// `policy.ttl_seconds` (default 0).
    #[must_use]
    pub fn ttl_seconds(&self) -> u32 {
        self.ttl_seconds
    }

    /// `policy.offline` (default false).
    #[must_use]
    pub fn offline(&self) -> bool {
        self.offline
    }

    /// The server limit `maxPatchBytes` (default 262 144).
    #[must_use]
    pub fn max_patch_bytes(&self) -> usize {
        self.max_patch_bytes
    }

    /// The hint `limits.max_patch_bytes` that the manifest carries: only a hint
    /// smaller than `maxPatchBytes` (manifest and patch §1.5: a larger or equal
    /// hint bounds nothing; a server SHOULD NOT write it, RQP-27).
    #[must_use]
    pub fn manifest_hint(&self) -> Option<u32> {
        self.patch_bytes_hint
            .filter(|&hint| usize::try_from(hint).is_ok_and(|h| h < self.max_patch_bytes))
    }

    /// The largest envelope that the server sends: the smaller of
    /// `maxPatchBytes` and the hint (manifest and patch §2.5 item 5).
    #[must_use]
    pub fn patch_limit(&self) -> usize {
        self.patch_bytes_hint
            .and_then(|hint| usize::try_from(hint).ok())
            .map_or(self.max_patch_bytes, |hint| hint.min(self.max_patch_bytes))
    }

    /// The text S of the policy line (manifest and patch §1.3) of the manifest
    /// that this configuration writes, or `None` when the manifest has neither
    /// `policy` nor `limits` (then the preimage has no policy line).
    #[must_use]
    pub fn policy_line_text(&self) -> Option<String> {
        let mut items: Vec<String> = Vec::new();
        if self.writes_policy() {
            items.push("policy".to_owned());
            if self.mode == Mode::Public {
                items.push("mode=public".to_owned());
            }
            if self.ttl_seconds != 0 {
                items.push(format!("ttl_seconds={}", self.ttl_seconds));
            }
            if self.offline {
                items.push("offline=true".to_owned());
            }
        }
        if let Some(hint) = self.manifest_hint() {
            items.push("limits".to_owned());
            items.push(format!("max_patch_bytes={hint}"));
        }
        if items.is_empty() {
            None
        } else {
            Some(items.join(";"))
        }
    }

    /// The manifest has a `policy` member: one member is not its default.
    #[must_use]
    pub fn writes_policy(&self) -> bool {
        self.mode == Mode::Public || self.ttl_seconds != 0 || self.offline
    }
}
