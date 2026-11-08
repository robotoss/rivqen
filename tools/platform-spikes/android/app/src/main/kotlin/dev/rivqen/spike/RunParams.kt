// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spike

import android.content.Intent
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/**
 * What a run does after the first load (run.sh `--es scenario <name>`, runlog.mjs SCENARIOS).
 *
 * - [STREAM]: the S1 run (CAP-A-01…03); also the redirect (CAP-A-04) and TLS error (CAP-A-05) runs,
 *   whose differences come from the URL and the rule only.
 * - [REDIRECT_3XX]: CAP-A-04 probe: the interceptor tries to give the WebView the fetched 3xx head
 *   as a `WebResourceResponse` (documented as not supported) and records what happens.
 * - [CUT_RANGE]: CAP-A-08: a streamed page whose body is cut gets the CL-33 fallback (one normal
 *   load); on the page of that load a script fetches a byte range of `/s/range`.
 * - [CUT_PARTIAL]: CAP-A-08 `partial` (S3): as [CUT_RANGE]; each `native.doc` snapshot is taken
 *   again [SpikeActivity] `LATE_SNAPSHOT_MS` later, so the log shows what stays on the page after
 *   the body error (the partial page or the error page).
 */
internal enum class Scenario(val extra: String, val cookieIntercept: CookieIntercept? = null, val steps: Boolean = false) {
    STREAM("stream"),
    REDIRECT_3XX("redirect-3xx"),
    CUT_RANGE("cut-range"),
    CUT_PARTIAL("cut-partial"),

    /** CAP-A-06: the intercepted page sets a cookie; [cookieIntercept] says how it reaches the WebView. */
    COOKIE_ON("cookie-on", CookieIntercept.ON_SET_COOKIES),
    COOKIE_ON_MAP("cookie-on-map", CookieIntercept.ON_MAP),
    COOKIE_OFF("cookie-off", CookieIntercept.OFF),
    COOKIE_MANUAL("cookie-manual", CookieIntercept.MANUAL),

    /** CAP-A-07: cookie sync steps without the early fetch. */
    COOKIE_SYNC("cookie-sync", steps = true),

    /** CAP-A-07 `persist`: the seed launch before the run (no run log) and the run that reads what survived. */
    COOKIE_SEED("cookie-seed", steps = true),
    COOKIE_PERSIST("cookie-persist", steps = true),
    ;

    companion object {
        fun of(extra: String?): Scenario? = if (extra == null) STREAM else entries.firstOrNull { it.extra == extra }
    }
}

/**
 * How the `Set-Cookie` of an intercepted response is given to the WebView (CAP-A-06).
 *
 * - [ON_SET_COOKIES]: `WebSettingsCompat.setCookiesIncludedInShouldInterceptRequest(true)` and
 *   `WebResourceResponseCompat.setCookies` (androidx.webkit 1.15, feature COOKIE_INTERCEPT).
 * - [ON_MAP]: the setting on; the `Set-Cookie` stays in the field map.
 * - [OFF]: the setting off; the `Set-Cookie` stays in the field map (the S1 prototype).
 * - [MANUAL]: the setting off; the app writes the `Set-Cookie` into `CookieManager` before it
 *   returns the response and removes it from the map (the rule of transport/http.md §2).
 */
internal enum class CookieIntercept { ON_SET_COOKIES, ON_MAP, OFF, MANUAL }

/**
 * The run parameters from the intent extras (run.sh: `am start --es run <id> --es url <url>
 * --es capability <CAP> [--es scenario <name>]`). Only spike-host URLs are accepted (CONTRACT.md
 * section 1).
 */
internal class RunParams(val run: String, val url: HttpUrl, val capability: String, val scenario: Scenario) {
    companion object {
        const val EXTRA_RUN = "run"
        const val EXTRA_URL = "url"
        const val EXTRA_CAPABILITY = "capability"
        const val EXTRA_SCENARIO = "scenario"
        private const val MAX_URL = 2048
        private val RUN = Regex("[A-Za-z0-9.-]{1,96}")
        private val CAPABILITY = Regex("CAP-A-[0-9]{2}")
        private val URL_TEXT = Regex("[\\x21-\\x7e]+")

        /** Origins of the spike host on the good-certificate port (sites A and B). */
        val HOST_ORIGINS: Set<String> = setOf("https://localhost:18443", "https://10.0.2.2:18443")

        /**
         * Origins the app may load and intercept: the spike host on the good and the bad-certificate
         * port. An allowed origin with a bad certificate is the CAP-A-05 case: the rule claims the
         * request, and the failed fetch must still give nothing to the WebView.
         */
        val LOAD_ORIGINS: Set<String> = HOST_ORIGINS + setOf("https://localhost:18444", "https://10.0.2.2:18444")

        /** @throws IllegalArgumentException when an extra is missing or not allowed */
        fun parse(intent: Intent): RunParams {
            val run = intent.getStringExtra(EXTRA_RUN)
            val text = intent.getStringExtra(EXTRA_URL)
            val capability = intent.getStringExtra(EXTRA_CAPABILITY)
            val scenario = Scenario.of(intent.getStringExtra(EXTRA_SCENARIO))
            require(scenario != null) { "scenario must be one of ${Scenario.entries.joinToString { it.extra }}" }
            require(run != null && RUN.matches(run)) { "run must match ${RUN.pattern}" }
            require(capability != null && CAPABILITY.matches(capability)) { "capability must match ${CAPABILITY.pattern}" }
            require(text != null && text.length <= MAX_URL && URL_TEXT.matches(text)) { "url must be printable ASCII, at most $MAX_URL" }
            val url = text.toHttpUrlOrNull()
            require(url != null) { "url is not an http(s) URL" }
            // The WebView compares the request URL with this string, so it must be in canonical form.
            require(url.toString() == text) { "url is not in canonical form" }
            require(InterceptRule.origin(url.scheme, url.host, url.port) in LOAD_ORIGINS) { "url is not on the spike host" }
            return RunParams(run, url, capability, scenario)
        }
    }
}
