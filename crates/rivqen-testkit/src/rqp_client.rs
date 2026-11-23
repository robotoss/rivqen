// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Helpers that read an RQP client fixture of the ledger into protocol
//! values (WP-08 S1, T-06): one copy for the tests of `rivqen-proto`,
//! `rivqen-template`, `rivqen-diff` and the session.
//!
//! [`fixtures`](crate::fixtures) keeps the values of a fixture as text; this
//! module converts them to the core types ([`snapshot_meta`], [`members`],
//! [`descriptor`], [`nonce`]), loads the protocol fixtures and the ledger
//! once per test binary ([`protocol`], [`ledger`], [`client_fixture`]), and
//! gives the URL forms of a fixture through `rivqen-url` ([`canonical_url`],
//! [`origin_and_path`]).
//!
//! Every function returns a [`ClientFixtureError`]; nothing panics. A test
//! that wants a panic with the message calls `unwrap_or_else`. A test that
//! uses this module is an integration test under `tests/`: the testkit
//! depends on `rivqen-proto`, so a unit test of that crate would see two
//! copies of its types.

use std::fmt;
use std::sync::OnceLock;

use rivqen_integrity::{PageRevision, Sha256Digest, TemplateRevision, b64u_encode};
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, Mode, NONCE_BYTES, Nonce, Policy, SnapshotMeta,
};
use rivqen_url::UrlError;

use crate::fixtures::{BlockData, Fixture, Members, RqpProtocol, SnapshotData, load_rqp_protocol};
use crate::ledger::Ledger;

/// A value of a client fixture cannot become a core value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClientFixtureError {
    /// The fixtures or the ledger do not load, or the ledger and the
    /// fixtures differ. The text is the message of the loader.
    Load(String),
    /// No client fixture has this id.
    NoFixture(String),
    /// A value has a form that the core type does not accept.
    Value {
        /// The member that holds the value.
        member: &'static str,
        /// What is wrong, without the value of a secret.
        what: String,
    },
    /// The URL of a fixture is not an absolute `http` or `https` URL that
    /// `rivqen-url` accepts.
    Url(UrlError),
}

impl fmt::Display for ClientFixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(message) => f.write_str(message),
            Self::NoFixture(id) => write!(f, "no client fixture {id}"),
            Self::Value { member, what } => write!(f, "{member}: {what}"),
            Self::Url(error) => write!(f, "fixture URL: {error}"),
        }
    }
}

impl std::error::Error for ClientFixtureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Url(error) => Some(error),
            _ => None,
        }
    }
}

fn value(member: &'static str, what: impl fmt::Display) -> ClientFixtureError {
    ClientFixtureError::Value {
        member,
        what: what.to_string(),
    }
}

/// All fixtures of `fixtures/rqp/protocol`, loaded once per test binary.
///
/// # Errors
/// [`ClientFixtureError::Load`] when the fixtures do not load.
pub fn protocol() -> Result<&'static RqpProtocol, ClientFixtureError> {
    static PROTOCOL: OnceLock<Result<RqpProtocol, ClientFixtureError>> = OnceLock::new();
    PROTOCOL
        .get_or_init(|| {
            load_rqp_protocol().map_err(|error| ClientFixtureError::Load(error.to_string()))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// The ledger of the client fixtures; it lists exactly the client fixtures.
///
/// # Errors
/// [`ClientFixtureError::Load`] when the ledger does not load or does not
/// match the fixtures.
pub fn ledger() -> Result<&'static Ledger, ClientFixtureError> {
    static LEDGER: OnceLock<Result<Ledger, ClientFixtureError>> = OnceLock::new();
    LEDGER
        .get_or_init(|| {
            let protocol = protocol()?;
            let ledger =
                Ledger::embedded().map_err(|error| ClientFixtureError::Load(error.to_string()))?;
            ledger
                .verify(protocol.client.iter().map(|f| f.id.as_str()))
                .map_err(|error| ClientFixtureError::Load(error.to_string()))?;
            Ok(ledger)
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// The client fixture with this id.
///
/// # Errors
/// [`ClientFixtureError::NoFixture`] when there is none;
/// [`ClientFixtureError::Load`] when the fixtures do not load.
pub fn client_fixture(id: &str) -> Result<&'static Fixture, ClientFixtureError> {
    protocol()?
        .client
        .iter()
        .find(|f| f.id == id)
        .ok_or_else(|| ClientFixtureError::NoFixture(id.to_owned()))
}

/// `policy` and `limits` with member presence kept.
///
/// # Errors
/// [`ClientFixtureError::Value`] for a `mode` that is not `private` or
/// `public`, or a number that does not fit `u32`.
pub fn members(members: &Members) -> Result<(Option<Policy>, Option<Limits>), ClientFixtureError> {
    let policy = members
        .policy
        .as_ref()
        .map(|p| {
            let mode = match p.mode.as_deref() {
                None => None,
                Some("private") => Some(Mode::Private),
                Some("public") => Some(Mode::Public),
                Some(other) => return Err(value("policy.mode", format!("{other:?}"))),
            };
            let ttl_seconds = p
                .ttl_seconds
                .map(u32::try_from)
                .transpose()
                .map_err(|e| value("policy.ttl_seconds", e))?;
            Ok(Policy {
                mode,
                ttl_seconds,
                offline: p.offline,
            })
        })
        .transpose()?;
    let limits = members
        .limits
        .as_ref()
        .map(|l| {
            let max_patch_bytes = l
                .max_patch_bytes
                .map(u32::try_from)
                .transpose()
                .map_err(|e| value("limits.max_patch_bytes", e))?;
            Ok(Limits { max_patch_bytes })
        })
        .transpose()?;
    Ok((policy, limits))
}

/// One block of a fixture snapshot.
///
/// # Errors
/// [`ClientFixtureError::Value`] for a block id, a format or a hash that the
/// core type does not accept.
pub fn descriptor(block: &BlockData) -> Result<BlockDescriptor, ClientFixtureError> {
    Ok(BlockDescriptor {
        id: BlockId::parse(&block.id).map_err(|e| value("block id", e))?,
        format: match block.format.as_str() {
            "html" => BlockFormat::Html,
            "json" => BlockFormat::Json,
            other => return Err(value("block format", format!("{other:?}"))),
        },
        sha256: Sha256Digest::from_b64u(&block.sha256).map_err(|e| value("block sha256", e))?,
    })
}

/// The values that the client keeps for a fixture snapshot.
///
/// # Errors
/// [`ClientFixtureError::Value`] for a value that the core type does not
/// accept.
pub fn snapshot_meta(data: &SnapshotData) -> Result<SnapshotMeta, ClientFixtureError> {
    let (policy, limits) = members(&data.members)?;
    Ok(SnapshotMeta {
        template_revision: TemplateRevision::parse(&data.template_revision)
            .map_err(|e| value("template_revision", e))?,
        page_revision: PageRevision::parse(&data.page_revision)
            .map_err(|e| value("page_revision", e))?,
        blocks: data
            .blocks
            .iter()
            .map(descriptor)
            .collect::<Result<_, _>>()?,
        policy,
        limits,
    })
}

/// The bytes of base64url text without padding (an independent decoder for
/// the tests: it does not use the encoder or the decoder of the crate under
/// test).
///
/// # Errors
/// [`ClientFixtureError::Value`] for a character outside the alphabet.
pub fn b64u_decode(text: &str) -> Result<Vec<u8>, ClientFixtureError> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0_u32, 0_u32);
    for &c in text.as_bytes() {
        let six = match c {
            b'A'..=b'Z' => c.wrapping_sub(b'A'),
            b'a'..=b'z' => c.wrapping_sub(b'a').wrapping_add(26),
            b'0'..=b'9' => c.wrapping_sub(b'0').wrapping_add(52),
            b'-' => 62,
            b'_' => 63,
            _ => return Err(value("base64url", "a character outside the alphabet")),
        };
        acc = acc.wrapping_shl(6) | u32::from(six);
        bits = bits.wrapping_add(6);
        if bits >= 8 {
            bits = bits.wrapping_sub(8);
            let byte =
                u8::try_from(acc.wrapping_shr(bits) & 0xFF).map_err(|e| value("base64url", e))?;
            out.push(byte);
            acc &= 1_u32.wrapping_shl(bits).wrapping_sub(1);
        }
    }
    Ok(out)
}

/// The nonce of the text N (22 base64url characters, no padding).
///
/// # Errors
/// [`ClientFixtureError::Value`] when the text is not base64url, does not
/// give [`NONCE_BYTES`] bytes, or the encoder of the crate does not write N
/// again.
pub fn nonce(text: &str) -> Result<Nonce, ClientFixtureError> {
    let bytes = b64u_decode(text)?;
    let array: [u8; NONCE_BYTES] = bytes
        .try_into()
        .map_err(|_| value("nonce", format!("not {NONCE_BYTES} bytes")))?;
    let nonce = Nonce(array);
    // The decoder is right when the encoder of the crate writes N again.
    if b64u_encode(&nonce.0) != text {
        return Err(value("nonce", "not canonical base64url"));
    }
    Ok(nonce)
}

/// U of the binding for a fixture final URL (manifest and patch §2.2),
/// from [`rivqen_url::parse`].
///
/// # Errors
/// [`ClientFixtureError::Url`] when `rivqen-url` rejects the text.
pub fn canonical_url(url: &str) -> Result<String, ClientFixtureError> {
    let parsed = rivqen_url::parse(url).map_err(ClientFixtureError::Url)?;
    Ok(parsed.binding_url().to_owned())
}

/// The origin and the path of a fixture URL, from [`rivqen_url::parse`] (the
/// pair that `read_manifest` compares, manifest and patch §1.6).
///
/// # Errors
/// [`ClientFixtureError::Url`] when `rivqen-url` rejects the text.
pub fn origin_and_path(url: &str) -> Result<(String, String), ClientFixtureError> {
    let parsed = rivqen_url::parse(url).map_err(ClientFixtureError::Url)?;
    Ok((parsed.origin().to_owned(), parsed.path().to_owned()))
}
