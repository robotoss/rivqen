// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spike

import android.annotation.SuppressLint
import android.app.Activity
import android.graphics.Bitmap
import android.net.http.SslError
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.WindowManager
import android.webkit.RenderProcessGoneDetail
import android.webkit.SslErrorHandler
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import okhttp3.OkHttpClient
import java.io.File
import java.io.IOException
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/**
 * Prototype A-STREAM (CONTRACT.md, CAP-A-01…03). One run per launch:
 *
 * 1. Start the OkHttp early fetch of the page ([EarlyFetch]) — before the WebView exists.
 * 2. Create the WebView, install the probe channel ([ProbeBridge]) and the client, call `loadUrl`.
 * 3. `shouldInterceptRequest` gives the main-frame request the fetched head and the reader side of
 *    the [BridgePipe] when [InterceptRule] allows it (A-04); else the request passes.
 * 4. The run ends when the page finished, the DOMContentLoaded probes arrived, the fetch ended and
 *    the pipe has its outcome (then a short wait for `metric.first_paint`), or at [RUN_TIMEOUT_MS].
 *
 * Output in `files/out/`: `app.jsonl` (app part of the run log), `metrics.json`, `device.json`, and
 * last `status.json` (`done` or `failed`). run.sh reads them with `run-as`.
 *
 * Threads: WebView calls and the run state on the main thread; the interceptor and the pipe reader
 * on WebView threads; the pump on an OkHttp thread. Cross-thread results reach the run state
 * through [main].
 */
class SpikeActivity : Activity() {
    private val main = Handler(Looper.getMainLooper())
    private val writer = Executors.newSingleThreadExecutor()
    private val t0 = SystemClock.elapsedRealtimeNanos()
    private val log = EventLog(t0, MAX_EVENTS)
    private lateinit var outDir: File

    private var params: RunParams? = null
    private var webView: WebView? = null
    private var pipe: BridgePipe? = null
    private var fetch: EarlyFetch? = null
    private var rule: InterceptRule? = null

    // Run state: main thread only.
    private var done = false
    private var pageFinished = false
    private var domProbes = false
    private var firstPaint = false
    private var graceStarted = false
    private var fetchResult: EarlyFetch.Result? = null
    private var outcome: BridgePipe.Outcome? = null
    private var fetchStartMs = 0L
    private var loadCallMs = 0L

    // Written by the interceptor thread.
    private val interceptReason = AtomicReference<String?>()
    private val subresourcePasses = AtomicInteger()
    private val mainFrameCalls = AtomicInteger()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        outDir = File(filesDir, "out")
        // One run per launch: a re-created activity does not start a second run.
        if (savedInstanceState != null) {
            finishAndRemoveTask()
            return
        }
        try {
            resetDir(outDir)
            begin()
        } catch (e: IllegalArgumentException) {
            fail("arguments: ${e.message}")
        } catch (e: IOException) {
            fail("output: ${EarlyFetch.describe(e)}")
        } catch (e: RuntimeException) {
            // A defect or a WebView failure: record it in the status file instead of a crash.
            Log.e(TAG, "run failed", e)
            fail("internal: ${EarlyFetch.describe(e)}")
        }
    }

    private fun begin() {
        val p = RunParams.parse(intent)
        params = p
        require(WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) { "WEB_MESSAGE_LISTENER is not supported" }
        require(WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) { "DOCUMENT_START_SCRIPT is not supported" }
        val sessionUrl = p.url.toString()
        rule = InterceptRule(sessionUrl, InterceptRule.origin(p.url.scheme, p.url.host, p.url.port), RunParams.HOST_ORIGINS)
        val bridge = BridgePipe(
            PIPE_CAPACITY,
            PIPE_STALL_MS,
            object : BridgePipe.Listener {
                override fun onRead(bytes: Int) {
                    log.emit("app.stream.read", mapOf("bytes" to bytes))
                }

                override fun onOutcome(outcome: BridgePipe.Outcome) {
                    main.post { onPipeOutcome(outcome) }
                }
            },
        )
        pipe = bridge

        // 1. Early fetch, before the WebView exists (CAP-A-02).
        val ef = EarlyFetch(client, p.url, log, bridge) { r -> main.post { onFetchEnd(r) } }
        fetch = ef
        fetchStartMs = log.now()
        ef.start()

        // 2. The WebView.
        val wv = WebView(this)
        log.metric("metric.webview_create", log.now() - fetchStartMs)
        configure(wv)
        ProbeBridge(RunParams.HOST_ORIGINS, log) { name, value -> main.post { onProbe(name, value) } }.install(wv)
        wv.webViewClient = Client(sessionUrl)
        setContentView(wv)
        webView = wv
        main.postDelayed({ timeout() }, RUN_TIMEOUT_MS)
        loadCallMs = log.now()
        log.metric("metric.load_call", loadCallMs - fetchStartMs)
        wv.loadUrl(sessionUrl)
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun configure(wv: WebView) {
        val s = wv.settings
        // The spike-host pages post their probes from page scripts; only the lab origins get the port.
        s.javaScriptEnabled = true
        s.allowFileAccess = false
        s.allowContentAccess = false
        s.domStorageEnabled = false
        s.cacheMode = WebSettings.LOAD_NO_CACHE
    }

    private inner class Client(private val sessionUrl: String) : WebViewClient() {
        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
            val r = rule ?: return null
            val p = pipe ?: return null
            val f = fetch ?: return null
            if (!request.isForMainFrame) {
                subresourcePasses.incrementAndGet()
                return null
            }
            mainFrameCalls.incrementAndGet()
            val url = request.url.toString()
            log.metric("metric.intercept", log.now() - loadCallMs)
            val verdict = r.decide(request.method, url, true, request.isRedirect)
            if (verdict.decision == InterceptRule.Decision.PASS) {
                return pass(url, request.method, verdict.reason, p, f)
            }
            val waitStart = log.now()
            val head = f.awaitHead(HEAD_WAIT_MS)
            log.metric("metric.head_wait", log.now() - waitStart)
            if (head == null) return pass(url, request.method, "no-head", p, f)
            if (head.redirected) return pass(url, request.method, "redirected", p, f)
            val parts = InterceptRule.head(head.status, head.reason, head.pairs)
                ?: return pass(url, request.method, "head-not-expressible", p, f)
            interceptReason.set(verdict.reason)
            logIntercept(url, request.method, "stream", parts.status, parts.fields.entries.map { listOf(it.key, InterceptRule.loggable(it.key, it.value)) })
            if (parts.dropped.isNotEmpty()) log.emit("app.error", mapOf("error" to "fields not passed: ${parts.dropped}"))
            return WebResourceResponse(parts.mimeType, parts.encoding, parts.status, parts.reason, parts.fields, p.source())
        }

        /** No interception. The unclaimed early fetch cannot serve this run any more: release it. */
        private fun pass(url: String, method: String, reason: String, p: BridgePipe, f: EarlyFetch): WebResourceResponse? {
            interceptReason.compareAndSet(null, reason)
            logIntercept(url, method, "pass", null, emptyList())
            if (reason != "claimed") {
                p.source().close()
                f.cancel()
            }
            return null
        }

        private fun logIntercept(url: String, method: String, decision: String, status: Int?, fields: List<List<String>>) {
            log.emit(
                "app.intercept",
                mapOf("url" to url, "method" to method, "main_frame" to true, "decision" to decision, "status" to status, "fields" to fields),
            )
        }

        override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
            nav("start", url, null)
        }

        override fun onPageCommitVisible(view: WebView, url: String) {
            // Timing only: the commit races the chunk probes, so it is a metric and not an app.nav event.
            log.metric("metric.commit", log.now() - loadCallMs)
        }

        override fun onPageFinished(view: WebView, url: String) {
            nav("finish", url, null)
            if (url == sessionUrl) {
                log.metric("metric.finish", log.now() - loadCallMs)
                pageFinished = true
                maybeDone()
            }
        }

        override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
            if (request.isForMainFrame) nav("fail", request.url.toString(), "${error.errorCode} ${error.description}")
        }

        override fun onReceivedSslError(view: WebView, handler: SslErrorHandler, error: SslError) {
            // A-03: always cancel; never proceed.
            handler.cancel()
            log.emit("app.ssl_error", mapOf("url" to error.url, "primary_error" to error.primaryError, "decision" to "cancel"))
        }

        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
            log.emit("app.error", mapOf("error" to "render process gone (crash ${detail.didCrash()})"))
            if (webView === view) {
                webView = null
                view.destroy()
            }
            complete("render process gone")
            return true
        }
    }

    private fun nav(kind: String, url: String, error: String?) {
        log.emit("app.nav", mapOf("kind" to kind, "url" to url, "error" to error))
    }

    private fun onProbe(name: String, value: Any?) {
        when (name) {
            "csp.inline" -> domProbes = true
            "metric.first_paint" -> firstPaint = true
            "chunk" -> if (value is Int && value in 0 until MAX_VISUAL_CHUNKS) visualState(value)
        }
        maybeDone()
    }

    /**
     * Render evidence for chunk [k] (CAP-A-01): `postVisualStateCallback` completes when the DOM
     * state at the time of the call (chunk k parsed, its script ran) is ready for the next draw.
     * Timing only, so a metric (ms from the load call), compared with `metric.fetch_end`.
     */
    private fun visualState(k: Int) {
        val wv = webView ?: return
        wv.postVisualStateCallback(
            k.toLong(),
            object : WebView.VisualStateCallback() {
                override fun onComplete(requestId: Long) {
                    log.metric("metric.visual_chunk_$requestId", log.now() - loadCallMs)
                }
            },
        )
    }

    private fun onFetchEnd(r: EarlyFetch.Result) {
        fetchResult = r
        log.metric("metric.fetch_end", r.t - loadCallMs)
        maybeDone()
    }

    private fun onPipeOutcome(o: BridgePipe.Outcome) {
        outcome = o
        if (o.kind == BridgePipe.Kind.CANCEL) fetch?.cancel()
        maybeDone()
    }

    private fun maybeDone() {
        if (done || !pageFinished || !domProbes || fetchResult == null || outcome == null) return
        if (firstPaint) {
            complete(null)
        } else if (!graceStarted) {
            graceStarted = true
            main.postDelayed({ complete(null) }, FIRST_PAINT_GRACE_MS)
        }
    }

    private fun timeout() {
        if (done) return
        val missing = listOfNotNull(
            "page finish".takeIf { !pageFinished },
            "DOMContentLoaded probes".takeIf { !domProbes },
            "fetch end".takeIf { fetchResult == null },
            "stream outcome".takeIf { outcome == null },
        )
        log.emit("app.error", mapOf("error" to "run timeout after $RUN_TIMEOUT_MS ms; missing: ${missing.joinToString()}"))
        complete("timeout; missing: ${missing.joinToString()}")
    }

    /** Ends the run once: writes the output files off the main thread, then finishes. */
    private fun complete(error: String?) {
        if (done) return
        done = true
        main.removeCallbacksAndMessages(null)
        fetchResult?.let { r -> fetch?.writeEnd(r) }
        val lines = log.lines()
        val metrics = metricsJson()
        val device = deviceJson()
        val status = mapOf("state" to if (error == null && log.dropped == 0) "done" else "failed", "error" to (error ?: if (log.dropped > 0) "${log.dropped} events dropped" else null))
        writer.execute {
            try {
                writeText(File(outDir, "app.jsonl"), lines.joinToString("") { it + "\n" })
                writeText(File(outDir, "metrics.json"), Json.encode(metrics) + "\n")
                writeText(File(outDir, "device.json"), Json.encode(device) + "\n")
                writeText(File(outDir, "status.json"), Json.encode(status) + "\n")
                Log.i(TAG, "run ${params?.run} ${status["state"]}")
            } catch (e: IOException) {
                // The run script sees no status file and stops at its own time limit.
                Log.e(TAG, "cannot write the output", e)
            }
            main.post { finishAndRemoveTask() }
        }
    }

    private fun fail(reason: String) {
        log.emit("app.error", mapOf("error" to reason))
        complete(reason)
    }

    private fun metricsJson(): Map<String, Any?> {
        val o = outcome
        val f = fetchResult
        return mapOf(
            "run" to params?.run,
            "metrics" to log.metrics(),
            "fetch" to f?.let { mapOf("bytes" to it.bytes, "sha256" to it.sha256, "complete" to it.complete, "error" to it.error) },
            "stream" to o?.let {
                mapOf(
                    "outcome" to it.kind.name.lowercase(),
                    "bytes" to it.bytes,
                    "sha256" to EarlyFetch.hex(it.sha256),
                    "error" to it.error?.let { e -> EarlyFetch.describe(e) },
                )
            },
            "intercept" to mapOf(
                "main_frame_calls" to mainFrameCalls.get(),
                "reason" to interceptReason.get(),
                "subresource_passes" to subresourcePasses.get(),
            ),
            "dropped_events" to log.dropped,
        )
    }

    private fun deviceJson(): Map<String, Any?> {
        val pkg = WebViewCompat.getCurrentWebViewPackage(this)
        return mapOf(
            "sdk_int" to Build.VERSION.SDK_INT,
            "release" to Build.VERSION.RELEASE,
            "model" to Build.MODEL,
            "fingerprint" to Build.FINGERPRINT,
            "webview_package" to pkg?.packageName,
            "webview_version" to pkg?.versionName,
            "user_agent" to WebSettings.getDefaultUserAgent(this),
            "features" to mapOf(
                "WEB_MESSAGE_LISTENER" to WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER),
                "DOCUMENT_START_SCRIPT" to WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT),
            ),
        )
    }

    override fun onDestroy() {
        main.removeCallbacksAndMessages(null)
        // Release the stream and the network call if the run did not end them.
        pipe?.source()?.close()
        fetch?.cancel()
        webView?.destroy()
        webView = null
        writer.shutdown()
        super.onDestroy()
    }

    private companion object {
        const val TAG = "RivqenSpike"
        const val MAX_EVENTS = 20_000
        const val PIPE_CAPACITY = 64 * 1024
        const val PIPE_STALL_MS = 60_000L
        const val HEAD_WAIT_MS = 30_000L
        const val RUN_TIMEOUT_MS = 150_000L
        const val FIRST_PAINT_GRACE_MS = 2_000L
        const val MAX_VISUAL_CHUNKS = 64

        /** No redirects (S1: a 3xx passes), no silent retry (a second host request would hide in the log), no cache. */
        val client: OkHttpClient = OkHttpClient.Builder()
            .followRedirects(false)
            .followSslRedirects(false)
            .retryOnConnectionFailure(false)
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .callTimeout(180, TimeUnit.SECONDS)
            .build()

        fun resetDir(dir: File) {
            if (dir.exists() && !dir.deleteRecursively()) throw IOException("cannot delete ${dir.name}")
            if (!dir.mkdirs()) throw IOException("cannot create ${dir.name}")
        }

        /** Writes to a temporary file, then renames, so the run script never reads half a file. */
        fun writeText(file: File, text: String) {
            val tmp = File(file.parentFile, file.name + ".tmp")
            tmp.writeText(text, Charsets.UTF_8)
            if (!tmp.renameTo(file)) throw IOException("cannot rename ${tmp.name}")
        }
    }
}
