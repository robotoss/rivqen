// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The hand-written C ABI of `include/rq_bench.h` (CONTRACT.md section 10).
//!
//! `RqBenchStream *` is an opaque handle: its address is a key of the handle table
//! ([`crate::registry`]), made with `ptr::without_provenance_mut`, and it is never
//! dereferenced. A null, closed or forged handle gives [`E_HANDLE`]; `rq_bench_close` twice
//! gives [`E_HANDLE`] the second time.

use std::convert::Infallible;
use std::num::NonZeroUsize;
use std::ptr;

use super::{E_ARG, E_NULL, E_PANIC, END, OK, code, guard};
use crate::registry;

/// Opaque stream type of the C header. Rust never creates or reads a value of it.
#[repr(C)]
pub struct RqBenchStream {
    _private: [u8; 0],
}

/// Result of one read into a raw caller buffer: a status code and the byte count.
///
/// # Safety
///
/// `buf` is null or valid for writes of `cap` bytes for the whole call.
pub(crate) unsafe fn read_raw(handle: usize, buf: *mut u8, cap: usize) -> (i32, usize) {
    if buf.is_null() {
        return (E_NULL, 0);
    }
    let Some(cap) = NonZeroUsize::new(cap) else {
        return (E_ARG, 0);
    };
    let read = registry::global().with(handle, |stream| {
        stream.read_with(cap, |slice| {
            // SAFETY: `slice.len() <= cap` (contract of `Stream::read_with`); the caller of
            // `read_raw` guarantees that `buf` is valid for writes of `cap` bytes. `slice` is
            // in an immutable static table and `buf` is writable memory, so they do not overlap.
            unsafe { ptr::copy_nonoverlapping(slice.as_ptr(), buf, slice.len()) };
            Ok::<(), Infallible>(())
        })
    });
    match read {
        Err(e) => (code(e), 0),
        Ok(Ok(Some(n))) => (OK, n),
        Ok(Ok(None)) => (END, 0),
        Ok(Err(never)) => match never {},
    }
}

/// Opens the document of `size` bytes in chunks of `chunk` bytes (`1 ..= 65536`).
/// Returns null for a bad argument or when 1024 streams are open.
#[unsafe(no_mangle)]
pub extern "C" fn rq_bench_open(size: u64, chunk: u32) -> *mut RqBenchStream {
    guard(ptr::null_mut(), || {
        match registry::global().open(size, chunk) {
            Ok(handle) => ptr::without_provenance_mut(handle.get()),
            Err(_) => ptr::null_mut(),
        }
    })
}

/// Copies the next chunk (at most `min(chunk, cap)` bytes) into `buf` and writes its length
/// to `*out_len`. Returns [`OK`], [`END`] (with `*out_len == 0`) or a negative error.
///
/// # Safety
///
/// `buf` is null or valid for writes of `cap` bytes. `out_len` is null or valid for one
/// aligned `size_t` write. `buf` and `out_len` do not overlap. No other thread reads or
/// writes this memory during the call. `s` may be any value. The same text is in
/// `include/rq_bench.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rq_bench_read(
    s: *mut RqBenchStream,
    buf: *mut u8,
    cap: usize,
    out_len: *mut usize,
) -> i32 {
    guard(E_PANIC, || {
        if out_len.is_null() {
            return E_NULL;
        }
        // SAFETY: the caller of `rq_bench_read` guarantees that `buf` is null or valid for
        // writes of `cap` bytes.
        let (status, n) = unsafe { read_raw(s.addr(), buf, cap) };
        // SAFETY: `out_len` is not null, and the caller guarantees that it is valid for one
        // aligned `size_t` write.
        unsafe { out_len.write(n) };
        status
    })
}

/// Closes the stream. Returns [`OK`], or [`E_HANDLE`] for a null, unknown or closed handle
/// (so a second close is a defined error).
#[unsafe(no_mangle)]
pub extern "C" fn rq_bench_close(s: *mut RqBenchStream) -> i32 {
    guard(E_PANIC, || match registry::global().close(s.addr()) {
        Ok(()) => OK,
        Err(e) => code(e),
    })
}

/// Writes the count of bytes that the Rust side copied out of the stream to `*out`.
///
/// # Safety
///
/// `out` is null or valid for one aligned `uint64_t` write. No other thread reads or writes
/// this memory during the call. `s` may be any value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rq_bench_copied(s: *mut RqBenchStream, out: *mut u64) -> i32 {
    guard(E_PANIC, || {
        if out.is_null() {
            return E_NULL;
        }
        match registry::global().with(s.addr(), |stream| stream.copied()) {
            Ok(n) => {
                // SAFETY: `out` is not null and the caller guarantees one aligned u64 write.
                unsafe { out.write(n) };
                OK
            }
            Err(e) => code(e),
        }
    })
}

/// Writes the SHA-256 of the document of `size` bytes (32 bytes) to `out`.
/// Returns [`E_ARG`] when `cap < 32` or `size` is above the limit.
///
/// # Safety
///
/// `out` is null or valid for writes of `cap` bytes. No other thread reads or writes this
/// memory during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rq_bench_sha256(size: u64, out: *mut u8, cap: usize) -> i32 {
    guard(E_PANIC, || {
        if out.is_null() {
            return E_NULL;
        }
        if cap < 32 {
            return E_ARG;
        }
        let Some(digest) = crate::doc_sha256(size) else {
            return E_ARG;
        };
        // SAFETY: `out` is not null and valid for `cap >= 32` bytes (caller); `digest` is a
        // local array, so the ranges do not overlap.
        unsafe { ptr::copy_nonoverlapping(digest.as_ptr(), out, digest.len()) };
        OK
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::super::E_HANDLE;
    use super::*;
    use crate::doc::byte_at;

    /// `rq_bench_read` with `cap <= buf.len()`; `None` passes null.
    fn read_with(
        s: *mut RqBenchStream,
        buf: Option<&mut [u8]>,
        cap: usize,
        out_len: Option<&mut usize>,
    ) -> i32 {
        let buf = buf.map_or(ptr::null_mut(), |b| {
            assert!(cap <= b.len());
            b.as_mut_ptr()
        });
        let out_len = out_len.map_or(ptr::null_mut(), ptr::from_mut);
        // SAFETY: `buf` is null or a live local buffer of at least `cap` bytes (asserted);
        // `out_len` is null or a live, aligned local `usize`.
        unsafe { rq_bench_read(s, buf, cap, out_len) }
    }

    fn read(s: *mut RqBenchStream, buf: &mut [u8]) -> (i32, usize) {
        let mut n = usize::MAX;
        let cap = buf.len();
        let status = read_with(s, Some(buf), cap, Some(&mut n));
        (status, n)
    }

    /// `rq_bench_copied`; `None` passes null.
    fn copied(s: *mut RqBenchStream, out: Option<&mut u64>) -> i32 {
        let out = out.map_or(ptr::null_mut(), ptr::from_mut);
        // SAFETY: `out` is null or a live, aligned local `u64`.
        unsafe { rq_bench_copied(s, out) }
    }

    /// `rq_bench_sha256` with `cap <= out.len()`; `None` passes null.
    fn sha(size: u64, out: Option<&mut [u8]>, cap: usize) -> i32 {
        let out = out.map_or(ptr::null_mut(), |b| {
            assert!(cap <= b.len());
            b.as_mut_ptr()
        });
        // SAFETY: `out` is null or a live local buffer of at least `cap` bytes (asserted).
        unsafe { rq_bench_sha256(size, out, cap) }
    }

    fn read_all(size: u64, chunk: u32) -> (Vec<u8>, Vec<usize>) {
        let s = rq_bench_open(size, chunk);
        assert!(!s.is_null());
        let mut buf = vec![0_u8; chunk as usize];
        let mut doc = Vec::new();
        let mut lens = Vec::new();
        loop {
            let (status, n) = read(s, &mut buf);
            if status == END {
                assert_eq!(n, 0);
                break;
            }
            assert_eq!(status, OK);
            lens.push(n);
            doc.extend_from_slice(&buf[..n]);
        }
        let mut count = 0;
        assert_eq!(copied(s, Some(&mut count)), OK);
        assert_eq!(count, size);
        assert_eq!(rq_bench_close(s), OK);
        (doc, lens)
    }

    #[test]
    fn c_abi_reads_the_document() {
        let (doc, lens) = read_all(1 << 20, 16_384);
        assert_eq!(lens, vec![16_384; 64]);
        assert_eq!(doc.len(), 1 << 20);
        assert!(doc.iter().enumerate().all(|(i, b)| *b == byte_at(i as u64)));
        let (_, lens) = read_all(1 << 20, 65_536);
        assert_eq!(lens, vec![65_536; 16]);
        let (doc, lens) = read_all(40_000, 16_384);
        assert_eq!(lens, vec![16_384, 16_384, 7_232]);
        assert_eq!(doc, (0..40_000_u64).map(byte_at).collect::<Vec<_>>());
    }

    #[test]
    fn c_abi_null_handle() {
        let mut buf = [0_u8; 16];
        assert_eq!(read(ptr::null_mut(), &mut buf), (E_HANDLE, 0));
        // A forged handle is a table miss, not a dereference.
        let forged = ptr::without_provenance_mut::<RqBenchStream>(0xdead_beef);
        assert_eq!(read(forged, &mut buf), (E_HANDLE, 0));
        assert_eq!(rq_bench_close(ptr::null_mut()), E_HANDLE);
        assert_eq!(rq_bench_close(forged), E_HANDLE);
        let mut count = 7;
        assert_eq!(copied(ptr::null_mut(), Some(&mut count)), E_HANDLE);
        assert_eq!(count, 7, "not written on error");
    }

    #[test]
    fn c_abi_close_twice() {
        let s = rq_bench_open(100, 10);
        assert!(!s.is_null());
        assert_eq!(rq_bench_close(s), OK);
        assert_eq!(rq_bench_close(s), E_HANDLE);
        let mut buf = [0_u8; 10];
        assert_eq!(read(s, &mut buf), (E_HANDLE, 0), "read after close");
        assert_eq!(copied(s, Some(&mut 0)), E_HANDLE, "copied after close");
    }

    #[test]
    fn c_abi_read_after_end() {
        let s = rq_bench_open(5, 4);
        let mut buf = [0_u8; 4];
        assert_eq!(read(s, &mut buf), (OK, 4));
        assert_eq!(read(s, &mut buf), (OK, 1));
        assert_eq!(buf[0], byte_at(4));
        for _ in 0..3 {
            assert_eq!(read(s, &mut buf), (END, 0));
        }
        assert_eq!(rq_bench_close(s), OK);
    }

    #[test]
    fn c_abi_bad_arguments() {
        assert!(rq_bench_open(10, 0).is_null());
        assert!(rq_bench_open(10, 65_537).is_null());
        assert!(rq_bench_open(u64::MAX, 1).is_null());
        let s = rq_bench_open(10, 4);
        let mut buf = [0_u8; 4];
        // Null out_len: an error, and the stream does not move.
        assert_eq!(read_with(s, Some(&mut buf), 4, None), E_NULL);
        // Null buffer: out_len is 0.
        let mut n = 99;
        assert_eq!(read_with(s, None, 4, Some(&mut n)), E_NULL);
        assert_eq!(n, 0);
        // A buffer of 0 bytes cannot make progress.
        n = 99;
        assert_eq!(read_with(s, Some(&mut buf), 0, Some(&mut n)), E_ARG);
        assert_eq!(n, 0);
        // A smaller buffer gives a shorter slice; nothing is lost.
        let mut small = [0_u8; 3];
        assert_eq!(read(s, &mut small), (OK, 3));
        assert_eq!(small, [0, 1, 2].map(byte_at));
        assert_eq!(read(s, &mut buf), (OK, 4));
        assert_eq!(buf, [3, 4, 5, 6].map(byte_at));
        assert_eq!(read(s, &mut buf), (OK, 3));
        assert_eq!(read(s, &mut buf), (END, 0));
        assert_eq!(copied(s, None), E_NULL);
        assert_eq!(rq_bench_close(s), OK);
    }

    #[test]
    fn c_abi_writes_only_inside_cap() {
        let s = rq_bench_open(100, 64);
        let mut buf = [0xee_u8; 32];
        let mut n = 0;
        assert_eq!(read_with(s, Some(&mut buf), 8, Some(&mut n)), OK);
        assert_eq!(n, 8);
        assert!(buf[8..].iter().all(|&b| b == 0xee));
        assert_eq!(rq_bench_close(s), OK);
    }

    #[test]
    fn c_abi_sha() {
        let mut out = [0_u8; 32];
        assert_eq!(sha(40_000, Some(&mut out), 32), OK);
        assert_eq!(out, crate::doc_sha256(40_000).unwrap());
        assert_eq!(sha(1, Some(&mut out), 31), E_ARG);
        assert_eq!(sha(u64::MAX, Some(&mut out), 32), E_ARG);
        assert_eq!(sha(1, None, 32), E_NULL);
    }

    #[test]
    fn header_matches_the_rust_values() {
        let header = include_str!("../../include/rq_bench.h");
        for (name, value) in [
            ("RQ_BENCH_OK", OK),
            ("RQ_BENCH_END", END),
            ("RQ_BENCH_E_HANDLE", E_HANDLE),
            ("RQ_BENCH_E_NULL", E_NULL),
            ("RQ_BENCH_E_ARG", E_ARG),
            ("RQ_BENCH_E_PANIC", E_PANIC),
            ("RQ_BENCH_E_JNI", super::super::E_JNI),
        ] {
            let line = format!("#define {name} ({value})");
            assert!(header.contains(&line), "header line: {line}");
        }
        for f in [
            "RqBenchStream *rq_bench_open(uint64_t size, uint32_t chunk);",
            "int32_t rq_bench_read(RqBenchStream *s, uint8_t *buf, size_t cap, size_t *out_len);",
            "int32_t rq_bench_close(RqBenchStream *s);",
            "int32_t rq_bench_copied(RqBenchStream *s, uint64_t *out);",
            "int32_t rq_bench_sha256(uint64_t size, uint8_t *out, size_t cap);",
        ] {
            assert!(header.contains(f), "header declaration: {f}");
        }
    }

    /// Review L-5: each pointer contract of a `# Safety` section is in the C header, so a C or
    /// Swift caller sees the same rules as a Rust caller.
    #[test]
    fn header_has_the_pointer_contracts() {
        let header = include_str!("../../include/rq_bench.h");
        // Only the code above the tests, so the strings of this test do not match themselves.
        let source = include_str!("c.rs");
        let code = source.split("#[cfg(test)]").next().unwrap();
        for (c_text, rust_text) in [
            (
                "Safety: buf is NULL or valid for writes of cap bytes.",
                "`buf` is null or valid for writes of `cap` bytes.",
            ),
            (
                "Safety: out_len is NULL or valid for one aligned size_t write.",
                "`out_len` is null or valid for one\n/// aligned `size_t` write.",
            ),
            (
                "Safety: buf and out_len do not overlap.",
                "`buf` and `out_len` do not overlap.",
            ),
            (
                "Safety: out is NULL or valid for one aligned uint64_t write.",
                "`out` is null or valid for one aligned `uint64_t` write.",
            ),
            (
                "Safety: out is NULL or valid for writes of cap bytes.",
                "`out` is null or valid for writes of `cap` bytes.",
            ),
            (
                "No other thread reads or writes that memory during the call.",
                "No other thread reads or writes",
            ),
            ("s: any value.", "`s` may be any value."),
        ] {
            assert!(header.contains(c_text), "header: {c_text}");
            assert!(code.contains(rust_text), "Rust # Safety: {rust_text}");
        }
        // Three unsafe exports, three `# Safety` sections; the safe exports take no pointer.
        assert_eq!(code.matches("pub unsafe extern \"C\" fn").count(), 3);
        assert_eq!(code.matches("pub extern \"C\" fn").count(), 2);
    }
}
