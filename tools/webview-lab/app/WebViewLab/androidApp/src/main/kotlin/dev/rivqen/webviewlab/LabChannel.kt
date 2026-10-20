// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.webviewlab

/**
 * The console channel of the lab world.
 *
 * With `setJavaScriptEnabled(false)`, Chromium installs no androidx.webkit message object in any
 * world (`components/js_injection/renderer/js_communication.cc`: `DidClearWindowObject` stops
 * when the frame has no main-world script context), so the lab world cannot call `postMessage`.
 * The lab-world script still runs (`ExecuteScriptInIsolatedWorld` runs when scripts are disabled),
 * and its `console.info` calls reach `WebChromeClient.onConsoleMessage`. The script sends its
 * result as numbered chunks:
 *
 *     rivqen-lab:<nonce>:<index>/<count>/<length>:<chunk>
 *
 * The nonce is random per WebView and is only in the lab-world script, which the page world
 * cannot read. The payload is ASCII JSON (every other character as `\uXXXX`).
 */
internal class ConsoleCollector(nonce: String) {
    val prefix: String = "rivqen-lab:$nonce:"

    private var chunks: Array<String?>? = null
    private var length = -1
    private var received = 0
    /** At most MAX_CHUNKS * CHUNK_CHARS (64 Mi), so it fits an Int. */
    private var stored = 0

    /** The first protocol error; the collector then ignores further messages. */
    var error: String? = null
        private set

    /** True when every chunk is in and the length matches. */
    val complete: Boolean
        get() = error == null && chunks != null && received == chunks?.size

    /**
     * Takes one console message. Returns false when the message is not for this collector
     * (another text, or another nonce). A malformed message for this collector sets [error].
     */
    fun offer(message: String): Boolean {
        if (!message.startsWith(prefix)) return false
        if (error != null || complete) return true
        val colon = message.indexOf(':', prefix.length)
        if (colon < 0) return fail("no header end")
        val head = message.substring(prefix.length, colon).split('/')
        if (head.size != 3) return fail("bad header")
        val index = head[0].toIntOrNull()
        val count = head[1].toIntOrNull()
        val total = head[2].toIntOrNull()
        if (index == null || count == null || total == null) return fail("bad header number")
        if (count !in 1..MAX_CHUNKS || index !in 0 until count || total !in 0..MAX_PAYLOAD_CHARS) {
            return fail("header out of range")
        }
        val store = chunks ?: arrayOfNulls<String>(count).also {
            chunks = it
            length = total
        }
        if (store.size != count || length != total) return fail("header changed")
        if (store[index] != null) return fail("chunk $index twice")
        val chunk = message.substring(colon + 1)
        if (chunk.length > CHUNK_CHARS) return fail("chunk $index too long")
        stored += chunk.length
        if (stored > length) return fail("more text than the header length")
        store[index] = chunk
        received++
        if (received == count && stored != length) return fail("length mismatch")
        return true
    }

    /** The payload text, or null when it is not complete. */
    fun payload(): String? {
        val store = chunks ?: return null
        if (!complete) return null
        return buildString(length) { for (c in store) append(c) }
    }

    private fun fail(why: String): Boolean {
        error = why
        return true
    }

    companion object {
        /** Bound for the chunk count. */
        const val MAX_CHUNKS: Int = 4096

        /** The script sends chunks of this many characters. */
        const val CHUNK_CHARS: Int = 16 * 1024

        /** Bound for the payload. */
        const val MAX_PAYLOAD_CHARS: Int = LabJson.MAX_SCRIPT_VALUE_CHARS
    }
}
