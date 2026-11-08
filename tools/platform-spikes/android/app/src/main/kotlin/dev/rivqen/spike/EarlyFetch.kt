// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spike

import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import java.io.IOException
import java.io.InputStream
import java.security.MessageDigest
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ExecutionException
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException

/**
 * The native early fetch of the page (CAP-A-02). [start] runs before the WebView exists. The head
 * is published for the interceptor; the body is pumped into the [pipe] on the OkHttp callback
 * thread, with back pressure from the pipe.
 *
 * Events: `app.fetch.start`, `app.fetch.head`, `app.fetch.end`, and `app.error` on a failure.
 */
internal class EarlyFetch(
    private val client: OkHttpClient,
    private val url: HttpUrl,
    private val log: EventLog,
    private val pipe: BridgePipe,
    private val onEnd: (Result) -> Unit,
) {
    /** The fetched head. [redirected] is true when the response has a prior response. */
    class Head(val status: Int, val reason: String, val pairs: List<Array<String>>, val redirected: Boolean)

    /** What the pump read from the network; [t] is the app time of the end. */
    class Result(val bytes: Long, val sha256: String, val complete: Boolean, val error: String?) {
        var t: Long = 0L
            internal set
    }

    private val head = CompletableFuture<Head>()

    @Volatile
    private var call: Call? = null

    fun start() {
        log.emit("app.fetch.start", mapOf("url" to url.toString()))
        val c = client.newCall(Request.Builder().url(url).get().build())
        call = c
        c.enqueue(
            object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    head.completeExceptionally(e)
                    pipe.fail(e)
                    log.emit("app.error", mapOf("error" to describe(e)))
                    end(Result(0, hex(sha256().digest()), complete = false, error = describe(e)))
                }

                override fun onResponse(call: Call, response: Response) {
                    response.use { r ->
                        val pairs = r.headers.map { (n, v) -> arrayOf(n, v) }
                        log.emit(
                            "app.fetch.head",
                            mapOf(
                                "url" to url.toString(),
                                "status" to r.code,
                                "fields" to pairs.map { listOf(it[0], InterceptRule.loggable(it[0], it[1])) },
                            ),
                        )
                        head.complete(Head(r.code, r.message, pairs, r.priorResponse != null))
                        end(pump(r.body.byteStream()))
                    }
                }
            },
        )
    }

    /** The head, or null when it does not come within [millis] or the fetch failed. */
    fun awaitHead(millis: Long): Head? =
        try {
            head.get(millis, TimeUnit.MILLISECONDS)
        } catch (e: TimeoutException) {
            log.emit("app.error", mapOf("error" to "head timeout after $millis ms"))
            null
        } catch (e: ExecutionException) {
            // The failure is already logged by onFailure.
            null
        } catch (e: InterruptedException) {
            Thread.currentThread().interrupt()
            log.emit("app.error", mapOf("error" to describe(e)))
            null
        }

    /** Stops the network call (the reader closed the pipe or the run ends). */
    fun cancel() {
        call?.cancel()
    }

    private fun pump(body: InputStream): Result {
        val md = sha256()
        val buf = ByteArray(PUMP_BUFFER)
        var total = 0L
        var complete = false
        var error: String? = null
        try {
            while (true) {
                val n = body.read(buf)
                if (n == -1) break
                md.update(buf, 0, n)
                total += n
                pipe.write(buf, 0, n)
            }
            pipe.finish()
            complete = true
        } catch (e: BridgePipe.CancelledException) {
            // The WebView closed the stream: stop the network read. Not an error of the fetch.
            cancel()
        } catch (e: IOException) {
            error = describe(e)
            pipe.fail(e)
            cancel()
            log.emit("app.error", mapOf("error" to error))
        } catch (e: RuntimeException) {
            // A defect: end the stream with the error instead of leaving the reader to the stall bound.
            error = describe(e)
            pipe.fail(e)
            cancel()
            log.emit("app.error", mapOf("error" to error))
        }
        return Result(total, hex(md.digest()), complete, error)
    }

    /**
     * The end of the fetch. `app.fetch.end` is not written here: the end of the network body races
     * the page probes of the last chunks (the host sends the last chunk when the beacon of the chunk
     * before it arrives, and the page sends that beacon before its chunk script runs). The owner
     * writes the event at the end of the run with [Result.t] ([writeEnd]).
     */
    private fun end(result: Result) {
        result.t = log.now()
        onEnd(result)
    }

    /** Writes `app.fetch.end` for [result] (called once, at the end of the run). */
    fun writeEnd(result: Result) {
        log.emitAt("app.fetch.end", mapOf("url" to url.toString(), "bytes" to result.bytes, "complete" to result.complete), result.t)
    }

    companion object {
        private const val PUMP_BUFFER = 8192

        fun sha256(): MessageDigest = MessageDigest.getInstance("SHA-256")

        fun hex(bytes: ByteArray): String {
            val digits = "0123456789abcdef"
            val sb = StringBuilder(bytes.size * 2)
            for (b in bytes) {
                val v = b.toInt() and 0xFF
                sb.append(digits[v shr 4]).append(digits[v and 0xF])
            }
            return sb.toString()
        }

        /** Class name and message of an error; no data. */
        fun describe(e: Throwable): String = "${e.javaClass.name}: ${e.message}"
    }
}
