// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Contract between the request fields (lane T-05) and the response selection
//! (lane T-06). Fixed by the architect (WP-03 S1).

/// An entity-tag of `If-None-Match` (RFC 9110 §8.8.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityTag {
    /// `W/` prefix present.
    pub weak: bool,
    /// The characters between the double quotes.
    pub opaque: String,
}

/// The `If-None-Match` field of the request (negotiation §3.5 item 1; DL-089 item 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IfNoneMatch {
    /// No field line.
    Absent,
    /// `*`.
    Any,
    /// A list of one or more entity-tags (all field lines combined).
    Tags(Vec<EntityTag>),
    /// Any other syntax: the condition of step 4 is true (no `304`).
    Invalid,
}

/// `Rq-Have` after negotiation §3.4 server steps 1, 2, 3 and 5 (the steps that do
/// not need the page). Steps 4 and 6 need the current template revision and the
/// number of blocks; lane T-06 applies them.
#[derive(Clone, PartialEq, Eq)]
pub struct Have {
    /// The Token `t` (parameters ignored).
    pub template: String,
    /// The decoded Byte Sequence `b`, or `None` when `b` is absent (missing, not a
    /// Byte Sequence, or a length that is not a multiple of 8).
    pub digests: Option<Vec<u8>>,
}

/// The RQP request fields that the response selection reads (negotiation §3).
// Each flag is an independent fact of one field; no state machine hides here.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, PartialEq, Eq)]
pub struct RqpRequest {
    /// `Rq-Version` is the Integer `1` (§3.2).
    pub version_1: bool,
    /// The capability set (§3.3); empty when the field is absent or invalid.
    pub capabilities: Vec<String>,
    /// `Accept` accepts `application/vnd.rivqen.delta+json` (§3.6; DL-089 item 4).
    pub accepts_delta: bool,
    /// `If-None-Match` for the `304` test (§3.5 item 1).
    pub if_none_match: IfNoneMatch,
    /// The delta base B (§3.5 item 2): the `r1.…` value between the quotes.
    pub base: Option<String>,
    /// `Rq-Have` (§3.4), when present and valid.
    pub have: Option<Have>,
    /// `Rq-Nonce` (§3.9): exactly one field line, a Byte Sequence of 16 bytes.
    pub nonce: Option<[u8; 16]>,
    /// The request has a `Range` field (§4 step 5.1).
    pub has_range: bool,
    /// The request has `If-Match`, `If-Unmodified-Since` or `If-Range` (DL-089 item 1).
    pub other_precondition: bool,
}

// Debug without the nonce and the digests: they must not reach a log (RQP-17).
impl core::fmt::Debug for Have {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Have")
            .field("template", &self.template)
            .field("digests_len", &self.digests.as_ref().map(Vec::len))
            .finish()
    }
}

impl core::fmt::Debug for RqpRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RqpRequest")
            .field("version_1", &self.version_1)
            .field("capabilities", &self.capabilities)
            .field("accepts_delta", &self.accepts_delta)
            .field("if_none_match", &self.if_none_match)
            .field("base", &self.base)
            .field("have", &self.have)
            .field("nonce", &self.nonce.map(|_| "<16 bytes>"))
            .field("has_range", &self.has_range)
            .field("other_precondition", &self.other_precondition)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_nonce_and_digests() {
        let r = RqpRequest {
            version_1: true,
            capabilities: vec!["block-patch".into()],
            accepts_delta: true,
            if_none_match: IfNoneMatch::Absent,
            base: None,
            have: Some(Have {
                template: "t1.x".into(),
                digests: Some(vec![0xAB; 8]),
            }),
            nonce: Some([0xCD; 16]),
            has_range: false,
            other_precondition: false,
        };
        let text = format!("{r:?}");
        assert!(text.contains("digests_len: Some(8)"), "{text}");
        assert!(text.contains("<16 bytes>"), "{text}");
        assert!(!text.contains("171") && !text.contains("205"), "{text}");
    }
}
