// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spikes.ffi

import java.nio.ByteBuffer
import java.security.MessageDigest
import uniffi.ffi_bench.DocStream
import uniffi.ffi_bench.docSha256
import uniffi.ffi_bench.openStream

/**
 * Measurement core of the JVM harness. It uses only APIs that Android has as well
 * (`System.nanoTime`, `MessageDigest`, direct buffers), so S2 can run it on the emulator.
 *
 * Timing: the time of a document is the sum of the timed segments open, every read (also the
 * last read that reports the end) and close. The SHA-256 update of a chunk runs between two
 * segments and is not timed. Each segment includes one `System.nanoTime` pair.
 */
internal enum class JvmPath(val id: String, val glueCopiesPerByte: Int) {
    /**
     * UniFFI through JNA. Rust copies the slice into a `Vec` (counted by Rust), the UniFFI
     * scaffolding writes it into a `RustBuffer` (`Vec<T>::write`, one copy), and the generated
     * `FfiConverterByteArray.read` copies the `RustBuffer` into a new `ByteArray` (one copy).
     */
    UNIFFI_KOTLIN("uniffi-kotlin", 2),

    /** JNI, Rust writes into the memory of a reused direct `ByteBuffer` (counted by Rust). */
    JNI_DIRECT("jni-direct", 0),

    /** JNI, Rust copies into a reused `ByteArray` with `SetByteArrayRegion` (counted by Rust). */
    JNI_BYTEARRAY("jni-bytearray", 0),
    ;

    companion object {
        fun of(id: String): JvmPath =
            entries.firstOrNull { it.id == id } ?: throw IllegalArgumentException("unknown path $id")
    }
}

internal class BenchFailure(message: String) : Exception(message)

internal data class CaseResult(
    val path: String,
    val size: Long,
    val chunk: Int,
    val docs: Int,
    val medianNs: Long,
    val p95Ns: Long,
    val chunkMedianNs: Long,
    val copiedBytes: Long,
    val rssDeltaKib: Long,
    val sha256Ok: Boolean,
)

/** Nearest-rank percentile `p` (0 < p <= 100) of [values]; sorts a copy. */
internal fun percentile(values: LongArray, p: Int): Long {
    require(values.isNotEmpty()) { "no values" }
    require(p in 1..100) { "bad percentile $p" }
    val sorted = values.copyOf().also { it.sort() }
    val rank = ((p.toLong() * sorted.size + 99) / 100).toInt() // ceil(p * n / 100)
    return sorted[rank - 1]
}

/** SHA-256 of the document computed in Kotlin from the formula of CONTRACT.md section 10. */
internal fun kotlinDocSha256(size: Long): ByteArray {
    val md = MessageDigest.getInstance("SHA-256")
    val block = ByteArray(65_536)
    var i = 0L
    while (i < size) {
        val n = minOf(block.size.toLong(), size - i).toInt()
        for (k in 0 until n) {
            block[k] = (((i + k) % 251 * 31 + 7) % 251).toByte()
        }
        md.update(block, 0, n)
        i += n
    }
    return md.digest()
}

/** Sums timed segments; no allocation, so the timed loop of the JNI paths makes no garbage. */
private class Timer {
    var docNs = 0L
    var last = 0L

    inline fun <T> time(block: () -> T): T {
        val t0 = System.nanoTime()
        val v = block()
        last = System.nanoTime() - t0
        docNs += last
        return v
    }
}

/** Result of one document. */
private class DocRun(val bytes: Long, val rustCopied: Long, val docNs: Long, val chunks: Int)

internal class JvmBench(
    private val path: JvmPath,
    private val size: Long,
    private val chunk: Int,
    private val rssBytes: () -> Long,
) {
    private val md: MessageDigest = MessageDigest.getInstance("SHA-256")
    private val direct: ByteBuffer = ByteBuffer.allocateDirect(chunk)
    private val array = ByteArray(chunk)
    private val expected: ByteArray = when (path) {
        JvmPath.UNIFFI_KOTLIN -> docSha256(size.toULong())
        JvmPath.JNI_DIRECT, JvmPath.JNI_BYTEARRAY -> NativeBench.sha256(size)
    } ?: throw BenchFailure("no Rust SHA-256 for size $size")

    init {
        if (!expected.contentEquals(kotlinDocSha256(size))) {
            throw BenchFailure("Rust SHA-256 differs from the Kotlin SHA-256 of the formula")
        }
    }

    fun run(warmup: Int, docs: Int): CaseResult {
        require(warmup >= 0 && docs > 0) { "bad document counts" }
        val maxChunks = ((size + chunk - 1) / chunk).toInt()
        val docNs = LongArray(docs)
        val chunkNs = LongArray(docs * maxChunks)
        var chunkCount = 0
        var copied = -1L
        val rssBefore = rssBytes()
        for (d in 0 until warmup + docs) {
            val measured = d >= warmup
            val r = oneDocument(if (measured) chunkNs else null, chunkCount)
            if (r.bytes != size) throw BenchFailure("document $d: ${r.bytes} bytes, want $size")
            if (r.rustCopied != size) throw BenchFailure("document $d: Rust copied ${r.rustCopied}")
            val docCopied = r.rustCopied + path.glueCopiesPerByte * r.bytes
            if (copied >= 0 && copied != docCopied) throw BenchFailure("copied bytes changed")
            copied = docCopied
            if (measured) {
                docNs[d - warmup] = r.docNs
                chunkCount += r.chunks
            }
        }
        val rssAfter = rssBytes()
        if (rssBefore < 0 || rssAfter < 0) throw BenchFailure("peak RSS not available")
        return CaseResult(
            path = path.id,
            size = size,
            chunk = chunk,
            docs = docs,
            medianNs = percentile(docNs, 50),
            p95Ns = percentile(docNs, 95),
            chunkMedianNs = percentile(chunkNs.copyOf(chunkCount), 50),
            copiedBytes = copied,
            rssDeltaKib = (rssAfter - rssBefore) / 1024,
            sha256Ok = true,
        )
    }

    /** Reads one document; per-chunk times go to [chunkNs] from [chunkAt]; a SHA-256 mismatch throws. */
    private fun oneDocument(chunkNs: LongArray?, chunkAt: Int): DocRun {
        md.reset()
        val timer = Timer()
        var bytes = 0L
        var chunks = 0
        val rustCopied: Long
        when (path) {
            JvmPath.UNIFFI_KOTLIN -> {
                val s: DocStream = timer.time { openStream(size.toULong(), chunk.toUInt()) }
                while (true) {
                    val c = timer.time { s.nextChunk() } ?: break
                    chunkNs?.set(chunkAt + chunks, timer.last)
                    chunks++
                    bytes += c.size
                    md.update(c)
                }
                rustCopied = s.copiedBytes().toLong()
                timer.time { s.close() }
            }
            JvmPath.JNI_DIRECT, JvmPath.JNI_BYTEARRAY -> {
                val h = timer.time { NativeBench.open(size, chunk) }
                if (h == 0L) throw BenchFailure("open failed")
                while (true) {
                    val n = timer.time {
                        if (path == JvmPath.JNI_DIRECT) {
                            NativeBench.readDirect(h, direct)
                        } else {
                            NativeBench.readArray(h, array)
                        }
                    }
                    if (n < 0) throw BenchFailure("read error $n")
                    if (n == 0) break
                    chunkNs?.set(chunkAt + chunks, timer.last)
                    chunks++
                    bytes += n
                    if (path == JvmPath.JNI_DIRECT) {
                        direct.clear().limit(n)
                        md.update(direct)
                        direct.clear()
                    } else {
                        md.update(array, 0, n)
                    }
                }
                rustCopied = NativeBench.copied(h)
                val rc = timer.time { NativeBench.close(h) }
                if (rc != 0) throw BenchFailure("close error $rc")
            }
        }
        if (!md.digest().contentEquals(expected)) throw BenchFailure("SHA-256 mismatch")
        return DocRun(bytes, rustCopied, timer.docNs, chunks)
    }
}
