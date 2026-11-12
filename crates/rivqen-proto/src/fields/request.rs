// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The RQP request fields (negotiation sections 3.2 to 3.5, 3.9 and 6.2).

use core::fmt;

use rivqen_model::error::{ErrorCode, HasErrorCode};
use rivqen_model::http::FieldLine;

use super::{
    ACCEPT, CAPABILITY_BLOCK_PATCH, DELTA_MEDIA_TYPE, HTML_MEDIA_TYPE, IF_NONE_MATCH,
    RQ_CAPABILITIES, RQ_HAVE, RQ_NONCE, RQ_VERSION, RQP_VERSION,
};
use crate::sf::{
    BareItem, Dictionary, InnerList, Item, Member, SerializeError, serialize_dictionary,
    serialize_item, serialize_list,
};
use crate::types::{MAX_BLOCKS, MAX_RQ_HAVE_BYTES, Nonce, SnapshotMeta};

/// What the client has when it builds the fields of one network attempt.
///
/// The caller has already decided that the request carries RQP fields
/// (negotiation section 6.1) and that the snapshot is usable (for example not
/// in a `public` partition that the app has turned off, H-81).
#[derive(Debug, Clone, Copy)]
pub enum RequestBasis<'a> {
    /// No usable snapshot. Fields: `Rq-Version`, `Rq-Capabilities` and
    /// `Accept: text/html` (negotiation section 6.2, "Without a snapshot").
    FirstVisit,
    /// The one fallback load of negotiation section 6.3, item 5: the same
    /// fields as [`Self::FirstVisit`], so no `Rq-Have`, no `Rq-Nonce`, no RQP
    /// `If-None-Match` and no delta media type.
    FallbackLoad,
    /// A usable snapshot S.
    ///
    /// With a nonce: a delta request (section 6.2) with `If-None-Match: "R"`,
    /// `Rq-Have` and `Rq-Nonce`, and the delta media type in `Accept`.
    ///
    /// Without a nonce (the CSPRNG failed, section 3.9): no delta offer, only
    /// `If-None-Match: "R"`, so a `304` stays possible (fail closed).
    Snapshot {
        /// The snapshot S.
        snapshot: &'a SnapshotMeta,
        /// The new nonce N of this network attempt, from a CSPRNG (RQP-29).
        nonce: Option<&'a Nonce>,
    },
}

/// Why the request fields cannot be built. Holds no field content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RequestFieldsError {
    /// The snapshot has more than [`MAX_BLOCKS`] blocks: it breaks the
    /// invariant of [`SnapshotMeta`] (the markup analysis rejects such a page),
    /// and its `Rq-Have` could exceed [`MAX_RQ_HAVE_BYTES`] (negotiation
    /// section 3.4). Only a delta request (with a nonce) sends `Rq-Have`, so
    /// only it checks the count.
    TooManyBlocks,
    /// The `Rq-Have` value would be longer than [`MAX_RQ_HAVE_BYTES`]. A
    /// snapshot with at most [`MAX_BLOCKS`] blocks gives at most 2786 bytes, so
    /// this means a broken invariant.
    RqHaveTooLong,
    /// A value has no Structured Field serialization (a broken invariant: the
    /// identifiers of `rivqen-integrity` are always valid Tokens).
    Serialize(SerializeError),
}

impl fmt::Display for RequestFieldsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyBlocks => f.write_str("request fields: the snapshot has too many blocks"),
            Self::RqHaveTooLong => f.write_str("request fields: Rq-Have is longer than 4096 bytes"),
            Self::Serialize(e) => write!(f, "request fields: {e}"),
        }
    }
}

impl core::error::Error for RequestFieldsError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Serialize(e) => Some(e),
            Self::TooManyBlocks | Self::RqHaveTooLong => None,
        }
    }
}

impl From<SerializeError> for RequestFieldsError {
    fn from(e: SerializeError) -> Self {
        Self::Serialize(e)
    }
}

impl HasErrorCode for RequestFieldsError {
    /// Every case is a snapshot that breaks its invariant: `INTERNAL_ERROR`
    /// (errors.md section 2), and the adapter loads the page normally.
    fn code(&self) -> ErrorCode {
        ErrorCode::InternalError
    }
}

/// The RQP request fields of one network attempt (negotiation section 6.2).
///
/// # Errors
///
/// [`RequestFieldsError`] when the snapshot breaks its invariant. The fields
/// are then not sent; the caller loads the page normally.
pub fn request_fields(basis: RequestBasis<'_>) -> Result<Vec<FieldLine>, RequestFieldsError> {
    let mut lines = vec![
        FieldLine::new(RQ_VERSION, rq_version()?),
        FieldLine::new(RQ_CAPABILITIES, rq_capabilities()?),
    ];
    let RequestBasis::Snapshot { snapshot, nonce } = basis else {
        lines.push(FieldLine::new(ACCEPT, HTML_MEDIA_TYPE));
        return Ok(lines);
    };
    let if_none_match = format!("\"{}\"", snapshot.page_revision);
    let Some(nonce) = nonce else {
        lines.push(FieldLine::new(ACCEPT, HTML_MEDIA_TYPE));
        lines.push(FieldLine::new(IF_NONE_MATCH, if_none_match));
        return Ok(lines);
    };
    let have = rq_have(snapshot)?;
    lines.push(FieldLine::new(
        ACCEPT,
        format!("{HTML_MEDIA_TYPE}, {DELTA_MEDIA_TYPE}"),
    ));
    lines.push(FieldLine::new(IF_NONE_MATCH, if_none_match));
    lines.push(FieldLine::new(RQ_HAVE, have));
    lines.push(FieldLine::new(RQ_NONCE, rq_nonce(nonce)?));
    Ok(lines)
}

/// `Rq-Version: 1` (section 3.2).
fn rq_version() -> Result<String, SerializeError> {
    serialize_item(&Item::new(BareItem::Integer(RQP_VERSION)))
}

/// `Rq-Capabilities: (block-patch)`: one Inner List of Tokens (section 3.3).
fn rq_capabilities() -> Result<String, SerializeError> {
    serialize_list(&vec![Member::InnerList(InnerList {
        items: vec![Item::new(BareItem::Token(
            CAPABILITY_BLOCK_PATCH.to_owned(),
        ))],
        params: crate::sf::Parameters::new(),
    })])
}

/// `Rq-Have: t=T, b=:prefixes:` (section 3.4): the template revision as a
/// Token and the first 8 bytes of each block digest, in document order, as a
/// Byte Sequence. Checks the block count before it allocates, and the length
/// after.
fn rq_have(snapshot: &SnapshotMeta) -> Result<String, RequestFieldsError> {
    if snapshot.blocks.len() > MAX_BLOCKS {
        return Err(RequestFieldsError::TooManyBlocks);
    }
    let prefixes: Vec<u8> = snapshot
        .blocks
        .iter()
        .flat_map(|b| b.sha256.prefix())
        .collect();
    let dictionary: Dictionary = [
        (
            "t".to_owned(),
            Member::Item(Item::new(BareItem::Token(
                snapshot.template_revision.to_string(),
            ))),
        ),
        (
            "b".to_owned(),
            Member::Item(Item::new(BareItem::ByteSequence(prefixes))),
        ),
    ]
    .into_iter()
    .collect();
    let value = serialize_dictionary(&dictionary)?;
    if value.len() > MAX_RQ_HAVE_BYTES {
        return Err(RequestFieldsError::RqHaveTooLong);
    }
    Ok(value)
}

/// `Rq-Nonce: :<base64>:` (section 3.9): a Byte Sequence of the 16 nonce
/// bytes with padding and no parameter, 26 bytes.
fn rq_nonce(nonce: &Nonce) -> Result<String, SerializeError> {
    serialize_item(&Item::new(BareItem::ByteSequence(nonce.0.to_vec())))
}
