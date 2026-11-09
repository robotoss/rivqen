// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Page manifest text to [`crate::types::PageManifest`] (manifest-patch
//! sections 1.2 to 1.6 and 1.8, items without the markup analysis).
//! Owner: WP-05 S1 T-04.
//!
//! [`read_manifest`] runs the checks of manifest-patch section 1.8 that need
//! no document, in this order:
//!
//! 1. Item 1: the text is at most [`MAX_MANIFEST_BYTES`], checked before the
//!    JSON parse.
//! 2. Item 2: the text is a JSON text by section 3.2 ([`crate::json`]) and is
//!    valid against `spec/rqp/schemas/manifest.schema.json`. The check reads
//!    the text once and stops at the first failure; it keeps no value tree.
//! 3. Item 4: `origin` and `path` are byte for byte the origin and the path
//!    of the final response URL (section 1.6, rule 1).
//!
//! Item 5 (the markup analysis and the page revision check) is WP-06. Every
//! failure is `MANIFEST_INVALID`; the error holds no byte of the text.

use core::fmt;

use rivqen_integrity::PageRevision;
use rivqen_model::error::{ErrorCode, HasErrorCode};

use crate::json::{
    Item, JsonError, Reader, SchemaKeyword, SchemaViolation, next_item, next_member,
};
use crate::types::{Limits, MAX_JSON_DEPTH, MAX_MANIFEST_BYTES, Mode, PageManifest, Policy};

/// `origin`: at most 512 characters (section 1.6).
const MAX_ORIGIN_CHARS: usize = 512;
/// `path`: at most 4 096 characters (section 1.6).
const MAX_PATH_CHARS: usize = 4096;
/// `policy.ttl_seconds`: at most one year (section 1.4).
const MAX_TTL_SECONDS: u64 = 31_536_000;
/// `limits.max_patch_bytes`: 1 024 to 5 242 880 (section 1.5).
const MIN_PATCH_HINT: u64 = 1024;
const MAX_PATCH_HINT: u64 = 5_242_880;

/// Why a manifest is invalid. Every variant is `MANIFEST_INVALID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ManifestError {
    /// The text is larger than [`MAX_MANIFEST_BYTES`] (section 1.8, item 1).
    TooLarge,
    /// The text breaks a JSON rule of section 3.2.
    Json(JsonError),
    /// The value fails `manifest.schema.json`.
    Schema(SchemaViolation),
    /// `origin` or `path` is not the one of the final response URL (section
    /// 1.6, rule 1).
    UrlMismatch,
}

impl ManifestError {
    /// A static reason without content.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::TooLarge => "manifest text larger than maxManifestBytes",
            Self::Json(_) => "manifest text breaks a JSON rule",
            Self::Schema(_) => "manifest fails the schema",
            Self::UrlMismatch => "manifest origin or path is not the final URL",
        }
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.reason())?;
        match self {
            Self::Json(e) => write!(f, ": {e}"),
            Self::Schema(v) => write!(f, ": {v}"),
            Self::TooLarge | Self::UrlMismatch => Ok(()),
        }
    }
}

impl core::error::Error for ManifestError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            Self::Schema(v) => Some(v),
            Self::TooLarge | Self::UrlMismatch => None,
        }
    }
}

impl HasErrorCode for ManifestError {
    fn code(&self) -> ErrorCode {
        ErrorCode::ManifestInvalid
    }
}

impl From<JsonError> for ManifestError {
    fn from(e: JsonError) -> Self {
        Self::Json(e)
    }
}

impl From<SchemaViolation> for ManifestError {
    fn from(v: SchemaViolation) -> Self {
        Self::Schema(v)
    }
}

fn violation(keyword: SchemaKeyword, pointer: &'static str) -> ManifestError {
    ManifestError::Schema(SchemaViolation::at(keyword, pointer))
}

/// Section 1.8, items 1, 2 and 4: parse the manifest text and compare its
/// `origin` and `path` with the final response URL.
///
/// `final_origin` is the ASCII serialization of the origin and `final_path`
/// the serialized path of the final URL, both from the URL object of the
/// network stack (section 1.6, rule 1).
///
/// # Errors
///
/// A [`ManifestError`] for the first failed check, in the order of section
/// 1.8.
pub fn read_manifest(
    text: &[u8],
    final_origin: &str,
    final_path: &str,
) -> Result<PageManifest, ManifestError> {
    let manifest = parse_manifest(text)?;
    check_url(&manifest, final_origin, final_path)?;
    Ok(manifest)
}

/// Section 1.8, item 4: `origin` and `path` equal the origin and the path of
/// the final response URL, byte for byte.
///
/// # Errors
///
/// [`ManifestError::UrlMismatch`] on a difference.
pub fn check_url(
    manifest: &PageManifest,
    final_origin: &str,
    final_path: &str,
) -> Result<(), ManifestError> {
    if manifest.origin.as_bytes() == final_origin.as_bytes()
        && manifest.path.as_bytes() == final_path.as_bytes()
    {
        Ok(())
    } else {
        Err(ManifestError::UrlMismatch)
    }
}

/// Section 1.8, items 1 and 2: the size, the JSON rules of section 3.2 and the
/// schema. `policy` and `limits` keep member presence: `Some` with the members
/// of the text (also `{}` and default members), `None` when absent.
///
/// # Errors
///
/// A [`ManifestError`] for the first failure: [`ManifestError::TooLarge`]
/// before the parse, then the first JSON or schema failure in text order.
pub fn parse_manifest(text: &[u8]) -> Result<PageManifest, ManifestError> {
    if text.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestError::TooLarge);
    }
    let mut reader = Reader::new(text, MAX_JSON_DEPTH)?;
    let Item::Object = next_item(&mut reader)? else {
        return Err(violation(SchemaKeyword::Type, ""));
    };
    let mut protocol = false;
    let mut origin = None;
    let mut path = None;
    let mut page_revision = None;
    let mut policy = None;
    let mut limits = None;
    // The reader rejects a second member with the same name (rule 2), so
    // each arm runs at most once.
    while let Some(name) = next_member(&mut reader)? {
        match &*name {
            "protocol" => {
                read_protocol(&mut reader)?;
                protocol = true;
            }
            "origin" => origin = Some(read_origin(&mut reader)?),
            "path" => path = Some(read_path(&mut reader)?),
            "page_revision" => page_revision = Some(read_page_revision(&mut reader)?),
            "policy" => policy = Some(read_policy(&mut reader)?),
            "limits" => limits = Some(read_limits(&mut reader)?),
            _ => return Err(violation(SchemaKeyword::AdditionalProperties, "")),
        }
    }
    let (true, Some(origin), Some(path), Some(page_revision)) =
        (protocol, origin, path, page_revision)
    else {
        return Err(violation(SchemaKeyword::Required, ""));
    };
    reader.finish()?;
    Ok(PageManifest {
        origin,
        path,
        page_revision,
        policy,
        limits,
    })
}

/// `protocol`: the string `"rqp/1"` (`const`).
fn read_protocol(reader: &mut Reader<'_>) -> Result<(), ManifestError> {
    match next_item(reader)? {
        Item::String(s) if s == "rqp/1" => Ok(()),
        _ => Err(violation(SchemaKeyword::Const, "/protocol")),
    }
}

/// `origin`: a string of at most 512 characters with the schema pattern.
fn read_origin(reader: &mut Reader<'_>) -> Result<String, ManifestError> {
    const AT: &str = "/origin";
    let Item::String(s) = next_item(reader)? else {
        return Err(violation(SchemaKeyword::Type, AT));
    };
    if s.chars().count() > MAX_ORIGIN_CHARS {
        return Err(violation(SchemaKeyword::MaxLength, AT));
    }
    if !is_origin(&s) {
        return Err(violation(SchemaKeyword::Pattern, AT));
    }
    Ok(s.into_owned())
}

/// `path`: a string of 1 to 4 096 characters with the schema pattern.
fn read_path(reader: &mut Reader<'_>) -> Result<String, ManifestError> {
    const AT: &str = "/path";
    let Item::String(s) = next_item(reader)? else {
        return Err(violation(SchemaKeyword::Type, AT));
    };
    if s.is_empty() {
        return Err(violation(SchemaKeyword::MinLength, AT));
    }
    if s.chars().count() > MAX_PATH_CHARS {
        return Err(violation(SchemaKeyword::MaxLength, AT));
    }
    if !is_path(&s) {
        return Err(violation(SchemaKeyword::Pattern, AT));
    }
    Ok(s.into_owned())
}

/// `page_revision`: `r1.` and a canonical digest (the schema pattern).
fn read_page_revision(reader: &mut Reader<'_>) -> Result<PageRevision, ManifestError> {
    const AT: &str = "/page_revision";
    let Item::String(s) = next_item(reader)? else {
        return Err(violation(SchemaKeyword::Type, AT));
    };
    PageRevision::parse(&s).map_err(|_| violation(SchemaKeyword::Pattern, AT))
}

/// `policy` (section 1.4): each member `Some` only when it is in the text.
fn read_policy(reader: &mut Reader<'_>) -> Result<Policy, ManifestError> {
    let Item::Object = next_item(reader)? else {
        return Err(violation(SchemaKeyword::Type, "/policy"));
    };
    let mut policy = Policy::default();
    while let Some(name) = next_member(reader)? {
        match &*name {
            "mode" => {
                policy.mode = Some(match next_item(reader)? {
                    Item::String(s) if s == "private" => Mode::Private,
                    Item::String(s) if s == "public" => Mode::Public,
                    _ => return Err(violation(SchemaKeyword::Enum, "/policy/mode")),
                });
            }
            "ttl_seconds" => {
                const AT: &str = "/policy/ttl_seconds";
                let Item::Integer(n) = next_item(reader)? else {
                    return Err(violation(SchemaKeyword::Type, AT));
                };
                if n > MAX_TTL_SECONDS {
                    return Err(violation(SchemaKeyword::Maximum, AT));
                }
                let ttl = u32::try_from(n).map_err(|_| violation(SchemaKeyword::Maximum, AT))?;
                policy.ttl_seconds = Some(ttl);
            }
            "offline" => {
                let Item::Bool(b) = next_item(reader)? else {
                    return Err(violation(SchemaKeyword::Type, "/policy/offline"));
                };
                policy.offline = Some(b);
            }
            _ => return Err(violation(SchemaKeyword::AdditionalProperties, "/policy")),
        }
    }
    Ok(policy)
}

/// `limits` (section 1.5): `max_patch_bytes` `Some` only when it is in the
/// text.
fn read_limits(reader: &mut Reader<'_>) -> Result<Limits, ManifestError> {
    const AT: &str = "/limits/max_patch_bytes";
    let Item::Object = next_item(reader)? else {
        return Err(violation(SchemaKeyword::Type, "/limits"));
    };
    let mut limits = Limits::default();
    while let Some(name) = next_member(reader)? {
        if &*name != "max_patch_bytes" {
            return Err(violation(SchemaKeyword::AdditionalProperties, "/limits"));
        }
        let Item::Integer(n) = next_item(reader)? else {
            return Err(violation(SchemaKeyword::Type, AT));
        };
        if n < MIN_PATCH_HINT {
            return Err(violation(SchemaKeyword::Minimum, AT));
        }
        if n > MAX_PATCH_HINT {
            return Err(violation(SchemaKeyword::Maximum, AT));
        }
        let hint = u32::try_from(n).map_err(|_| violation(SchemaKeyword::Maximum, AT))?;
        limits.max_patch_bytes = Some(hint);
    }
    Ok(limits)
}

/// The schema pattern of `origin`:
/// `^https?://([a-z0-9._-]+|\[[0-9a-f:.]+\])(:[1-9][0-9]{0,4})?$`.
fn is_origin(s: &str) -> bool {
    let Some(rest) = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
    else {
        return false;
    };
    let port = if let Some(bracketed) = rest.strip_prefix('[') {
        let host_len = bracketed
            .bytes()
            .take_while(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f' | b':' | b'.'))
            .count();
        match bracketed.get(host_len..).and_then(|r| r.strip_prefix(']')) {
            Some(port) if host_len > 0 => port,
            _ => return false,
        }
    } else {
        let host_len = rest
            .bytes()
            .take_while(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
            .count();
        match rest.get(host_len..) {
            Some(port) if host_len > 0 => port,
            _ => return false,
        }
    };
    if port.is_empty() {
        return true;
    }
    let Some(digits) = port.strip_prefix(':') else {
        return false;
    };
    let bytes = digits.as_bytes();
    matches!(bytes.first(), Some(b'1'..=b'9'))
        && bytes.len() <= 5
        && bytes.iter().all(u8::is_ascii_digit)
}

/// The schema pattern of `path`: `^/[!$-;=@-~]*$`.
fn is_path(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.first() == Some(&b'/')
        && bytes
            .iter()
            .all(|b| matches!(b, b'!' | b'$'..=b';' | b'=' | b'@'..=b'~'))
}
