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
import java.io.ByteArrayInputStream
import java.io.File
import java.io.IOException
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/**
 * Prototype A-STREAM (CONTRACT.md, CAP-A-01…05, CAP-A-08). One run per launch:
 *
 * 1. Start the OkHttp early fetch of the page ([EarlyFetch]) — before the WebView exists.
 * 2. Create the WebView, install the probe channel ([ProbeBridge]) and the client, call `loadUrl`.
 * 3. `shouldInterceptRequest` gives the main-frame request the fetched head and the reader side of
 *    the [BridgePipe] when [InterceptRule] allows it (A-04) and the head can serve it
 *    ([InterceptRule.headPass]: no redirect, no 3xx, expressible); else the request passes.
 * 4. The load settles when a document finished and its DOMContentLoaded probes arrived (the page
 *    that finished is the page of the `location` probe), or when the main frame failed
 *    (`onReceivedError` or a cancelled TLS error) and a page finished.
 * 5. [Scenario.CUT_RANGE]: when the delivered stream ended with an error (CL-33) and the load
 *    settled, call `loadUrl` once more: the request passes (`claimed`) and the WebView loads it
 *    itself. When that load settled, a page script fetches byte ranges of `/s/range`.
 * 6. The run ends when the last load settled, the fetch ended and the pipe has its outcome (then a
 *    short wait for `metric.first_paint`), or at [RUN_TIMEOUT_MS].
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

    private var probes: ProbeBridge? = null

    // Run state: main thread only.
    private var done = false
    private var loads = 0
    private var finishedUrl: String? = null
    private var location: String? = null
    private var domProbes = false
    private var sslCancelled = false
    private var mainFrameError = false
    private var errorPageStarted = false
    private var rangeDone = false
    private var cutStep = CutStep.FIRST_LOAD

    /** Steps of [Scenario.CUT_RANGE]. */
    private enum class CutStep { FIRST_LOAD, FIRST_SNAPSHOT, FALLBACK_LOAD, FALLBACK_SNAPSHOT, RANGE_PAGE, RANGE }
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
        // DOCUMENT_START_SCRIPT is optional: without it the probe channel is installed after the load
        // (ProbeBridge.Channel.AFTER_LOAD; WebView 91 of the API 29 image).
        require(WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) { "WEB_MESSAGE_LISTENER is not supported" }
        val sessionUrl = p.url.toString()
        rule = InterceptRule(sessionUrl, InterceptRule.origin(p.url.scheme, p.url.host, p.url.port), RunParams.LOAD_ORIGINS)
        val bridge = BridgePipe(
            PIPE_CAPACITY,
            PIPE_STALL_MS,
            object : BridgePipe.Listener {
                override fun onRead(bytes: Int, requested: Int) {
                    // `want`: the length the WebView asked for (the read block; README "Read blocks").
                    log.emit("app.stream.read", mapOf("bytes" to bytes, "want" to requested))
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
        val bridgeProbes = ProbeBridge(RunParams.HOST_ORIGINS, log) { name, value -> main.post { onProbe(name, value) } }
        bridgeProbes.install(wv)
        probes = bridgeProbes
        wv.webViewClient = Client(sessionUrl, p.scenario, bridgeProbes.channel == ProbeBridge.Channel.AFTER_LOAD)
        setContentView(wv)
        webView = wv
        main.postDelayed({ timeout() }, RUN_TIMEOUT_MS)
        loadCallMs = log.now()
        log.metric("metric.load_call", loadCallMs - fetchStartMs)
        loads = 1
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

    /**
     * The WebView client. [afterLoad]: the probe channel is [ProbeBridge.Channel.AFTER_LOAD], so the
     * page sends no chunk probes; the beacon requests of the chunks then start the visual-state
     * callbacks (metrics only).
     */
    private inner class Client(
        private val sessionUrl: String,
        private val scenario: Scenario,
        private val afterLoad: Boolean,
    ) : WebViewClient() {
        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
            val r = rule ?: return null
            val p = pipe ?: return null
            val f = fetch ?: return null
            if (!request.isForMainFrame) {
                subresourcePasses.incrementAndGet()
                subresource(request)
                return null
            }
            mainFrameCalls.incrementAndGet()
            val url = request.url.toString()
            log.metric("metric.intercept", log.now() - loadCallMs)
            val verdict = r.decide(request.method, url, true, request.isRedirect)
            if (verdict.decision == InterceptRule.Decision.PASS) {
                return pass(url, request, verdict.reason, p, f)
            }
            val waitStart = log.now()
            val head = f.awaitHead(HEAD_WAIT_MS)
            log.metric("metric.head_wait", log.now() - waitStart)
            if (head == null) return pass(url, request, "no-head", p, f)
            val unused = InterceptRule.headPass(head.redirected, head.status, head.reason, head.pairs)
            if (unused == "redirect-status" && scenario == Scenario.REDIRECT_3XX) return redirectProbe(url, request, head, p, f)
            if (unused != null) return pass(url, request, unused, p, f)
            val parts = InterceptRule.head(head.status, head.reason, head.pairs)
                ?: return pass(url, request, "head-not-expressible", p, f)
            interceptReason.set(verdict.reason)
            logIntercept(url, request, "stream", parts.status, parts.fields.entries.map { listOf(it.key, InterceptRule.loggable(it.key, it.value)) })
            if (parts.dropped.isNotEmpty()) log.emit("app.error", mapOf("error" to "fields not passed: ${parts.dropped}"))
            return WebResourceResponse(parts.mimeType, parts.encoding, parts.status, parts.reason, parts.fields, p.source())
        }

        /**
         * Subresources always pass. Two lab observations: the `Range` field that the WebView gives the
         * interceptor for a `/s/range` request (CAP-A-08, probe `native.range_request`), and, with the
         * after-load channel, a visual-state callback for each chunk beacon.
         */
        private fun subresource(request: WebResourceRequest) {
            val url = request.url.toString()
            val origin = InterceptRule.origin(request.url.scheme ?: "", request.url.host ?: "", request.url.port)
            if (url.contains(RANGE_PATH) && origin in RunParams.HOST_ORIGINS) {
                val range = request.requestHeaders.entries.firstOrNull { it.key.equals("Range", ignoreCase = true) }?.value
                log.emit("app.probe", mapOf("name" to "native.range_request", "value" to range))
            }
            if (afterLoad) {
                val k = InterceptRule.beaconIndex(url, RunParams.HOST_ORIGINS)
                if (k in 0 until MAX_VISUAL_CHUNKS) main.post { visualState(k, "beacon") }
            }
        }

        /**
         * CAP-A-04 lab probe (never part of the prototype rule): gives the WebView the fetched 3xx
         * head as a `WebResourceResponse`, which the reference calls not supported. When the
         * constructor refuses it, the request passes.
         */
        private fun redirectProbe(url: String, request: WebResourceRequest, head: EarlyFetch.Head, p: BridgePipe, f: EarlyFetch): WebResourceResponse? {
            val fields = InterceptRule.rawFields(head.pairs)
            val type = fields.entries.firstOrNull { it.key.equals("Content-Type", ignoreCase = true) }?.value
            val media: Array<String?> = InterceptRule.mediaType(type) ?: arrayOf("text/plain", null)
            val response = try {
                WebResourceResponse(media[0], media[1], head.status, head.reason, fields, ByteArrayInputStream(ByteArray(0)))
            } catch (e: IllegalArgumentException) {
                log.emit("app.error", mapOf("error" to "WebResourceResponse refused status ${head.status}: ${EarlyFetch.describe(e)}"))
                return pass(url, request, "redirect-status", p, f)
            }
            interceptReason.set("redirect-probe")
            logIntercept(url, request, "stream", head.status, fields.entries.map { listOf(it.key, InterceptRule.loggable(it.key, it.value)) })
            p.source().close()
            f.cancel()
            return response
        }

        /** No interception. The unclaimed early fetch cannot serve this run any more: release it. */
        private fun pass(url: String, request: WebResourceRequest, reason: String, p: BridgePipe, f: EarlyFetch): WebResourceResponse? {
            interceptReason.compareAndSet(null, reason)
            logIntercept(url, request, "pass", null, emptyList(), reason)
            if (reason != "claimed") {
                p.source().close()
                f.cancel()
            }
            return null
        }

        private fun logIntercept(
            url: String,
            request: WebResourceRequest,
            decision: String,
            status: Int?,
            fields: List<List<String>>,
            reason: String? = null,
        ) {
            log.emit(
                "app.intercept",
                mapOf(
                    "url" to url, "method" to request.method, "main_frame" to true, "redirect" to request.isRedirect,
                    "decision" to decision, "reason" to reason, "status" to status, "fields" to fields,
                ),
            )
        }

        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            // Never overrides; records the main-frame redirects that the WebView follows (CAP-A-04).
            if (request.isForMainFrame && request.isRedirect) nav("redirect", request.url.toString(), null)
            return false
        }

        override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
            nav("start", url, null)
            // After onReceivedError and onPageFinished of a failed main frame, the WebView commits
            // its error page (one more onPageStarted, API 29 and 37). Only then the load settled.
            if (mainFrameError && finishedUrl != null) {
                errorPageStarted = true
                maybeDone()
            }
        }

        override fun onPageCommitVisible(view: WebView, url: String) {
            // Timing only: the commit races the chunk probes, so it is a metric and not an app.nav event.
            log.metric("metric.commit", log.now() - loadCallMs)
        }

        override fun onPageFinished(view: WebView, url: String) {
            nav("finish", url, null)
            if (url == sessionUrl) log.metric("metric.finish", log.now() - loadCallMs)
            finishedUrl = url
            probes?.installAfterLoad(view, url)
            maybeDone()
        }

        override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
            if (request.isForMainFrame) {
                nav("fail", request.url.toString(), "${error.errorCode} ${error.description}")
                mainFrameError = true
                maybeDone()
            }
        }

        override fun onReceivedHttpError(view: WebView, request: WebResourceRequest, errorResponse: WebResourceResponse) {
            // The status that the WebView took for the main document (also for an intercepted response).
            if (request.isForMainFrame) nav("fail", request.url.toString(), "http ${errorResponse.statusCode}")
        }

        override fun onReceivedSslError(view: WebView, handler: SslErrorHandler, error: SslError) {
            // A-03: always cancel; never proceed.
            handler.cancel()
            log.emit("app.ssl_error", mapOf("url" to error.url, "primary_error" to error.primaryError, "decision" to "cancel"))
            sslCancelled = true
            maybeDone()
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
            "location" -> location = value as? String
            "csp.inline" -> domProbes = true
            "range.done" -> rangeDone = true
            "metric.first_paint" -> firstPaint = true
            "chunk" -> if (value is Int && value in 0 until MAX_VISUAL_CHUNKS) visualState(value, "chunk")
        }
        maybeDone()
    }

    /**
     * Render evidence for chunk [k] (CAP-A-01): `postVisualStateCallback` completes when the DOM
     * state at the time of the call (chunk k parsed, its script ran) is ready for the next draw.
     * Timing only, so a metric (ms from the load call), compared with `metric.fetch_end`.
     */
    private fun visualState(k: Int, trigger: String) {
        val wv = webView ?: return
        wv.postVisualStateCallback(
            k.toLong(),
            object : WebView.VisualStateCallback() {
                override fun onComplete(requestId: Long) {
                    log.metric("metric.visual_${trigger}_$requestId", log.now() - loadCallMs)
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

    /**
     * The current load settled: a document finished and its DOMContentLoaded probes arrived (the
     * finished page is the page of the `location` probe); or a TLS error was cancelled and a page
     * finished (no error-page commit follows, API 29 and 37); or the main frame failed
     * (`onReceivedError`), a page finished and the error page started.
     */
    private fun settled(): Boolean {
        val finished = finishedUrl ?: return false
        return (domProbes && location == finished) || sslCancelled || (mainFrameError && errorPageStarted)
    }

    /** Clears the state of the current load before the next `loadUrl`. */
    private fun nextLoad() {
        loads += 1
        finishedUrl = null
        location = null
        domProbes = false
        sslCancelled = false
        mainFrameError = false
        errorPageStarted = false
    }

    private fun maybeDone() {
        if (done || !settled() || fetchResult == null || outcome == null) return
        if (params?.scenario == Scenario.CUT_RANGE && !cutRangeSteps()) return
        if (firstPaint) {
            complete(null)
        } else if (!graceStarted) {
            graceStarted = true
            main.postDelayed({ complete(null) }, FIRST_PAINT_GRACE_MS)
        }
    }

    /**
     * The steps of [Scenario.CUT_RANGE] after a settled load; true when all steps are done.
     * Step 1 (CL-33): the delivered stream ended with an error, so `loadUrl` runs once more; the
     * rule passes that request (`claimed`) and the WebView loads it itself. Each of the two loads
     * ends with `native.doc`. Step 2: a normal page of the spike host (`/s/echo?html=1`; the rule
     * passes it, `url`), because an error page has no probe channel. Step 3: on that page,
     * [RANGE_SCRIPT] runs; it ends with the probe `range.done`.
     */
    private fun cutRangeSteps(): Boolean {
        val wv = webView ?: return true
        val p = params ?: return true
        when (cutStep) {
            CutStep.FIRST_LOAD -> {
                if (outcome?.kind != BridgePipe.Kind.ERROR) return true
                cutStep = CutStep.FIRST_SNAPSHOT
                snapshot(wv) {
                    log.emit("app.probe", mapOf("name" to "native.step", "value" to "fallback-load"))
                    cutStep = CutStep.FALLBACK_LOAD
                    nextLoad()
                    wv.loadUrl(p.url.toString())
                }
                return false
            }
            CutStep.FALLBACK_LOAD -> {
                cutStep = CutStep.FALLBACK_SNAPSHOT
                snapshot(wv) {
                    log.emit("app.probe", mapOf("name" to "native.step", "value" to "range-page"))
                    cutStep = CutStep.RANGE_PAGE
                    nextLoad()
                    val page = p.url.newBuilder().encodedPath("/s/echo").query(null)
                        .addQueryParameter("html", "1").addQueryParameter("run", p.run).build()
                    wv.loadUrl(page.toString())
                }
                return false
            }
            CutStep.RANGE_PAGE -> {
                cutStep = CutStep.RANGE
                log.emit("app.probe", mapOf("name" to "native.step", "value" to "range-fetch"))
                wv.evaluateJavascript(RANGE_SCRIPT) { result ->
                    if (result != "\"started\"") {
                        log.emit("app.error", mapOf("error" to "range script did not start: ${result?.take(MAX_SCRIPT_RESULT)}"))
                        rangeDone = true
                        maybeDone()
                    }
                }
                return false
            }
            CutStep.FIRST_SNAPSHOT, CutStep.FALLBACK_SNAPSHOT -> return false
            CutStep.RANGE -> return rangeDone
        }
    }

    /**
     * What the page has now (CAP-A-08): `native.doc` with the URL, title, ready state and the length
     * of the body text, from `evaluateJavascript` (it needs no page channel and also works on an
     * error page). Then [next] on the main thread.
     */
    private fun snapshot(wv: WebView, next: () -> Unit) {
        wv.evaluateJavascript(DOC_SCRIPT) { result ->
            log.emit("app.probe", mapOf("name" to "native.doc", "value" to result?.take(MAX_SCRIPT_RESULT)))
            next()
        }
    }

    private fun timeout() {
        if (done) return
        val missing = listOfNotNull(
            "page finish".takeIf { finishedUrl == null },
            "DOMContentLoaded probes".takeIf { !domProbes && !sslCancelled && !mainFrameError },
            "error page start".takeIf { mainFrameError && !errorPageStarted },
            "fetch end".takeIf { fetchResult == null },
            "stream outcome".takeIf { outcome == null },
            "range probes".takeIf { params?.scenario == Scenario.CUT_RANGE && !rangeDone },
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
            "scenario" to params?.scenario?.extra,
            "channel" to probes?.channel?.value,
            "loads" to loads,
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
        const val MAX_SCRIPT_RESULT = 200
        const val RANGE_PATH = "/s/range?"

        /** The page state for `native.doc` (a JSON string; [MAX_SCRIPT_RESULT] characters at most). */
        val DOC_SCRIPT = """
            (function () {
              var b = document.body;
              return JSON.stringify({ href: String(location.href), title: document.title, ready: document.readyState,
                text: b ? b.textContent.length : null, rqlab: !!window.rqLab });
            })();
        """.trimIndent()

        /**
         * CAP-A-08: two byte-range fetches of `/s/range` (4096 bytes; byte i is (i * 31 + 7) mod 251)
         * from the page: `bytes=100-1123` (a 206 with 1024 matching bytes is expected) and
         * `bytes=5000-` (a 416 is expected). The probes go out at the end, after both requests, so
         * the `native.range_request` events of the interceptor come first in every run. Returns
         * "started", or "no-channel" when the page has no `rqLab`.
         */
        val RANGE_SCRIPT = """
            (function () {
              if (!window.rqLab) { return 'no-channel'; }
              var run = new URLSearchParams(location.search).get('run') || '';
              var url = '/s/range?run=' + encodeURIComponent(run) + '&size=4096';
              var out = [];
              function same(b, from) {
                for (var i = 0; i < b.length; i++) { if (b[i] !== ((from + i) * 31 + 7) % 251) { return false; } }
                return true;
              }
              fetch(url, { headers: { Range: 'bytes=100-1123' }, cache: 'no-store' })
                .then(function (r) {
                  return r.arrayBuffer().then(function (buf) {
                    var b = new Uint8Array(buf);
                    out.push(['range.status', r.status], ['range.content_range', r.headers.get('Content-Range')],
                      ['range.bytes', b.length], ['range.match', same(b, 100)]);
                  });
                })
                .then(function () { return fetch(url, { headers: { Range: 'bytes=5000-' }, cache: 'no-store' }); })
                .then(function (r) { out.push(['range.bad_status', r.status]); })
                .catch(function (e) { out.push(['range.error', String(e && e.name)]); })
                .then(function () {
                  out.forEach(function (p) { window.rqLab.probe(p[0], p[1]); });
                  window.rqLab.probe('range.done', true);
                });
              return 'started';
            })();
        """.trimIndent()

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
