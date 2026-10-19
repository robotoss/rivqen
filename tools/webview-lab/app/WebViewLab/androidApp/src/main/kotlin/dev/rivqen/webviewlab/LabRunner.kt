// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.webviewlab

import android.annotation.SuppressLint
import android.app.Activity
import android.net.Uri
import android.view.ViewGroup
import android.webkit.ConsoleMessage
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.webkit.JavaScriptExecutionException
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import androidx.webkit.WebViewOutcomeReceiver
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import java.security.SecureRandom
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * What the run saw for one case and view, next to the result file. It goes into `run.json`.
 *
 * - [channel]: how the result left the lab world: `proxy` (view on: `JavaScriptReplyProxy.executeJavaScript`
 *   after the load), `console` (view off: the console chunks of the lab-world script at the `load` event), or `none`.
 * - [bridge]: the lab-world script found the androidx.webkit message object (null: no payload).
 * - [consoleCheck]: view on only: the `load`-event tree of the console channel against the
 *   tree read through the proxy after `onPageFinished` plus the settle time (`same-tree`,
 *   `different-tree`, `missing`).
 * - [mainWorld]: the plain `WebView.evaluateJavascript` check. View off: `not-run` (the callback
 *   gave null) or `ran`. View on: `same-tree` or `different-tree` against the result tree, or `not-run`.
 */
internal data class RowNote(
    val id: String,
    val view: View,
    val walker: LabResult.Walker,
    val channel: String,
    val bridge: Boolean?,
    val consoleCheck: String,
    val mainWorld: String,
    val blocked: Int,
    val blockedOrigins: List<String>,
    val loadProblem: String?,
)

internal data class Row(val result: LabResult, val note: RowNote)

/**
 * Runs one case in one scripting view in a new WebView (CONTRACT section 4).
 *
 * `serialize.js` runs only in an isolated JavaScript world of androidx.webkit
 * (`WebViewCompat.getExecutionWorld`), so no page script shares its context. View off sets
 * `WebSettings.setJavaScriptEnabled(false)` and nothing turns it on. All WebView calls run on
 * the main thread.
 */
internal class LabRunner(
    private val activity: Activity,
    private val container: FrameLayout,
    private val plan: LabPlan,
    private val port: Int,
    private val settleMs: Long,
) {
    init {
        require(LabNet.isValidPort(port)) { "bad port" }
        require(settleMs in 0..MAX_SETTLE_MS) { "bad settle time" }
    }

    private val random = SecureRandom()

    /** True when the installed WebView supports the lab world. */
    val worldSupported: Boolean =
        WebViewFeature.isFeatureSupported(WebViewFeature.JS_INJECTION_IN_FRAME_AND_WORLD) &&
            WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)

    suspend fun run(c: LabCase, view: View): Row {
        val url = LabNet.caseUrl(port, c.id)
        val page = PageState(url, ConsoleCollector(newNonce()))
        val ready = CompletableDeferred<JavaScriptReplyProxy>()
        val wv = WebView(activity)
        container.addView(wv, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        try {
            configure(wv.settings, view)
            if (!worldSupported || !installWorld(wv, ready, LabJson.worldScript(page.nonce, plan.walker, c.probe))) {
                val reason = "the WebView does not support androidx.webkit JS_INJECTION_IN_FRAME_AND_WORLD"
                return row(LabResult.unavailable(c.id, view, reason), "none", page, "skipped", "skipped", null)
            }
            wv.webViewClient = LabClient(page, ::isLab)
            wv.webChromeClient = LabChrome(page)
            wv.loadUrl(url)
            val finished = withTimeoutOrNull(PAGE_TIMEOUT_MS) { page.finished.await() }
            val problem = when {
                finished == null -> "the page did not finish loading in $PAGE_TIMEOUT_MS ms"
                page.rendererGone -> "the renderer process ended"
                else -> page.loadError
            }
            if (problem != null) {
                return row(LabResult.unavailable(c.id, view, "load: $problem"), "none", page, "skipped", "skipped", problem)
            }
            delay(settleMs)
            // The lab-world script runs at the load event, before onPageFinished; give its console chunks a short time.
            withTimeoutOrNull(CHANNEL_TIMEOUT_MS) { page.consoleDone.await() }
            val payload = decodePayload(page.console)
            return if (view == View.ON) readOn(wv, c, page, ready, payload) else readOff(wv, c, page, payload)
        } finally {
            container.removeView(wv)
            wv.stopLoading()
            wv.destroy()
        }
    }

    /** View on: the tree after the load, through the reply proxy of the ready message. */
    private suspend fun readOn(
        wv: WebView,
        c: LabCase,
        page: PageState,
        ready: CompletableDeferred<JavaScriptReplyProxy>,
        payload: Payload,
    ): Row {
        val proxy = withTimeoutOrNull(CHANNEL_TIMEOUT_MS) { ready.await() }
        val result = if (proxy == null) {
            LabResult.unavailable(c.id, View.ON, "the lab world sent no ready message in view on")
        } else {
            readThroughProxy(proxy, c)
        }
        val consoleCheck = when {
            payload.value?.tree == null -> "missing" + (payload.problem?.let { ": $it" } ?: "")
            payload.value.tree == result.tree -> "same-tree"
            else -> "different-tree"
        }
        return row(result, if (proxy == null) "none" else "proxy", page, consoleCheck, mainWorldCheck(wv, View.ON, result.tree), null, payload.value?.bridge)
    }

    /** View off: the tree of the lab-world script at the load event, through the console channel. */
    private suspend fun readOff(wv: WebView, c: LabCase, page: PageState, payload: Payload): Row {
        val p = payload.value
        val result = when {
            p == null -> LabResult.unavailable(
                c.id,
                View.OFF,
                "the lab-world script sent no result in view off: ${payload.problem ?: "no console chunks"}",
            )
            p.tree == null -> LabResult(
                c.id, View.OFF, LabResult.Walker.UNAVAILABLE, null, p.characterSet, p.contentType, p.probe,
                "lab world: ${p.error ?: "no tree"}",
            )
            else -> LabResult(c.id, View.OFF, LabResult.Walker.OK, p.tree, p.characterSet, p.contentType, p.probe, null)
        }
        val channel = if (result.walker == LabResult.Walker.OK) "console" else "none"
        return row(result, channel, page, "n/a", mainWorldCheck(wv, View.OFF, result.tree), null, p?.bridge)
    }

    private class Payload(val value: LabJson.WorldPayload?, val problem: String?)

    private fun decodePayload(collector: ConsoleCollector): Payload {
        collector.error?.let { return Payload(null, "console channel: $it") }
        val text = collector.payload() ?: return Payload(null, null)
        return try {
            Payload(LabJson.decodeWorldPayload(text), null)
        } catch (e: ScriptValueException) {
            Payload(null, e.message)
        }
    }

    private fun row(
        result: LabResult,
        channel: String,
        page: PageState,
        consoleCheck: String,
        mainWorld: String,
        problem: String?,
        bridge: Boolean? = null,
    ): Row = Row(
        result,
        RowNote(
            result.id, result.view, result.walker, channel, bridge, consoleCheck, mainWorld,
            page.blocked.get(), page.blockedOrigins.toList().sorted(), problem,
        ),
    )

    /**
     * Runs the meta script and then `serialize.js` in the lab world, after the load. The page
     * facts stay in the result when the walker fails.
     */
    private suspend fun readThroughProxy(proxy: JavaScriptReplyProxy, c: LabCase): LabResult {
        val meta = try {
            LabJson.decodeMeta(execute(proxy, LabJson.metaScript(c.probe)))
        } catch (e: JavaScriptExecutionException) {
            return LabResult.unavailable(c.id, View.ON, "lab-world script failed (type ${e.errorType}): ${e.message}")
        } catch (e: ScriptValueException) {
            return LabResult.unavailable(c.id, View.ON, "page facts: ${e.message}")
        } catch (e: ScriptTimeoutException) {
            return LabResult.unavailable(c.id, View.ON, e.message ?: "lab-world script timeout")
        }
        val failure = try {
            val tree = LabJson.decodeGuarded(execute(proxy, LabJson.guardedWalker(plan.walker)))
            return LabResult(c.id, View.ON, LabResult.Walker.OK, tree, meta.characterSet, meta.contentType, meta.probe, null)
        } catch (e: JavaScriptExecutionException) {
            "lab-world script failed (type ${e.errorType}): ${e.message}"
        } catch (e: ScriptValueException) {
            e.message ?: "walker value"
        } catch (e: ScriptTimeoutException) {
            e.message ?: "lab-world script timeout"
        }
        return LabResult(
            c.id, View.ON, LabResult.Walker.UNAVAILABLE, null, meta.characterSet, meta.contentType, meta.probe, failure,
        )
    }

    /**
     * The FACT check of the plain API. View off: does `evaluateJavascript` run at all? View on:
     * does the page world give the same tree as the lab world?
     */
    private suspend fun mainWorldCheck(wv: WebView, view: View, tree: String?): String {
        val script = if (view == View.OFF) "'rivqen-main-world'" else LabJson.guardedWalker(plan.walker)
        // The holder keeps "the callback gave null" apart from "no callback before the time limit".
        val raw = withTimeoutOrNull(SCRIPT_TIMEOUT_MS) {
            suspendCancellableCoroutine<Callback> { cont ->
                wv.evaluateJavascript(script) { value: String? -> if (cont.isActive) cont.resume(Callback(value)) }
            }
        } ?: return "no-callback"
        if (view == View.OFF) {
            return try {
                if (LabJson.decodeScriptString(raw.value) == null) "not-run" else "ran"
            } catch (e: ScriptValueException) {
                "error: ${e.message}"
            }
        }
        val text = try {
            LabJson.decodeScriptString(raw.value)
        } catch (e: ScriptValueException) {
            return "error: ${e.message}"
        }
        if (text == null) return "not-run"
        val value = try {
            LabJson.decodeGuarded(raw.value)
        } catch (e: ScriptValueException) {
            return "walker-error: ${e.message}"
        }
        return when {
            tree == null -> "ran"
            value == tree -> "same-tree"
            else -> "different-tree"
        }
    }

    private suspend fun execute(proxy: JavaScriptReplyProxy, script: String): String =
        withTimeoutOrNull(SCRIPT_TIMEOUT_MS) {
            suspendCancellableCoroutine { cont ->
                if (WebViewFeature.isFeatureSupported(WebViewFeature.JS_INJECTION_IN_FRAME_AND_WORLD)) {
                    proxy.executeJavaScript(
                        script,
                        object : WebViewOutcomeReceiver<String, JavaScriptExecutionException> {
                            override fun onResult(result: String) {
                                if (cont.isActive) cont.resume(result)
                            }

                            override fun onError(error: JavaScriptExecutionException) {
                                if (cont.isActive) cont.resumeWithException(error)
                            }
                        },
                    )
                } else {
                    cont.resumeWithException(ScriptValueException("the WebView does not support executeJavaScript"))
                }
            }
        } ?: throw ScriptTimeoutException("lab-world script gave no result in $SCRIPT_TIMEOUT_MS ms")

    /**
     * Adds the lab world: a message listener (view on gets a reply proxy from it) and the
     * script of [LabJson.worldScript], added for DOMContentLoaded.
     */
    private fun installWorld(wv: WebView, ready: CompletableDeferred<JavaScriptReplyProxy>, script: String): Boolean {
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.JS_INJECTION_IN_FRAME_AND_WORLD)) return false
        val world = WebViewCompat.getExecutionWorld(wv, WORLD_NAME)
        val rules = setOf(LabNet.origin(port))
        WebViewCompat.addWebMessageListener(wv, BRIDGE, rules, world) { _, message, origin, isMainFrame, proxy ->
            if (isMainFrame && message.data == READY && isLab(origin)) ready.complete(proxy)
        }
        WebViewCompat.addJavaScriptOnEvent(wv, script, WebViewCompat.INJECTION_EVENT_DOCUMENT_END, rules, world)
        return true
    }

    private fun isLab(u: Uri): Boolean = LabNet.isLabUrl(u.scheme, u.host, u.port, u.userInfo, port)

    private fun newNonce(): String {
        val b = ByteArray(NONCE_BYTES)
        random.nextBytes(b)
        val hex = "0123456789abcdef"
        return buildString { for (x in b) append(hex[x.toInt() shr 4 and 0xF]).append(hex[x.toInt() and 0xF]) }
    }

    private fun configure(s: WebSettings, view: View) {
        // CONTRACT section 4 step 1. Nothing below turns content JavaScript on for view off.
        s.javaScriptEnabled = view == View.ON
        s.javaScriptCanOpenWindowsAutomatically = false
        s.setSupportMultipleWindows(false)
        s.allowFileAccess = false
        s.allowContentAccess = false
        s.domStorageEnabled = false
        s.setGeolocationEnabled(false)
        s.mediaPlaybackRequiresUserGesture = true
        s.cacheMode = WebSettings.LOAD_NO_CACHE
        s.safeBrowsingEnabled = false
    }

    /** Load state of one page. The callbacks run on the main thread, except the request check. */
    private class PageState(val url: String, val console: ConsoleCollector) {
        val nonce: String = console.prefix.removePrefix("rivqen-lab:").removeSuffix(":")
        val finished = CompletableDeferred<Unit>()
        val consoleDone = CompletableDeferred<Unit>()
        var loadError: String? = null
        var rendererGone = false
        val blocked = AtomicInteger()
        val blockedOrigins: MutableSet<String> = ConcurrentHashMap.newKeySet()
    }

    private class LabChrome(private val page: PageState) : WebChromeClient() {
        override fun onConsoleMessage(consoleMessage: ConsoleMessage): Boolean {
            val c = page.console
            if (c.offer(consoleMessage.message()) && (c.complete || c.error != null)) page.consoleDone.complete(Unit)
            // Handled: page console text does not go to the log.
            return true
        }
    }

    // The class overrides onRenderProcessGone and returns true (below). The androidx.webkit
    // detector reports it anyway (checked with androidx.webkit 1.17.1 lint, AGP 9.1.1).
    @SuppressLint("MissingOnRenderProcessGone")
    private class LabClient(private val page: PageState, private val isLab: (Uri) -> Boolean) : WebViewClient() {
        private fun block(u: Uri) {
            page.blocked.incrementAndGet()
            // Only scheme and host are kept, never the path or the query.
            if (page.blockedOrigins.size < MAX_BLOCKED_ORIGINS) page.blockedOrigins.add("${u.scheme}://${u.host}")
        }

        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
            // Runs on a WebView thread. Null lets the lab server answer.
            if (isLab(request.url)) return null
            block(request.url)
            return LabNet.blockedResponse()
        }

        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            if (isLab(request.url)) return false
            block(request.url)
            return true
        }

        override fun onPageFinished(view: WebView, url: String) {
            if (url == page.url) page.finished.complete(Unit)
        }

        override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
            if (request.isForMainFrame) page.loadError = "error ${error.errorCode}: ${error.description}"
        }

        override fun onReceivedHttpError(view: WebView, request: WebResourceRequest, errorResponse: WebResourceResponse) {
            if (request.isForMainFrame) page.loadError = "HTTP ${errorResponse.statusCode}"
        }

        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
            // Handled: the app must not end with the renderer. The WebView is destroyed after the run.
            page.rendererGone = true
            page.finished.complete(Unit)
            return true
        }
    }

    private class ScriptTimeoutException(message: String) : Exception(message)

    private class Callback(val value: String?)

    companion object {
        const val TAG: String = "WebViewLab"
        const val WORLD_NAME: String = "rivqen-lab"
        const val BRIDGE: String = "rivqenLab"
        const val READY: String = "ready"
        const val MAX_SETTLE_MS: Long = 2000
        const val PAGE_TIMEOUT_MS: Long = 20_000
        const val CHANNEL_TIMEOUT_MS: Long = 3_000
        const val SCRIPT_TIMEOUT_MS: Long = 10_000
        const val MAX_BLOCKED_ORIGINS: Int = 32
        private const val NONCE_BYTES = 16
    }
}
