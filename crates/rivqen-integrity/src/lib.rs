// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Hashes and identifiers of the Rivqen core (DL-143).
//!
//! SHA-256 and base64url without padding (`B64U`) of
//! docs/engineering/protocol/manifest-patch.md section 1.3, and the three
//! RQP identifiers `t1.` (template revision), `r1.` (page revision) and `d1.`
//! (binding). An identifier is the prefix and 43 base64url characters whose
//! last character is canonical (`[AEIMQUYcgkosw048]`, the schemas of
//! `spec/rqp/schemas/`). Values are compared byte for byte.
//!
//! WP-06 adds SHA-1 for the legacy protocol.

#![forbid(unsafe_code)]

use core::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest as _, Sha256};

/// Length of a SHA-256 digest in bytes.
pub const DIGEST_BYTES: usize = 32;
/// Length of the base64url text of a digest, without padding.
pub const DIGEST_B64U_CHARS: usize = 43;
/// Length of a digest prefix in `Rq-Have` (negotiation section 3.4).
pub const PREFIX_BYTES: usize = 8;

/// A SHA-256 digest.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Digest([u8; DIGEST_BYTES]);

impl Sha256Digest {
    /// The digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
        &self.0
    }

    /// The first [`PREFIX_BYTES`] bytes (the `Rq-Have` member `b`).
    #[must_use]
    pub fn prefix(&self) -> [u8; PREFIX_BYTES] {
        let mut out = [0_u8; PREFIX_BYTES];
        for (o, b) in out.iter_mut().zip(self.0.iter()) {
            *o = *b;
        }
        out
    }

    /// `B64U(digest)`: 43 characters.
    #[must_use]
    pub fn to_b64u(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }

    /// Parse the 43-character base64url text of a digest. Rejects padding,
    /// another length, a character outside the alphabet and a last character
    /// that is not canonical.
    ///
    /// # Errors
    ///
    /// [`IdError`] when the text is not a canonical digest.
    pub fn from_b64u(text: &str) -> Result<Self, IdError> {
        if text.len() != DIGEST_B64U_CHARS {
            return Err(IdError::Length);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(text.as_bytes())
            .map_err(|_| IdError::Encoding)?;
        let digest: [u8; DIGEST_BYTES] = bytes.try_into().map_err(|_| IdError::Length)?;
        Ok(Self(digest))
    }
}

/// Writes the first 8 base64url characters only, e.g.
/// `Sha256Digest(0Aqua3-_...)`: the full hash of a block would allow
/// guess-testing of its content in logs (H-60).
impl fmt::Debug for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = self.to_b64u();
        write!(f, "Sha256Digest({}...)", text.get(..8).unwrap_or_default())
    }
}

/// `SHA-256(data)`.
#[must_use]
pub fn sha256(data: &[u8]) -> Sha256Digest {
    sha256_parts(&[data])
}

/// `SHA-256` of the concatenation of `parts`, without a copy.
#[must_use]
pub fn sha256_parts(parts: &[&[u8]]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    Sha256Digest(hasher.finalize().into())
}

/// `B64U(bytes)`: base64url without padding.
#[must_use]
pub fn b64u_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Why a text is not a digest or an identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdError {
    /// The prefix is not the one of the identifier.
    Prefix,
    /// The text has another length.
    Length,
    /// A character is outside the base64url alphabet, or the last character
    /// is not canonical.
    Encoding,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Prefix => "identifier has another prefix",
            Self::Length => "identifier has another length",
            Self::Encoding => "identifier is not canonical base64url",
        })
    }
}

impl core::error::Error for IdError {}

macro_rules! identifier {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(Sha256Digest);

        impl $name {
            /// The prefix of this identifier.
            pub const PREFIX: &'static str = $prefix;

            /// The identifier of a digest.
            #[must_use]
            pub const fn from_digest(digest: Sha256Digest) -> Self {
                Self(digest)
            }

            /// The digest.
            #[must_use]
            pub const fn digest(&self) -> &Sha256Digest {
                &self.0
            }

            /// Parse the text form: the prefix and a canonical digest.
            ///
            /// # Errors
            ///
            /// [`IdError`] when the text is not a canonical identifier.
            pub fn parse(text: &str) -> Result<Self, IdError> {
                let rest = text.strip_prefix(Self::PREFIX).ok_or(IdError::Prefix)?;
                Sha256Digest::from_b64u(rest).map(Self)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", Self::PREFIX, self.0.to_b64u())
            }
        }

        /// Writes the prefix and the first 8 base64url characters only (as
        /// [`Sha256Digest`]); [`fmt::Display`] writes the full wire text.
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let text = self.0.to_b64u();
                write!(
                    f,
                    "{}({}{}...)",
                    stringify!($name),
                    Self::PREFIX,
                    text.get(..8).unwrap_or_default()
                )
            }
        }
    };
}

identifier!(
    /// Template revision `t1.` (manifest-patch section 1.3, step 3).
    TemplateRevision,
    "t1."
);
identifier!(
    /// Page revision `r1.` (manifest-patch section 1.3, step 5).
    PageRevision,
    "r1."
);
identifier!(
    /// Binding `d1.` of a patch envelope (manifest-patch section 2.2).
    Binding,
    "d1."
);
