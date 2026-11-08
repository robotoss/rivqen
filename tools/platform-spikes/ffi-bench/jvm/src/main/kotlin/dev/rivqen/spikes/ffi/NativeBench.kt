// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spikes.ffi

import java.nio.ByteBuffer

/**
 * JNI functions of the C ABI path (`src/abi/jni.rs` of the Rust crate).
 *
 * Read results: `n > 0` bytes read, `0` end of the document, a negative status code on error
 * (the codes of `include/rq_bench.h`). The caller loads the library with [System.load] first.
 */
internal object NativeBench {
    const val E_HANDLE = -1
    const val E_NULL = -2
    const val E_ARG = -3
    const val E_JNI = -5

    /** A stream handle, or 0 for a bad argument. */
    @JvmStatic external fun open(size: Long, chunk: Int): Long

    /**
     * Writes the next chunk at address 0 of the direct [buffer], up to its capacity, and
     * ignores its position and limit. A read-only buffer gives [E_ARG] without a native call
     * (review L-1); the native side checks it again. No other thread may use [buffer] during
     * the call.
     */
    fun readDirect(handle: Long, buffer: ByteBuffer): Int =
        if (buffer.isReadOnly) E_ARG else readDirectRaw(handle, buffer)

    /** The JNI function behind [readDirect]: it also refuses a read-only buffer ([E_ARG]). */
    @JvmStatic external fun readDirectRaw(handle: Long, buffer: ByteBuffer): Int

    /** Copies the next chunk to [array] from index 0 (`SetByteArrayRegion`). */
    @JvmStatic external fun readArray(handle: Long, array: ByteArray): Int

    /** 0, or [E_HANDLE] when the stream is not open. */
    @JvmStatic external fun close(handle: Long): Int

    /** Bytes that the Rust side copied out of the stream, or a negative status code. */
    @JvmStatic external fun copied(handle: Long): Long

    /** SHA-256 of the document of [size] bytes, or null. */
    @JvmStatic external fun sha256(size: Long): ByteArray?

    /** Peak resident memory of the process in bytes (`getrusage`), or -1. */
    @JvmStatic external fun peakRssBytes(): Long
}
