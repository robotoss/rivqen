// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spikes.ffi

import java.io.File
import java.nio.ByteBuffer
import kotlin.system.exitProcess
import uniffi.ffi_bench.openStream

/*
 * JVM harness CLI (run.sh starts one JVM per case, so the peak RSS of a case is its own):
 *   --lib <libffi_bench.dylib> --selfcheck
 *   --lib <libffi_bench.dylib> --versions <out.json>
 *   --lib <libffi_bench.dylib> --path <id> --size N --chunk N --warmup N --docs N --out <case.json>
 * Exit 0 on success; 1 when a check fails (also a SHA-256 mismatch); 2 for a usage error.
 */

private fun args(argv: Array<String>): Map<String, String> {
    val m = HashMap<String, String>()
    var i = 0
    while (i < argv.size) {
        val k = argv[i]
        if (!k.startsWith("--")) throw IllegalArgumentException("unexpected $k")
        if (k == "--selfcheck") {
            m["selfcheck"] = "1"
            i++
            continue
        }
        if (i + 1 >= argv.size) throw IllegalArgumentException("$k needs a value")
        m[k.removePrefix("--")] = argv[i + 1]
        i += 2
    }
    return m
}

private fun check(ok: Boolean, what: String) {
    if (!ok) throw BenchFailure("self-check: $what")
}

/** Error paths of the JNI and UniFFI paths, and the percentile helper. */
private fun selfCheck() {
    val array = ByteArray(16)
    val direct = ByteBuffer.allocateDirect(16)
    // Null (0), negative and unknown handles.
    for (h in longArrayOf(0L, -1L, 0x7fff_ffffL)) {
        check(NativeBench.readArray(h, array) == NativeBench.E_HANDLE, "readArray bad handle $h")
        check(NativeBench.readDirect(h, direct) == NativeBench.E_HANDLE, "readDirect bad handle $h")
        check(NativeBench.close(h) == NativeBench.E_HANDLE, "close bad handle $h")
        check(NativeBench.copied(h) == NativeBench.E_HANDLE.toLong(), "copied bad handle $h")
    }
    // Bad open arguments.
    check(NativeBench.open(1, 0) == 0L, "open chunk 0")
    check(NativeBench.open(1, 65_537) == 0L, "open chunk 65537")
    check(NativeBench.open(-1, 16) == 0L, "open negative size")
    check(NativeBench.open(1, -1) == 0L, "open negative chunk")
    check(NativeBench.sha256(-1) == null, "sha256 negative size")
    // Close twice; read after close.
    val h = NativeBench.open(10, 4)
    check(h != 0L, "open")
    // A heap buffer has no address: a JNI error, and the stream does not move.
    check(NativeBench.readDirect(h, ByteBuffer.allocate(16)) == NativeBench.E_JNI, "heap buffer")
    check(NativeBench.readArray(h, ByteArray(0)) == NativeBench.E_ARG, "empty array")
    check(NativeBench.readArray(h, array) == 4, "first read")
    check(array.copyOf(4).contentEquals(byteArrayOf(7, 38, 69, 100)), "first bytes")
    check(NativeBench.readDirect(h, direct) == 4, "direct read")
    check(direct.get(0) == 131.toByte(), "direct byte 4")
    check(NativeBench.readArray(h, array) == 2, "short last chunk")
    check(NativeBench.readArray(h, array) == 0, "end")
    check(NativeBench.readArray(h, array) == 0, "end again")
    check(NativeBench.copied(h) == 10L, "copied")
    check(NativeBench.close(h) == 0, "close")
    check(NativeBench.close(h) == NativeBench.E_HANDLE, "close twice")
    check(NativeBench.readArray(h, array) == NativeBench.E_HANDLE, "read after close")
    check(NativeBench.peakRssBytes() > 0, "peak RSS")
    // UniFFI: a bad open is an empty stream; close twice is a no-op; a call after close throws.
    val bad = openStream(10u, 0u)
    check(bad.nextChunk() == null, "uniffi bad open")
    bad.close()
    val s = openStream(5u, 4u)
    check(s.nextChunk()?.size == 4, "uniffi first chunk")
    s.close()
    s.close()
    val afterClose = runCatching { s.nextChunk() }
    check(afterClose.exceptionOrNull() is IllegalStateException, "uniffi call after close")
    // Percentiles (nearest rank).
    val v = LongArray(1000) { 1000L - it } // 1..1000
    check(percentile(v, 50) == 500L && percentile(v, 95) == 950L, "percentile 1000")
    check(percentile(longArrayOf(3, 1, 2), 50) == 2L, "percentile odd")
    check(percentile(longArrayOf(4, 1, 3, 2), 50) == 2L, "percentile even")
    check(percentile(longArrayOf(9), 95) == 9L, "percentile one")
    check(kotlinDocSha256(0).size == 32, "kotlin sha")
}

private fun json(r: CaseResult): String =
    "{\"chunk\":${r.chunk},\"chunk_median_ns\":${r.chunkMedianNs},\"copied_bytes\":${r.copiedBytes}," +
        "\"docs\":${r.docs},\"median_ns\":${r.medianNs},\"p95_ns\":${r.p95Ns},\"path\":\"${r.path}\"," +
        "\"rss_delta_kib\":${r.rssDeltaKib},\"sha256_ok\":${r.sha256Ok},\"size\":${r.size}}\n"

private fun versions(): String {
    val kotlin = KotlinVersion.CURRENT.toString()
    val jna = com.sun.jna.Native.VERSION
    val java = System.getProperty("java.runtime.version") ?: "unknown"
    return "{\"java\":\"$java\",\"jna\":\"$jna\",\"kotlin\":\"$kotlin\"}\n"
}

fun main(argv: Array<String>) {
    val a = try {
        args(argv)
    } catch (e: IllegalArgumentException) {
        System.err.println("usage: ${e.message}")
        exitProcess(2)
    }
    val lib = a["lib"] ?: run {
        System.err.println("usage: --lib is required")
        exitProcess(2)
    }
    try {
        System.load(File(lib).absolutePath)
        // The UniFFI bindings load the same file through JNA.
        System.setProperty("uniffi.component.ffi_bench.libraryOverride", File(lib).absolutePath)
        when {
            a["selfcheck"] != null -> {
                selfCheck()
                println("self-check ok")
            }
            a["versions"] != null -> File(a.getValue("versions")).writeText(versions())
            else -> {
                val path = JvmPath.of(a["path"] ?: throw IllegalArgumentException("--path"))
                val size = a["size"]?.toLongOrNull() ?: throw IllegalArgumentException("--size")
                val chunk = a["chunk"]?.toIntOrNull() ?: throw IllegalArgumentException("--chunk")
                val warmup = a["warmup"]?.toIntOrNull() ?: 200
                val docs = a["docs"]?.toIntOrNull() ?: 1000
                val out = a["out"] ?: throw IllegalArgumentException("--out")
                val r = JvmBench(path, size, chunk) { NativeBench.peakRssBytes() }.run(warmup, docs)
                File(out).writeText(json(r))
                println("${r.path} chunk ${r.chunk}: median ${r.medianNs} ns, p95 ${r.p95Ns} ns")
            }
        }
    } catch (e: IllegalArgumentException) {
        System.err.println("usage: ${e.message}")
        exitProcess(2)
    } catch (e: BenchFailure) {
        System.err.println("FAIL: ${e.message}")
        exitProcess(1)
    }
}
