// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The core error type (errors.md, errors-logging section 2.1).

use core::error::Error;
use core::fmt;

use crate::error::{Category, ErrorCode, FallbackHint, HasErrorCode};
use crate::ids::{Generation, RequestId, SessionId};

/// The boxed cause of an error.
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

/// Safe, structured context of an error: operation, state, ids and counts.
/// It has no field that can hold a URL, a header, a token or content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// The operation, for example `"split_template"`.
    pub op: &'static str,
    /// The session, when the error belongs to one.
    pub session: Option<SessionId>,
    /// The session generation.
    pub generation: Option<Generation>,
    /// The request.
    pub request: Option<RequestId>,
    /// A byte count that explains the error (a size, an offset).
    pub bytes: Option<u64>,
    /// The state name of the session machine.
    pub state: Option<&'static str>,
}

impl Context {
    /// A context with the operation only.
    #[must_use]
    pub const fn new(op: &'static str) -> Self {
        Self {
            op,
            session: None,
            generation: None,
            request: None,
            bytes: None,
            state: None,
        }
    }

    /// Sets the session.
    #[must_use]
    pub const fn with_session(mut self, session: SessionId) -> Self {
        self.session = Some(session);
        self
    }

    /// Sets the generation.
    #[must_use]
    pub const fn with_generation(mut self, generation: Generation) -> Self {
        self.generation = Some(generation);
        self
    }

    /// Sets the request.
    #[must_use]
    pub const fn with_request(mut self, request: RequestId) -> Self {
        self.request = Some(request);
        self
    }

    /// Sets the byte count.
    #[must_use]
    pub const fn with_bytes(mut self, bytes: u64) -> Self {
        self.bytes = Some(bytes);
        self
    }

    /// Sets the state name.
    #[must_use]
    pub const fn with_state(mut self, state: &'static str) -> Self {
        self.state = Some(state);
        self
    }
}

/// An error of the core: a catalog code, safe context and an optional cause.
///
/// [`fmt::Display`] writes the code and the operation only. [`fmt::Debug`]
/// writes the code, the context and whether a cause exists, never the cause
/// (it can hold data). The cause is reachable through [`Error::source`].
pub struct RivqenError {
    code: ErrorCode,
    context: Context,
    source: Option<BoxError>,
}

impl RivqenError {
    /// An error without a cause.
    #[must_use]
    pub const fn new(code: ErrorCode, context: Context) -> Self {
        Self {
            code,
            context,
            source: None,
        }
    }

    /// The same error with a cause.
    #[must_use]
    pub fn with_source(mut self, source: impl Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// The catalog code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// The category of the code.
    #[must_use]
    pub const fn category(&self) -> Category {
        self.code.category()
    }

    /// What the adapter does after this error.
    #[must_use]
    pub const fn fallback(&self) -> FallbackHint {
        self.code.fallback()
    }

    /// The safe context.
    #[must_use]
    pub const fn context(&self) -> &Context {
        &self.context
    }
}

impl HasErrorCode for RivqenError {
    fn code(&self) -> ErrorCode {
        self.code
    }
}

impl fmt::Display for RivqenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} in {}", self.code, self.context.op)
    }
}

impl fmt::Debug for RivqenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RivqenError")
            .field("code", &self.code)
            .field("context", &self.context)
            .field("has_source", &self.source.is_some())
            .finish()
    }
}

impl Error for RivqenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.source {
            Some(source) => Some(source.as_ref()),
            None => None,
        }
    }
}
