// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! JNI glue of the C ABI path for the Kotlin class `dev.rivqen.spikes.ffi.NativeBench`.
//!
//! The JVM passes handles as `long`. Read results: `n > 0` bytes read, `0` end of the
//! document, a negative status code of [`super`] on error.
//!
//! - `readDirect` writes into the memory of a direct `ByteBuffer` (from address 0, up to its
//!   capacity); one copy, no JVM heap allocation.
//! - `readArray` copies into a `byte[]` with `SetByteArrayRegion`; one copy.
//!
//! Every function runs in [`guard`], and `EnvUnowned::with_env` catches panics as well.

use std::num::NonZeroUsize;

use jni::objects::{JByteArray, JByteBuffer, JClass};
use jni::sys::{jint, jlong};
use jni::{EnvUnowned, Outcome};

use super::c::read_raw;
use super::{E_ARG, E_HANDLE, E_JNI, E_PANIC, END, OK, code, guard};
use crate::registry;

/// Handle as `usize`; a negative `long` names no stream.
fn handle(h: jlong) -> Option<usize> {
    usize::try_from(h).ok()
}

/// JNI result of a read status and byte count.
fn read_result(status: i32, n: usize) -> jint {
    match status {
        OK => jint::try_from(n).unwrap_or(E_ARG),
        END => 0,
        e => e,
    }
}

fn outcome<E>(o: &Outcome<jint, E>) -> jint {
    match o {
        Outcome::Ok(v) => *v,
        Outcome::Err(_) => E_JNI,
        Outcome::Panic(_) => E_PANIC,
    }
}

/// The bytes of `s` as JNI `jbyte` values.
fn as_jbytes(s: &[u8]) -> &[i8] {
    // SAFETY: `u8` and `i8` have the same size and alignment and every bit pattern is valid
    // for both; the new slice covers the same memory and has the lifetime of `s`.
    unsafe { std::slice::from_raw_parts(s.as_ptr().cast::<i8>(), s.len()) }
}

/// `long open(long size, int chunk)`: a handle, or 0 for a bad argument.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_rivqen_spikes_ffi_NativeBench_open<'l>(
    _env: EnvUnowned<'l>,
    _class: JClass<'l>,
    size: jlong,
    chunk: jint,
) -> jlong {
    guard(0, || {
        let (Ok(size), Ok(chunk)) = (u64::try_from(size), u32::try_from(chunk)) else {
            return 0;
        };
        registry::global()
            .open(size, chunk)
            .ok()
            .and_then(|h| jlong::try_from(h.get()).ok())
            .unwrap_or(0)
    })
}

/// `int readDirect(long handle, ByteBuffer buffer)` for a direct buffer.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_rivqen_spikes_ffi_NativeBench_readDirect<'l>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    stream: jlong,
    buffer: JByteBuffer<'l>,
) -> jint {
    guard(E_PANIC, || {
        let Some(h) = handle(stream) else {
            return E_HANDLE;
        };
        outcome(
            &env.with_env(|env| -> jni::errors::Result<jint> {
                // A heap buffer has no address: GetDirectBufferAddress gives null, and the
                // crate returns an error (E_JNI).
                let address = env.get_direct_buffer_address(&buffer)?;
                let capacity = env.get_direct_buffer_capacity(&buffer)?;
                // SAFETY: `address` and `capacity` come from GetDirectBufferAddress and
                // GetDirectBufferCapacity of a direct buffer; the local reference `buffer` keeps
                // the buffer alive until this native call returns, so its memory is valid for
                // writes of `capacity` bytes during `read_raw`.
                let (status, n) = unsafe { read_raw(h, address, capacity) };
                Ok(read_result(status, n))
            })
            .into_outcome(),
        )
    })
}

/// `int readArray(long handle, byte[] array)`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_rivqen_spikes_ffi_NativeBench_readArray<'l>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    stream: jlong,
    array: JByteArray<'l>,
) -> jint {
    guard(E_PANIC, || {
        let Some(h) = handle(stream) else {
            return E_HANDLE;
        };
        outcome(
            &env.with_env(|env| -> jni::errors::Result<jint> {
                let Some(cap) = NonZeroUsize::new(array.len(env)?) else {
                    return Ok(E_ARG);
                };
                let read = registry::global().with(h, |s| {
                    s.read_with(cap, |slice| array.set_region(env, 0, as_jbytes(slice)))
                });
                Ok(match read {
                    Err(e) => code(e),
                    Ok(Ok(Some(n))) => read_result(OK, n),
                    Ok(Ok(None)) => 0,
                    Ok(Err(_)) => E_JNI,
                })
            })
            .into_outcome(),
        )
    })
}

/// `int close(long handle)`: 0, or `E_HANDLE` when the stream is not open.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_rivqen_spikes_ffi_NativeBench_close<'l>(
    _env: EnvUnowned<'l>,
    _class: JClass<'l>,
    stream: jlong,
) -> jint {
    guard(E_PANIC, || {
        let Some(h) = handle(stream) else {
            return E_HANDLE;
        };
        match registry::global().close(h) {
            Ok(()) => OK,
            Err(e) => code(e),
        }
    })
}

/// `long copied(long handle)`: bytes the Rust side copied, or a negative status code.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_rivqen_spikes_ffi_NativeBench_copied<'l>(
    _env: EnvUnowned<'l>,
    _class: JClass<'l>,
    stream: jlong,
) -> jlong {
    guard(jlong::from(E_PANIC), || {
        let Some(h) = handle(stream) else {
            return jlong::from(E_HANDLE);
        };
        match registry::global().with(h, |s| s.copied()) {
            Ok(n) => jlong::try_from(n).unwrap_or(jlong::MAX),
            Err(e) => jlong::from(code(e)),
        }
    })
}

/// `byte[] sha256(long size)`: the 32-byte digest, or null for a bad size or a JNI error.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_rivqen_spikes_ffi_NativeBench_sha256<'l>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    size: jlong,
) -> JByteArray<'l> {
    guard(JByteArray::default(), || {
        let Some(digest) = u64::try_from(size).ok().and_then(crate::doc_sha256) else {
            return JByteArray::default();
        };
        match env
            .with_env(|env| -> jni::errors::Result<JByteArray<'l>> {
                env.byte_array_from_slice(&digest)
            })
            .into_outcome()
        {
            Outcome::Ok(a) => a,
            Outcome::Err(_) | Outcome::Panic(_) => JByteArray::default(),
        }
    })
}

/// `long peakRssBytes()`: peak resident memory of the process (`getrusage`), or -1.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_rivqen_spikes_ffi_NativeBench_peakRssBytes<'l>(
    _env: EnvUnowned<'l>,
    _class: JClass<'l>,
) -> jlong {
    guard(-1, || {
        super::sys::peak_rss_bytes()
            .and_then(|b| jlong::try_from(b).ok())
            .unwrap_or(-1)
    })
}

#[cfg(test)]
mod tests {
    //! The exported functions need a JVM; the JVM harness self-check (`Main.kt`) calls them.
    //! These tests cover the pure helpers.

    use super::*;

    #[test]
    fn jni_helpers() {
        assert_eq!(handle(-1), None);
        assert_eq!(handle(0), Some(0));
        assert_eq!(handle(42), Some(42));
        assert_eq!(read_result(OK, 16_384), 16_384);
        assert_eq!(read_result(END, 0), 0);
        assert_eq!(read_result(E_HANDLE, 0), E_HANDLE);
        assert_eq!(read_result(OK, usize::MAX), E_ARG);
        assert_eq!(outcome::<()>(&Outcome::Ok(5)), 5);
        assert_eq!(outcome(&Outcome::Err(())), E_JNI);
        assert_eq!(outcome::<()>(&Outcome::Panic(Box::new(()))), E_PANIC);
        assert_eq!(as_jbytes(&[0, 1, 127, 128, 255]), &[0, 1, 127, -128, -1]);
    }
}
