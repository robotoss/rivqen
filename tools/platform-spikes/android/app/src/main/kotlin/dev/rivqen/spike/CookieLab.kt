// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spike

import android.os.Handler
import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import org.json.JSONException
import org.json.JSONObject
import org.json.JSONTokener
import java.io.IOException

/**
 * Cookie cases of CAP-A-06 and CAP-A-07 (synthetic names and values only, `[a-z0-9-]`). The
 * helpers run on the main thread; every callback comes back on it through [main].
 */
internal object CookieLab {
    /** Attribute variants, one cookie each per site (CAP-A-07). `<p>` is the site prefix (`a`, `b`). */
    val VARIANTS: List<Pair<String, String>> = listOf(
        "plain" to "",
        "http" to "; HttpOnly",
        "secure" to "; Secure",
        "strict" to "; SameSite=Strict",
        "lax" to "; SameSite=Lax",
        "none" to "; SameSite=None; Secure",
        "none-insecure" to "; SameSite=None",
    )

    /** `Set-Cookie`-style lines of the variants for one site prefix. */
    fun lines(prefix: String): List<String> = VARIANTS.map { (name, attrs) -> "$prefix-$name=v1; Path=/$attrs" }

    /** Sets [lines] for [url] one after the other; [done] gets the callback result of each line. */
    fun setAll(cm: CookieManager, url: String, lines: List<String>, done: (List<Boolean>) -> Unit) {
        val results = ArrayList<Boolean>(lines.size)
        fun next(i: Int) {
            if (i == lines.size) {
                done(results)
                return
            }
            cm.setCookie(url, lines[i]) { ok ->
                results.add(ok == true)
                next(i + 1)
            }
        }
        next(0)
    }

    /**
     * A native GET of the spike-host echo (`/s/echo`, JSON) with [cookie] as the `Cookie` field (the
     * value that `CookieManager.getCookie` gave, or none). [done] gets the `cookie` member that the
     * host saw, or `error: <class>` on a failure. OkHttp has no cookie jar here: only this field is sent.
     */
    fun fetchEcho(client: OkHttpClient, url: HttpUrl, cookie: String?, main: Handler, done: (String?) -> Unit) {
        val b = Request.Builder().url(url).get()
        if (!cookie.isNullOrEmpty()) b.header("Cookie", cookie)
        client.newCall(b.build()).enqueue(
            object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    main.post { done("error: ${e.javaClass.name}") }
                }

                override fun onResponse(call: Call, response: Response) {
                    val v = response.use { r ->
                        if (r.code != HTTP_OK) {
                            "error: http ${r.code}"
                        } else {
                            // The echo body is a small JSON object; at most MAX_ECHO bytes are read.
                            echoCookie(r.peekBody(MAX_ECHO.toLong()).string())
                        }
                    }
                    main.post { done(v) }
                }
            },
        )
    }

    /** The `cookie` member of an echo JSON text, `null` when it is null, or `error: …`. */
    fun echoCookie(json: String?): String? {
        if (json == null) return "error: no echo"
        return try {
            val o = JSONObject(json)
            if (o.isNull("cookie")) null else o.getString("cookie")
        } catch (e: JSONException) {
            "error: ${e.javaClass.name}"
        }
    }

    /** Decodes an `evaluateJavascript` result that is a JSON string (else `null`). */
    fun jsString(result: String?): String? =
        try {
            JSONTokener(result ?: "null").nextValue() as? String
        } catch (e: JSONException) {
            null
        }

    /**
     * Facts of a main-frame request that a page or the app started (the security question of
     * CAP-A-07): `hasGesture`, `isRedirect`, the request field names, and the values of the fields
     * that tell the initiator (`Referer`, `Origin`, `Sec-Fetch-*`). Cookie values are not included.
     */
    fun requestFacts(request: WebResourceRequest): String {
        val headers = request.requestHeaders.mapKeys { it.key.lowercase() }
        val o = JSONObject()
        o.put("url", request.url.toString())
        o.put("gesture", request.hasGesture())
        o.put("redirect", request.isRedirect)
        o.put("fields", headers.keys.sorted().joinToString(","))
        for (name in INITIATOR_FIELDS) headers[name]?.let { o.put(name, it) }
        return o.toString()
    }

    private const val HTTP_OK = 200
    private const val MAX_ECHO = 16 * 1024
    private val INITIATOR_FIELDS = listOf("referer", "origin", "sec-fetch-site", "sec-fetch-mode", "sec-fetch-user", "sec-fetch-dest")

    /** Reads the echo `<pre id="echo">` text of the page (JSON string result). */
    const val ECHO_SCRIPT = "(function(){var e=document.getElementById('echo');return e?e.textContent:null;})()"

    /** `document.cookie` of the page (JSON string result). */
    const val DOC_COOKIE_SCRIPT = "(function(){try{return String(document.cookie);}catch(e){return 'error: '+e.name;}})()"
}
