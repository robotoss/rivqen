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
//! A host that is not ASCII, a host with a punycode label (a label that
//! starts with `xn--`, ASCII case-insensitive, also after the
//! percent-decoding; DL-225), a URL with user information (H-68), another
//! scheme and a text over [`MAX_URL_BYTES`] are errors: the caller opens no
//! session and the web view loads normally; the session gives the catalog code
//! (S2). No function panics, also with debug assertions. `Debug` and
//! the error texts never hold URL bytes.
//!
//! The public items are the contract of the lanes T-04 and T-06: T-04 writes
//! the bodies and the private items.

#![forbid(unsafe_code)]

use core::fmt;

// The direct pin of H-156 (see Cargo.toml): `url` reads it through `idna`.
// This crate names no item of it.
use idna_adapter as _;

mod canon;
mod scan;

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
    /// The host is absent, it is not ASCII (H-156), or it has a punycode
    /// label: a label that starts with `xn--`, ASCII case-insensitive, also
    /// in a percent-encoded form (DL-225). Without Unicode data the parser
    /// cannot check such a label, so it rejects all of them.
    Host,
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLong => "the URL is over the size limit",
            Self::Syntax => "the text is not an absolute URL",
            Self::Scheme => "the scheme is not http or https",
            Self::UserInfo => "the URL has user information",
            Self::Host => "the host is absent, not ASCII or punycode",
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
        canon::cache_url(
            &self.binding,
            self.path_start,
            self.path_end,
            ignored_query_params,
        )
    }
}

/// Parse one absolute URL.
///
/// # Errors
///
/// A [`UrlError`] for every text that is not an absolute `http` or `https`
/// URL with an ASCII host without a punycode label and without user
/// information.
pub fn parse(input: &str) -> Result<ParsedUrl, UrlError> {
    if input.len() > MAX_URL_BYTES {
        return Err(UrlError::TooLong);
    }
    // No host byte over 0x7F, no host label `xn--` and no user information
    // reach the parser (see the module `scan`).
    let scheme = scan::check(input)?;
    let mut url = url::Url::parse(input).map_err(|error| match error {
        url::ParseError::EmptyHost => UrlError::Host,
        _ => UrlError::Syntax,
    })?;
    // Fail closed when the parser and the scan disagree. With the scan above
    // these checks do not fail; they keep the result safe if a later `url`
    // reads the text in another way.
    let scheme_name = match scheme {
        Scheme::Http => "http",
        Scheme::Https => "https",
    };
    if url.scheme() != scheme_name {
        return Err(UrlError::Scheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(UrlError::UserInfo);
    }
    if !host_is_accepted(&url) {
        return Err(UrlError::Host);
    }

    url.set_fragment(None);
    let path_len = url.path().len();
    let query_len = url
        .query()
        .map_or(Some(0), |query| query.len().checked_add(1));
    let binding = String::from(url);
    if !binding.is_ascii() {
        return Err(UrlError::Syntax);
    }
    // U is `origin path [? query]`: with no user information, the text
    // before the path is the serialized origin.
    let tail_len = query_len
        .and_then(|query_len| query_len.checked_add(path_len))
        .ok_or(UrlError::Syntax)?;
    let path_start = binding
        .len()
        .checked_sub(tail_len)
        .ok_or(UrlError::Syntax)?;
    let path_end = path_start.checked_add(path_len).ok_or(UrlError::Syntax)?;
    let origin = binding
        .get(..path_start)
        .ok_or(UrlError::Syntax)?
        .to_owned();
    Ok(ParsedUrl {
        scheme,
        binding,
        origin,
        path_start,
        path_end,
    })
}

/// The host check after the parse: a host is present, and no label of it
/// starts with `xn--` (DL-225; the scan already rejects such a host, so this
/// is defense in depth).
fn host_is_accepted(url: &url::Url) -> bool {
    url.host_str()
        .is_some_and(|host| !host.is_empty() && !scan::has_punycode_label(host.as_bytes()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::host_is_accepted;

    #[test]
    fn host_check_after_parse() {
        // `url` alone accepts these hosts; the check after the parse rejects
        // them. (A valid punycode label does not reach a debug assertion.)
        for text in [
            "https://xn--bcher-kva.example/",
            "https://a.XN--BCHER-KVA.example/",
            "https://a.xn--bcher-kva/",
        ] {
            let url = url::Url::parse(text).unwrap();
            assert!(!host_is_accepted(&url), "{text}");
        }
        for text in [
            "https://a.example/",
            "https://ab--c.example/",
            "https://[::1]/",
            "https://127.1/",
        ] {
            let url = url::Url::parse(text).unwrap();
            assert!(host_is_accepted(&url), "{text}");
        }
        // No host (a URL that `parse` never gives to this check).
        assert!(!host_is_accepted(&url::Url::parse("data:x").unwrap()));
        assert!(!host_is_accepted(&url::Url::parse("foo://").unwrap()));
    }
}
