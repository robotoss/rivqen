// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Engine configuration and its check (core API section 1; resource-limits
//! sections 1.1 and 3). Owner: WP-05 S2 T-04.
//!
//! [`Engine::new`] accepts a configuration only when every limit is inside
//! its safe range ([`Limit::range`]) and every pair of [`Limit::NOT_ABOVE`]
//! holds. The check only compares numbers: no arithmetic, no allocation
//! except the boxed cause of the error.

use core::error::Error;
use core::fmt;

use rivqen_model::context::{Context, RivqenError};
use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_model::limits::{Limit, ResourceLimits};

/// The operation name in the [`Context`] of a configuration error.
pub const CONFIG_OP: &str = "engine_new";

/// The configuration of an [`Engine`]. Later work packages add fields (stores,
/// trust policy); build it with [`EngineConfig::new`] or [`Default`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EngineConfig {
    /// The resource limits (resource-limits section 1).
    pub limits: ResourceLimits,
}

impl EngineConfig {
    /// A configuration with these limits.
    #[must_use]
    pub const fn new(limits: ResourceLimits) -> Self {
        Self { limits }
    }

    /// Checks every limit against its safe range, in the order of
    /// [`Limit::ALL`], then the pairs of [`Limit::NOT_ABOVE`]. The first
    /// failure wins.
    ///
    /// # Errors
    ///
    /// [`ConfigError`] with the limit and the kind of violation.
    pub fn check(&self) -> Result<(), ConfigError> {
        for limit in Limit::ALL {
            let value = self.limits.get(limit);
            let range = limit.range();
            if value < range.min() {
                return Err(ConfigError::new(limit, Violation::BelowMin));
            }
            if value > range.max() {
                return Err(ConfigError::new(limit, Violation::AboveMax));
            }
        }
        for (limit, bound) in Limit::NOT_ABOVE {
            if self.limits.get(limit) > self.limits.get(bound) {
                return Err(ConfigError::new(limit, Violation::Above(bound)));
            }
        }
        Ok(())
    }
}

/// How a limit breaks the rules of resource-limits section 1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Violation {
    /// The value is below the minimum of the range.
    BelowMin,
    /// The value is above the maximum of the range.
    AboveMax,
    /// The value is above the value of this other limit.
    Above(Limit),
}

/// A configuration value outside its safe range. It names the limit and the
/// rule; it never holds the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigError {
    limit: Limit,
    violation: Violation,
}

impl ConfigError {
    const fn new(limit: Limit, violation: Violation) -> Self {
        Self { limit, violation }
    }

    /// The limit that failed.
    #[must_use]
    pub const fn limit(&self) -> Limit {
        self.limit
    }

    /// The rule that failed.
    #[must_use]
    pub const fn violation(&self) -> Violation {
        self.violation
    }

    /// The configuration error inside an error of [`Engine::new`], if any.
    #[must_use]
    pub fn of(error: &RivqenError) -> Option<&Self> {
        error
            .source()
            .and_then(|cause| cause.downcast_ref::<Self>())
    }
}

impl HasErrorCode for ConfigError {
    /// `CONFIG_INVALID` (errors.md section 2, DL-158): a configuration
    /// outside the safe range is a bug of the host app; the adapter loads
    /// pages normally.
    fn code(&self) -> ErrorCode {
        ErrorCode::ConfigInvalid
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.limit.name();
        match self.violation {
            Violation::BelowMin => write!(f, "{name} is below its safe range"),
            Violation::AboveMax => write!(f, "{name} is above its safe range"),
            Violation::Above(bound) => write!(f, "{name} is above {}", bound.name()),
        }
    }
}

impl Error for ConfigError {}

/// The core engine. WP-05 has only the checked configuration; sessions, stores
/// and the event loop come with WP-07 and WP-08.
#[derive(Debug)]
pub struct Engine {
    config: EngineConfig,
}

impl Engine {
    /// Makes an engine from a checked configuration.
    ///
    /// # Errors
    ///
    /// `CONFIG_INVALID` in operation [`CONFIG_OP`] when a limit is outside its
    /// safe range; the cause is a [`ConfigError`] (see [`ConfigError::of`]).
    #[expect(
        clippy::result_large_err,
        reason = "core API section 1 returns RivqenError; one call per engine"
    )]
    pub fn new(config: EngineConfig) -> Result<Self, RivqenError> {
        match config.check() {
            Ok(()) => Ok(Self { config }),
            Err(cause) => {
                Err(RivqenError::new(cause.code(), Context::new(CONFIG_OP)).with_source(cause))
            }
        }
    }

    /// The configuration of this engine.
    #[must_use]
    pub const fn config(&self) -> &EngineConfig {
        &self.config
    }
}
