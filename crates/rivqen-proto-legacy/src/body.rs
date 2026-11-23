// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The body of a data update (legacy client CL-29; legacy wire section 5): a
//! JSON object with `data` (string values, in the order of the text),
//! `template-tag`, `html-sha1` and `diff`, read once within the limits.
//!
//! Owner: WP-08 S1, T-01.
//!
//! # Rules
//!
//! [`read_data_body`] reads the body of a legacy `200` whose
//! `template-change` is a data update (legacy wire §5.2, §5.4, C-11):
//!
//! 1. The body is at most [`BodyLimits::max_bytes`] bytes. The test runs
//!    before any other read.
//! 2. The body is one JSON text for the strict reader of `rivqen-proto`
//!    (manifest-patch §3.2): UTF-8 without a byte order mark, no duplicate
//!    member name in any object, no lone surrogate escape, numbers only as
//!    digits, nesting at most [`BodyLimits::max_depth`].
//! 3. The top-level value is an object with a member `data` whose value is
//!    an object; each member of `data` has a string value. The members keep
//!    the order of the text: the order of the server data (D-25, CL-27).
//! 4. The member `html-sha1` is a string (CL-29 compares it with the SHA-1
//!    of the rebuilt page; without it the check cannot pass).
//! 5. The members `template-tag` and `diff` are strings when present
//!    (C-11 sends both). The session does not use them: CL-29 compares the
//!    `template-tag` field of the head, and the client computes the diff.
//! 6. Other members are read (rule 2 applies) and ignored.
//!
//! Any break is a [`BodyError`]: the session gives a normal load (CL-29).
//!
//! # Safety
//!
//! The reader does not recurse and is linear in the body, except the
//! duplicate check (`O(n log n)`). The memory is the decoded `data` (at most
//! the body length) and the member names of the open objects. Neither
//! `Debug` nor an error holds a byte of the body.

use core::fmt;

use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_proto::json::{Event, JsonError, Reader};
use rivqen_template::legacy::DataBlock;

/// Member name `data`.
pub const DATA: &str = "data";
/// Member name `html-sha1`.
pub const HTML_SHA1: &str = "html-sha1";
/// Member name `template-tag`.
pub const TEMPLATE_TAG_MEMBER: &str = "template-tag";
/// Member name `diff`.
pub const DIFF: &str = "diff";
/// The nesting depth of a data body of the canonical server (C-11): the
/// top-level object and `data`. A smaller [`BodyLimits::max_depth`] rejects
/// every data body.
pub const DATA_BODY_DEPTH: usize = 2;

/// The limits of one read, given by the caller (the checked engine
/// configuration).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyLimits {
    max_bytes: usize,
    max_depth: usize,
}

impl BodyLimits {
    /// At most `max_bytes` bytes of body and `max_depth` levels of nesting
    /// (the top-level object has depth 1).
    #[must_use]
    pub const fn new(max_bytes: usize, max_depth: usize) -> Self {
        Self {
            max_bytes,
            max_depth,
        }
    }

    /// The largest body in bytes; a body of this size passes.
    #[must_use]
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// The deepest nesting; a body of this depth passes.
    #[must_use]
    pub const fn max_depth(&self) -> usize {
        self.max_depth
    }
}

/// A member of the data body that has a value of another type than a
/// string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyMember {
    /// `html-sha1`.
    HtmlSha1,
    /// `template-tag`.
    TemplateTag,
    /// `diff`.
    Diff,
}

impl BodyMember {
    /// The member name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HtmlSha1 => HTML_SHA1,
            Self::TemplateTag => TEMPLATE_TAG_MEMBER,
            Self::Diff => DIFF,
        }
    }
}

/// Why a body is not a data body (CL-29). No variant holds a byte of the
/// body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyError {
    /// The body is over [`BodyLimits::max_bytes`].
    TooLarge,
    /// The strict reader rejects the text (rule and byte offset only).
    Json(JsonError),
    /// The top-level value is not an object.
    NotObject,
    /// The object has no member `data`.
    NoData,
    /// The value of `data` is not an object.
    DataNotObject,
    /// A member of `data` has a value that is not a string (the function
    /// token `value-not-string` of CL-42).
    ValueNotString,
    /// The object has no member `html-sha1`.
    NoHtmlSha1,
    /// `html-sha1`, `template-tag` or `diff` is not a string.
    NotString(BodyMember),
}

impl BodyError {
    /// A static reason, safe to log.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::TooLarge => "the legacy data body is over the size limit",
            Self::Json(_) => "the legacy data body is not a strict JSON text",
            Self::NotObject => "the legacy data body is not a JSON object",
            Self::NoData => "the legacy data body has no data member",
            Self::DataNotObject => "the data member of the legacy data body is not an object",
            Self::ValueNotString => "a data value of the legacy data body is not a string",
            Self::NoHtmlSha1 => "the legacy data body has no html-sha1 member",
            Self::NotString(_) => "a member of the legacy data body is not a string",
        }
    }
}

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "{}: {error}", self.reason()),
            Self::NotString(member) => write!(f, "{}: {}", self.reason(), member.name()),
            _ => f.write_str(self.reason()),
        }
    }
}

impl core::error::Error for BodyError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl HasErrorCode for BodyError {
    /// `DOCUMENT_TOO_LARGE` for [`BodyError::TooLarge`]; else
    /// `INTEGRITY_ERROR`: the data update fails the check of CL-29.
    fn code(&self) -> ErrorCode {
        match self {
            Self::TooLarge => ErrorCode::DocumentTooLarge,
            _ => ErrorCode::IntegrityError,
        }
    }
}

/// A data body that passed the rules of the module.
///
/// `Debug` writes counts and lengths only: the values are page content.
#[derive(Clone, PartialEq, Eq)]
pub struct DataBody {
    data: Vec<DataBlock>,
    html_sha1: String,
    template_tag: Option<String>,
    diff: Option<String>,
}

impl DataBody {
    /// The members of `data` in the order of the text. The keys are unique.
    #[must_use]
    pub fn data(&self) -> &[DataBlock] {
        &self.data
    }

    /// The members of `data`, moved out.
    #[must_use]
    pub fn into_data(self) -> Vec<DataBlock> {
        self.data
    }

    /// The `html-sha1` member as sent.
    #[must_use]
    pub fn html_sha1(&self) -> &str {
        &self.html_sha1
    }

    /// The `template-tag` member, when present.
    #[must_use]
    pub fn template_tag(&self) -> Option<&str> {
        self.template_tag.as_deref()
    }

    /// The `diff` member, when present.
    #[must_use]
    pub fn diff(&self) -> Option<&str> {
        self.diff.as_deref()
    }
}

impl fmt::Debug for DataBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataBody")
            .field("data_len", &self.data.len())
            .field("html_sha1_len", &self.html_sha1.len())
            .field(
                "template_tag_len",
                &self.template_tag.as_ref().map(String::len),
            )
            .field("diff_len", &self.diff.as_ref().map(String::len))
            .finish()
    }
}

/// The next event. The reader gives `None` only after a complete value, so
/// `None` inside the object cannot occur; it fails closed as
/// [`BodyError::NotObject`].
fn next<'a>(reader: &mut Reader<'a>) -> Result<Event<'a>, BodyError> {
    reader
        .next_event()
        .map_err(BodyError::Json)?
        .ok_or(BodyError::NotObject)
}

/// The string value of a member.
fn string_value(reader: &mut Reader<'_>, member: BodyMember) -> Result<String, BodyError> {
    match next(reader)? {
        Event::String(text) => Ok(text.into_owned()),
        _ => Err(BodyError::NotString(member)),
    }
}

/// The members of the `data` object, in text order.
fn data_value(reader: &mut Reader<'_>) -> Result<Vec<DataBlock>, BodyError> {
    if next(reader)? != Event::StartObject {
        return Err(BodyError::DataNotObject);
    }
    let mut data = Vec::new();
    loop {
        // The reader gives a name or `}` here.
        let Event::Name(key) = next(reader)? else {
            return Ok(data);
        };
        let key = key.into_owned();
        let Event::String(value) = next(reader)? else {
            return Err(BodyError::ValueNotString);
        };
        data.push(DataBlock {
            key,
            value: value.into_owned(),
        });
    }
}

/// Read one value and drop it. The reader bounds the depth.
fn skip_value(reader: &mut Reader<'_>) -> Result<(), BodyError> {
    let mut depth = 0_usize;
    loop {
        match next(reader)? {
            Event::StartObject | Event::StartArray => depth = depth.saturating_add(1),
            Event::EndObject | Event::EndArray => depth = depth.saturating_sub(1),
            // A name comes only inside an object (depth above 0).
            Event::Name(_)
            | Event::String(_)
            | Event::Integer(_)
            | Event::Bool(_)
            | Event::Null => {}
        }
        if depth == 0 {
            return Ok(());
        }
    }
}

/// CL-29 (body form): read a data body (see the module rules).
///
/// # Errors
///
/// [`BodyError`] for the first break of a rule.
pub fn read_data_body(body: &[u8], limits: &BodyLimits) -> Result<DataBody, BodyError> {
    if body.len() > limits.max_bytes {
        return Err(BodyError::TooLarge);
    }
    let mut reader = Reader::new(body, limits.max_depth).map_err(BodyError::Json)?;
    if next(&mut reader)? != Event::StartObject {
        return Err(BodyError::NotObject);
    }
    let mut data = None;
    let mut html_sha1 = None;
    let mut template_tag = None;
    let mut diff = None;
    // The reader gives a name or `}` here.
    while let Event::Name(name) = next(&mut reader)? {
        // The reader rejects a second member of one name (rule 2).
        match name.as_ref() {
            DATA => data = Some(data_value(&mut reader)?),
            HTML_SHA1 => html_sha1 = Some(string_value(&mut reader, BodyMember::HtmlSha1)?),
            TEMPLATE_TAG_MEMBER => {
                template_tag = Some(string_value(&mut reader, BodyMember::TemplateTag)?);
            }
            DIFF => diff = Some(string_value(&mut reader, BodyMember::Diff)?),
            _ => skip_value(&mut reader)?,
        }
    }
    reader.finish().map_err(BodyError::Json)?;
    Ok(DataBody {
        data: data.ok_or(BodyError::NoData)?,
        html_sha1: html_sha1.ok_or(BodyError::NoHtmlSha1)?,
        template_tag,
        diff,
    })
}
