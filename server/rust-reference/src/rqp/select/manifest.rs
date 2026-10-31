// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The page revision with the policy line (manifest and patch §1.3, H-55), the
//! manifest JSON text M (§1.2, §1.7, §3.2), the delta binding (§2.2, H-107,
//! H-108) and the patch envelope (§2.2, §2.3, H-104).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::rqp::config::{Mode, RqpConfig};

/// The start tag of the manifest element (§1.7 server item 1).
pub const MANIFEST_START: &[u8] = b"<script type=\"application/rivqen-manifest+json\">";

/// The end tag of the manifest element.
pub const MANIFEST_END: &[u8] = b"</script>";

/// One block line of the page revision preimage: id, format, full hash.
#[derive(Debug, Clone, Copy)]
pub struct BlockLine<'a> {
    /// The block id.
    pub id: &'a str,
    /// `html` or `json`.
    pub format: &'a str,
    /// `B64U(SHA-256(content))`.
    pub sha256: &'a str,
}

/// `B64U(SHA-256(parts…))`.
fn b64u_sha256(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

/// `page_revision` (§1.3 steps 4 and 5) of the template revision, the policy
/// line text S (`None`: no policy line) and the block lines in order.
#[must_use]
pub fn page_revision(
    template_revision: &str,
    policy_line: Option<&str>,
    blocks: &[BlockLine<'_>],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"rqp-r1\n");
    hasher.update(template_revision.as_bytes());
    hasher.update(b"\n");
    if let Some(s) = policy_line {
        hasher.update(b"PL\t");
        hasher.update(s.as_bytes());
        hasher.update(b"\n");
    }
    for block in blocks {
        hasher.update(block.id.as_bytes());
        hasher.update(b"\t");
        hasher.update(block.format.as_bytes());
        hasher.update(b"\t");
        hasher.update(block.sha256.as_bytes());
        hasher.update(b"\n");
    }
    format!("r1.{}", URL_SAFE_NO_PAD.encode(hasher.finalize()))
}

/// The binding of §2.2: `"d1." ‖ B64U(SHA-256("rqp-d1" LF U LF B LF N LF R′ LF))`,
/// N = the base64url text of the nonce without padding.
#[must_use]
pub fn binding(url: &str, base: &str, nonce: &[u8; 16], next: &str) -> String {
    let n = URL_SAFE_NO_PAD.encode(nonce);
    let hash = b64u_sha256(&[
        b"rqp-d1\n",
        url.as_bytes(),
        b"\n",
        base.as_bytes(),
        b"\n",
        n.as_bytes(),
        b"\n",
        next.as_bytes(),
        b"\n",
    ]);
    format!("d1.{hash}")
}

/// A sink that counts the bytes of a JSON text and, when it has a buffer,
/// keeps them. The size check of step 7 runs the same writer without a buffer
/// first, so a delta that is too large is never built.
pub struct Out {
    buf: Option<Vec<u8>>,
    len: usize,
}

impl Out {
    /// A sink that only counts.
    #[must_use]
    pub fn counter() -> Self {
        Self { buf: None, len: 0 }
    }

    /// A sink that keeps the bytes; `capacity` is the expected size.
    #[must_use]
    pub fn buffer(capacity: usize) -> Self {
        Self {
            buf: Some(Vec::with_capacity(capacity)),
            len: 0,
        }
    }

    /// The number of bytes written.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when nothing was written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The bytes (empty for a counter).
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf.unwrap_or_default()
    }

    fn push(&mut self, bytes: &[u8]) {
        self.len = self.len.saturating_add(bytes.len());
        if let Some(buf) = &mut self.buf {
            buf.extend_from_slice(bytes);
        }
    }

    /// A JSON string of the UTF-8 bytes `text` (RFC 8259 §7): `"`, `\` and the
    /// control characters are escaped; every other byte is copied.
    fn string(&mut self, text: &[u8]) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        self.push(b"\"");
        let mut from = 0;
        for (i, &b) in text.iter().enumerate() {
            let short: &[u8] = match b {
                b'"' => b"\\\"",
                b'\\' => b"\\\\",
                b'\n' => b"\\n",
                b'\r' => b"\\r",
                b'\t' => b"\\t",
                0x08 => b"\\b",
                0x0C => b"\\f",
                0x00..=0x1F => b"",
                _ => continue,
            };
            self.push(text.get(from..i).unwrap_or_default());
            if short.is_empty() {
                let hi = HEX.get(usize::from(b >> 4)).copied().unwrap_or(b'0');
                let lo = HEX.get(usize::from(b & 0x0F)).copied().unwrap_or(b'0');
                self.push(&[b'\\', b'u', b'0', b'0', hi, lo]);
            } else {
                self.push(short);
            }
            from = i.saturating_add(1);
        }
        self.push(text.get(from..).unwrap_or_default());
        self.push(b"\"");
    }
}

/// The manifest JSON text M (§1.2; H-103: no member with its default value).
/// Member order is fixed, so the same value gives the same bytes (§1.7 server
/// item 6).
#[must_use]
pub fn manifest_text(config: &RqpConfig, origin: &str, path: &str, page_revision: &str) -> Vec<u8> {
    let mut out = Out::buffer(256);
    out.push(b"{\"protocol\":\"rqp/1\",\"origin\":");
    out.string(origin.as_bytes());
    out.push(b",\"path\":");
    out.string(path.as_bytes());
    out.push(b",\"page_revision\":");
    out.string(page_revision.as_bytes());
    if config.writes_policy() {
        let mut members: Vec<String> = Vec::new();
        if config.mode() == Mode::Public {
            members.push("\"mode\":\"public\"".to_owned());
        }
        if config.ttl_seconds() != 0 {
            members.push(format!("\"ttl_seconds\":{}", config.ttl_seconds()));
        }
        if config.offline() {
            members.push("\"offline\":true".to_owned());
        }
        out.push(b",\"policy\":{");
        out.push(members.join(",").as_bytes());
        out.push(b"}");
    }
    if let Some(hint) = config.manifest_hint() {
        out.push(format!(",\"limits\":{{\"max_patch_bytes\":{hint}}}").as_bytes());
    }
    out.push(b"}");
    out.into_bytes()
}

/// The patch envelope (§2.2, §2.3): `binding` and one operation per changed
/// block, `(id, content)` in document order.
pub fn envelope(out: &mut Out, binding: &str, operations: &[(&str, &[u8])]) {
    out.push(b"{\"binding\":");
    out.string(binding.as_bytes());
    out.push(b",\"operations\":[");
    for (i, (id, content)) in operations.iter().enumerate() {
        if i > 0 {
            out.push(b",");
        }
        out.push(b"{\"block_id\":");
        out.string(id.as_bytes());
        out.push(b",\"content\":");
        out.string(content);
        out.push(b"}");
    }
    out.push(b"]}");
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;

    #[test]
    fn json_string_escapes() {
        let mut out = Out::buffer(0);
        out.string(b"a\"b\\c\n\r\t\x08\x0C\x01\x1F\x7F<\xC3\xA9");
        let text = out.into_bytes();
        assert_eq!(
            text,
            b"\"a\\\"b\\\\c\\n\\r\\t\\b\\f\\u0001\\u001f\x7F<\xC3\xA9\""
        );
        let parsed: String = serde_json::from_slice(&text).unwrap();
        assert_eq!(
            parsed.as_bytes(),
            b"a\"b\\c\n\r\t\x08\x0C\x01\x1F\x7F<\xC3\xA9"
        );
    }

    #[test]
    fn counter_and_buffer_agree() {
        let ops: [(&str, &[u8]); 2] = [("a", b"x\"y"), ("b", b"\x00z")];
        let mut count = Out::counter();
        envelope(&mut count, "d1.x", &ops);
        let mut buf = Out::buffer(0);
        envelope(&mut buf, "d1.x", &ops);
        assert_eq!(count.len(), buf.len());
        let bytes = buf.into_bytes();
        assert_eq!(bytes.len(), count.len());
        assert_eq!(count.into_bytes(), b"");
        assert_eq!(
            bytes,
            br#"{"binding":"d1.x","operations":[{"block_id":"a","content":"x\"y"},{"block_id":"b","content":"\u0000z"}]}"#
        );
    }
}
