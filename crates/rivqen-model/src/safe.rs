// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Secret-safe wrappers (errors-logging section 4, ERR-06).
//!
//! None of these types prints a secret or page content through `Debug` or
//! `Display`.

use core::fmt;

use crate::ids::ValueError;

/// A token, cookie or key. [`fmt::Debug`] writes `Secret(<redacted>)`; there
/// is no `Display`. The value is reachable only through [`Secret::expose`].
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    /// Wraps a secret value.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// The secret value. Do not format it.
    #[must_use]
    pub const fn expose(&self) -> &T {
        &self.0
    }

    /// Takes the secret value out.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// URL scheme of a [`SafeUrl`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Scheme {
    /// `https`
    Https,
    /// `http` (a debug build with an explicit app flag only; H-49)
    Http,
}

impl Scheme {
    /// The scheme text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Http => "http",
        }
    }
}

/// A URL built from parts. [`fmt::Debug`] and [`fmt::Display`] write the
/// origin and the path only, for example `https://shop.example.com/catalog`:
/// never the query (it can hold a token) and never a fragment or user info.
///
/// URL parsing is not here (WP-08); the adapter or the parser gives the parts.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SafeUrl {
    scheme: Scheme,
    host: String,
    port: Option<u16>,
    path: String,
    query: Option<Secret<String>>,
}

impl SafeUrl {
    /// A URL without a query.
    ///
    /// # Errors
    ///
    /// [`ValueError`] when the host is empty or holds a byte outside
    /// `[A-Za-z0-9.-]`, or when the path does not start with `/` or holds a
    /// control byte, space, `?` or `#`.
    pub fn new(
        scheme: Scheme,
        host: &str,
        port: Option<u16>,
        path: &str,
    ) -> Result<Self, ValueError> {
        if host.is_empty() {
            return Err(ValueError::new("SafeUrl", "empty host"));
        }
        if !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return Err(ValueError::new("SafeUrl", "bad host byte"));
        }
        if !path.starts_with('/') {
            return Err(ValueError::new("SafeUrl", "path does not start with /"));
        }
        if path
            .bytes()
            .any(|b| b <= b' ' || b == 0x7f || b == b'?' || b == b'#')
        {
            return Err(ValueError::new("SafeUrl", "bad path byte"));
        }
        Ok(Self {
            scheme,
            host: host.to_ascii_lowercase(),
            port,
            path: path.to_owned(),
            query: None,
        })
    }

    /// The same URL with a query (the text after `?`, without it). The query
    /// is a [`Secret`]: it never appears in `Debug` or `Display`.
    ///
    /// # Errors
    ///
    /// [`ValueError`] when the query holds a control byte, space or `#`.
    pub fn with_query(mut self, query: Secret<String>) -> Result<Self, ValueError> {
        if query
            .expose()
            .bytes()
            .any(|b| b <= b' ' || b == 0x7f || b == b'#')
        {
            return Err(ValueError::new("SafeUrl", "bad query byte"));
        }
        self.query = Some(query);
        Ok(self)
    }

    /// The scheme.
    #[must_use]
    pub const fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// The host, in lowercase.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The explicit port, if any.
    #[must_use]
    pub const fn port(&self) -> Option<u16> {
        self.port
    }

    /// The path, starting with `/`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The query, if any.
    #[must_use]
    pub const fn query(&self) -> Option<&Secret<String>> {
        self.query.as_ref()
    }

    /// The origin: scheme, host and explicit port, for example
    /// `https://shop.example.com:8443`.
    #[must_use]
    pub fn origin(&self) -> String {
        match self.port {
            Some(port) => format!("{}://{}:{port}", self.scheme.as_str(), self.host),
            None => format!("{}://{}", self.scheme.as_str(), self.host),
        }
    }
}

impl fmt::Display for SafeUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.origin(), self.path)
    }
}

impl fmt::Debug for SafeUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Page or block content. [`fmt::Debug`] writes the length only
/// (`PageBytes(len=12345)`); there is no `Display`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PageBytes(Vec<u8>);

impl PageBytes {
    /// Wraps content bytes.
    #[must_use]
    pub const fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The content.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// The length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when there are no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Takes the bytes out.
    #[must_use]
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Debug for PageBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PageBytes(len={})", self.0.len())
    }
}
