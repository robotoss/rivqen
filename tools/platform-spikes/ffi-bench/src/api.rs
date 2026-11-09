// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! UniFFI path of CONTRACT.md section 10: `open_stream` and `DocStream::next_chunk`.
//!
//! No unsafe code here (the crate denies it outside `abi`; the UniFFI macros pass that lint).

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};

use crate::doc::{Cursor, Stream};

/// One document stream for the UniFFI path. The foreign side owns it through an `Arc`.
#[derive(Debug, uniffi::Object)]
pub struct DocStream {
    /// `None` when the open failed: such a stream gives no chunk.
    inner: Mutex<Option<Stream>>,
}

/// Opens the document of `size` bytes in chunks of `chunk` bytes.
///
/// The contract signature has no error. A bad `size` or `chunk` (see [`crate::doc::Cursor::new`])
/// gives a stream that is already at its end; the SHA-256 check of the harness then fails.
#[uniffi::export]
#[must_use]
pub fn open_stream(size: u64, chunk: u32) -> Arc<DocStream> {
    Arc::new(DocStream {
        inner: Mutex::new(Cursor::new(size, chunk).ok().map(Stream::new)),
    })
}

/// SHA-256 of the document of `size` bytes; `None` when `size` is above the limit.
#[uniffi::export]
#[must_use]
pub fn doc_sha256(size: u64) -> Option<Vec<u8>> {
    crate::doc_sha256(size).map(|d| d.to_vec())
}

#[uniffi::export]
impl DocStream {
    /// The next chunk, copied into a new `Vec` (UniFFI then copies it again into a
    /// `RustBuffer`); `None` at the end.
    #[must_use]
    pub fn next_chunk(&self) -> Option<Vec<u8>> {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let stream = guard.as_mut()?;
        let mut out = None;
        // The cap is the chunk size, so only `chunk` limits the slice.
        let cap = NonZeroUsize::MAX;
        let read: Result<Option<usize>, ()> = stream.read_with(cap, |slice| {
            out = Some(slice.to_vec());
            Ok(())
        });
        match read {
            Ok(Some(_)) => out,
            Ok(None) | Err(()) => None,
        }
    }

    /// Bytes that the Rust code of this stream copied (the `Vec` of each chunk).
    #[must_use]
    pub fn copied_bytes(&self) -> u64 {
        let guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        guard.as_ref().map_or(0, Stream::copied)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::doc::byte_at;

    #[test]
    fn uniffi_stream_gives_the_document() {
        let s = open_stream(40_000, 16_384);
        let mut doc = Vec::new();
        let mut lens = Vec::new();
        while let Some(c) = s.next_chunk() {
            lens.push(c.len());
            doc.extend_from_slice(&c);
        }
        assert_eq!(lens, vec![16_384, 16_384, 7_232]);
        assert_eq!(doc, (0..40_000_u64).map(byte_at).collect::<Vec<_>>());
        assert_eq!(s.copied_bytes(), 40_000);
        assert_eq!(s.next_chunk(), None);
        assert_eq!(s.copied_bytes(), 40_000);
    }

    #[test]
    fn uniffi_bad_open_is_an_empty_stream() {
        for (size, chunk) in [(10, 0), (10, 65_537), (u64::MAX, 1)] {
            let s = open_stream(size, chunk);
            assert_eq!(s.next_chunk(), None);
            assert_eq!(s.copied_bytes(), 0);
        }
    }

    #[test]
    fn uniffi_sha() {
        assert_eq!(doc_sha256(0).unwrap(), crate::sha256::digest(b"").to_vec());
        assert_eq!(doc_sha256(u64::MAX), None);
    }
}
