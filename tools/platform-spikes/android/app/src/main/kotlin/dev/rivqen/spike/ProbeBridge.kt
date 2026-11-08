// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spike

import android.net.Uri
import android.webkit.WebView
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/**
 * The probe channel of CONTRACT.md section 6. `WebViewCompat.addWebMessageListener` injects a port
 * object only into frames of the [origins] (A-06; no `addJavascriptInterface`). A document-start
 * script (`addDocumentStartJavaScript`, same origins) puts `window.rqLab.probe(name, value)` on top
 * of it and adds the page probes of the prototype (`location`, `origin`, `status`, `csp.inline` at
 * DOMContentLoaded; `metric.first_paint`).
 *
 * The page scripts of the spike host call `window.rqLab.probe` from the page world, so the object
 * must be in the page world; an isolated world would hide it from them. The origin rule is the
 * boundary. Messages are untrusted: [onPostMessage] checks the frame, the origin, the size and the
 * shape and drops the rest with an `app.error`.
 */
internal class ProbeBridge(
    private val origins: Set<String>,
    private val log: EventLog,
    private val onProbe: (name: String, value: Any?) -> Unit,
) : WebViewCompat.WebMessageListener {
    private var count = 0

    /** How the page gets `window.rqLab`. */
    enum class Channel(val value: String) {
        /** `addDocumentStartJavaScript`: before every page script (the S1 channel). */
        DOCUMENT_START("document-start"),

        /**
         * The WebView has no DOCUMENT_START_SCRIPT (WebView 91 of the API 29 image): [installAfterLoad]
         * runs [SCRIPT] with `evaluateJavascript` after `onPageFinished`. Page scripts that ran before
         * (the chunk scripts) find no `rqLab` and send no probe; the DOMContentLoaded probes run at once.
         */
        AFTER_LOAD("after-load"),
    }

    /** The channel of this WebView; set by [install]. */
    var channel: Channel = Channel.DOCUMENT_START
        private set

    /**
     * Call on the main thread before `loadUrl`.
     *
     * @throws IllegalStateException when WEB_MESSAGE_LISTENER is missing (no channel at all)
     */
    fun install(webView: WebView) {
        if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            WebViewCompat.addWebMessageListener(webView, PORT_NAME, origins, this)
        } else {
            throw IllegalStateException("WEB_MESSAGE_LISTENER is not supported")
        }
        if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            WebViewCompat.addDocumentStartJavaScript(webView, SCRIPT, origins)
            channel = Channel.DOCUMENT_START
        } else {
            channel = Channel.AFTER_LOAD
        }
    }

    /**
     * The [Channel.AFTER_LOAD] install: runs [SCRIPT] in the page when [url] (the URL of the page
     * that finished) is on an allowed origin. `evaluateJavascript` has no origin rule of its own, so
     * this check is the boundary; the script also stops when the page has no `rqLabPort` (the
     * listener injects it only into the allowed origins). Main thread only.
     *
     * @return true when the script was started
     */
    fun installAfterLoad(webView: WebView, url: String): Boolean {
        if (channel != Channel.AFTER_LOAD) return false
        val u = Uri.parse(url)
        val scheme = u.scheme ?: return false
        val port = if (u.port != -1) u.port else if (scheme == "https") HTTPS_PORT else HTTP_PORT
        if (InterceptRule.origin(scheme, u.host ?: "", port) !in origins) return false
        webView.evaluateJavascript(SCRIPT) { result ->
            // "installed" sends the page probes; any other result means no probes from this page, so
            // the result is logged (no race with page probes then).
            if (result != "\"installed\"") log.emit("app.probe", mapOf("name" to "native.after_load", "value" to result?.take(MAX_VALUE)))
        }
        return true
    }

    override fun onPostMessage(
        view: WebView,
        message: WebMessageCompat,
        sourceOrigin: Uri,
        isMainFrame: Boolean,
        replyProxy: JavaScriptReplyProxy,
    ) {
        val scheme = sourceOrigin.scheme ?: ""
        val port = if (sourceOrigin.port != -1) sourceOrigin.port else if (scheme == "https") HTTPS_PORT else HTTP_PORT
        val origin = InterceptRule.origin(scheme, sourceOrigin.host ?: "", port)
        if (!isMainFrame || origin !in origins) {
            reject("probe from a frame that is not the main frame of an allowed origin")
            return
        }
        if (++count > MAX_PROBES) {
            if (count == MAX_PROBES + 1) reject("more than $MAX_PROBES probes")
            return
        }
        val data = if (message.type == WebMessageCompat.TYPE_STRING) message.data else null
        if (data == null || data.length > MAX_MESSAGE) {
            reject("probe is not a string of at most $MAX_MESSAGE characters")
            return
        }
        val o = try {
            JSONObject(data)
        } catch (e: JSONException) {
            reject("probe is not JSON: ${e.javaClass.name}")
            return
        }
        val name = o.optString("name", "")
        if (!NAME.matches(name)) {
            reject("probe name does not match ${NAME.pattern}")
            return
        }
        val raw: Any? = o.opt("value")
        val problem = when {
            raw == null || raw == JSONObject.NULL -> null
            raw is Boolean || raw is Int || raw is Long || raw is Double -> null
            raw is String -> if (raw.length <= MAX_VALUE) null else "probe value longer than $MAX_VALUE"
            raw is JSONObject || raw is JSONArray -> "probe value is not a scalar"
            else -> "probe value has type ${raw.javaClass.name}"
        }
        if (problem != null) {
            reject(problem)
            return
        }
        val value: Any? = if (raw == JSONObject.NULL) null else raw
        if (name.startsWith("metric.")) {
            log.metric(name, value as? Number)
        } else {
            log.emit("app.probe", mapOf("name" to name, "value" to value))
        }
        onProbe(name, value)
    }

    private fun reject(why: String) {
        log.emit("app.error", mapOf("error" to "probe rejected: $why"))
    }

    companion object {
        /** Name of the injected port object. The page uses `window.rqLab` (defined by [SCRIPT]). */
        const val PORT_NAME = "rqLabPort"
        private const val HTTPS_PORT = 443
        private const val HTTP_PORT = 80
        private const val MAX_PROBES = 1000
        private const val MAX_MESSAGE = 4096
        private const val MAX_VALUE = 2048
        private val NAME = Regex("[a-z][a-z0-9._-]{0,63}")

        /**
         * Runs before the page scripts (addDocumentStartJavaScript), or after the load
         * ([Channel.AFTER_LOAD]; then the DOMContentLoaded probes run at once). The probes at
         * DOMContentLoaded come after every chunk probe of the page (the parser has run all chunk
         * scripts by then).
         * `csp.inline` inserts an inline script without a nonce: under `script-src 'nonce-…'` it
         * must not run. A step that throws sends `null` for its probe.
         */
        val SCRIPT = """
            (function () {
              var port = window.rqLabPort;
              if (!port) { return 'no-port'; }
              if (window.rqLab) { return 'present'; }
              function send(name, value) {
                try {
                  port.postMessage(JSON.stringify({ name: String(name), value: value === undefined ? null : value }));
                } catch (e) { /* the port is gone: nothing to report to */ }
              }
              Object.defineProperty(window, 'rqLab', { value: Object.freeze({ probe: send }) });
              try {
                new PerformanceObserver(function (list) {
                  list.getEntries().forEach(function (e) {
                    if (e.name === 'first-contentful-paint') { send('metric.first_paint', Math.round(e.startTime)); }
                  });
                }).observe({ type: 'paint', buffered: true });
              } catch (e) { send('metric.first_paint', null); }
              function domProbes() {
                send('location', String(document.location.href));
                send('origin', String(window.origin));
                var status = null;
                try {
                  var nav = performance.getEntriesByType('navigation')[0];
                  if (nav && typeof nav.responseStatus === 'number') { status = nav.responseStatus; }
                } catch (e) { status = null; }
                send('status', status);
                var inline = null;
                try {
                  window.rqInlineRan = false;
                  var s = document.createElement('script');
                  s.textContent = 'window.rqInlineRan = true;';
                  (document.head || document.documentElement).appendChild(s);
                  s.remove();
                  inline = window.rqInlineRan === true;
                } catch (e) { inline = null; }
                send('csp.inline', inline);
              }
              if (document.readyState === 'loading') {
                document.addEventListener('DOMContentLoaded', domProbes);
              } else {
                domProbes();
              }
              return 'installed';
            })();
        """.trimIndent()
    }
}
