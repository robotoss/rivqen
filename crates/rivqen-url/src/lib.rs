// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Rivqen URL parser (WP-08 S1 contract; H-156, DL-218).
//!
//! The only crate with the dependency `url`. It reads one absolute `http`
//! or `https` URL from untrusted text and gives:
//!
//! - the URL U of the delta binding: the WHATWG serialization without the
//!   fragment (manifest and patch section 2.2, DL-060);
//! - the serialized origin and the path, for the manifest check (manifest
//!   and patch section 1.6);
//! - the canonical URL of the cache key (cache identity section 2, steps 1
//!   to 6).
//!
//! A host that is not ASCII, a URL with user information (H-68), another
//! scheme and a text over [`MAX_URL_BYTES`] are errors: the caller opens no
//! session and the web view loads normally; the session gives the catalog code
//! (S2). No function panics. `Debug` and
//! the error texts never hold URL bytes.
//!
//! The public items are the contract of the lanes T-04 and T-06: T-04 writes
//! the bodies and the private items.

#![forbid(unsafe_code)]

use core::fmt;

// The direct pin of H-156 (see Cargo.toml). The crate has no item to use.
use idna_adapter as _;
use url as _;

/// The longest input that [`parse`] reads, in bytes. A longer text is
/// [`UrlError::TooLong`].
pub const MAX_URL_BYTES: usize = 8192;

/// Why a text is not a URL that Rivqen handles. No variant holds input bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UrlError {
    /// The text is over [`MAX_URL_BYTES`].
    TooLong,
    /// The WHATWG parser rejects the text, or the URL is not absolute.
    Syntax,
    /// The scheme is not `http` or `https`.
    Scheme,
    /// The URL has a user name or a password (H-68).
    UserInfo,
    /// The host is absent, or it is not ASCII (H-156).
    Host,
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLong => "the URL is over the size limit",
            Self::Syntax => "the text is not an absolute URL",
            Self::Scheme => "the scheme is not http or https",
            Self::UserInfo => "the URL has user information",
            Self::Host => "the host is absent or not ASCII",
        })
    }
}

impl core::error::Error for UrlError {}

/// The scheme of a parsed URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// `http`.
    Http,
    /// `https`.
    Https,
}

/// A parsed absolute `http` or `https` URL without user information.
#[derive(Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    scheme: Scheme,
    /// The WHATWG serialization without the fragment.
    binding: String,
    /// The serialized origin (`scheme://host[:port]`, no default port).
    origin: String,
    /// Byte offset of the path in `binding`.
    path_start: usize,
    /// Byte offset of the `?` in `binding`, or its length.
    path_end: usize,
}

impl fmt::Debug for ParsedUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedUrl")
            .field("scheme", &self.scheme)
            .field("len", &self.binding.len())
            .finish_non_exhaustive()
    }
}

impl ParsedUrl {
    /// The scheme.
    #[must_use]
    pub const fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// The URL U of the delta binding: the WHATWG serialization without the
    /// fragment (manifest and patch section 2.2).
    #[must_use]
    pub fn binding_url(&self) -> &str {
        &self.binding
    }

    /// The serialized origin (manifest and patch section 1.6).
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The path, without the query (manifest and patch section 1.6).
    #[must_use]
    pub fn path(&self) -> &str {
        self.binding
            .get(self.path_start..self.path_end)
            .unwrap_or("/")
    }

    /// The canonical URL of the cache key (cache identity section 2, steps 1
    /// to 6): U with the percent-encoding normalization of step 4 and
    /// without the query parameters whose name is in `ignored_query_params`
    /// (step 6). The order of the other parameters does not change (step 5).
    #[must_use]
    pub fn cache_url(&self, ignored_query_params: &[&str]) -> String {
        // Contract stub (T-04).
        let _ = ignored_query_params;
        self.binding.clone()
    }
}

/// Parse one absolute URL.
///
/// # Errors
///
/// A [`UrlError`] for every text that is not an absolute `http` or `https`
/// URL with an ASCII host and without user information.
pub fn parse(input: &str) -> Result<ParsedUrl, UrlError> {
    // Contract stub (T-04): every input is rejected.
    if input.len() > MAX_URL_BYTES {
        return Err(UrlError::TooLong);
    }
    Err(UrlError::Syntax)
}
