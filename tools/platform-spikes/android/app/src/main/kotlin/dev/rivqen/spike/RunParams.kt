// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.spike

import android.content.Intent
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/**
 * The run parameters from the intent extras (run.sh: `am start --es run <id> --es url <url>
 * --es capability <CAP>`). Only spike-host URLs are accepted (CONTRACT.md section 1).
 */
internal class RunParams(val run: String, val url: HttpUrl, val capability: String) {
    companion object {
        const val EXTRA_RUN = "run"
        const val EXTRA_URL = "url"
        const val EXTRA_CAPABILITY = "capability"
        private const val MAX_URL = 2048
        private val RUN = Regex("[A-Za-z0-9.-]{1,96}")
        private val CAPABILITY = Regex("CAP-A-[0-9]{2}")
        private val URL_TEXT = Regex("[\\x21-\\x7e]+")

        /** Origins of the spike host on the good-certificate port (sites A and B). */
        val HOST_ORIGINS: Set<String> = setOf("https://localhost:18443", "https://10.0.2.2:18443")

        /** Origins the app may load: the spike host on the good and the bad-certificate port. */
        private val LOAD_ORIGINS: Set<String> = HOST_ORIGINS + setOf("https://localhost:18444", "https://10.0.2.2:18444")

        /** @throws IllegalArgumentException when an extra is missing or not allowed */
        fun parse(intent: Intent): RunParams {
            val run = intent.getStringExtra(EXTRA_RUN)
            val text = intent.getStringExtra(EXTRA_URL)
            val capability = intent.getStringExtra(EXTRA_CAPABILITY)
            require(run != null && RUN.matches(run)) { "run must match ${RUN.pattern}" }
            require(capability != null && CAPABILITY.matches(capability)) { "capability must match ${CAPABILITY.pattern}" }
            require(text != null && text.length <= MAX_URL && URL_TEXT.matches(text)) { "url must be printable ASCII, at most $MAX_URL" }
            val url = text.toHttpUrlOrNull()
            require(url != null) { "url is not an http(s) URL" }
            // The WebView compares the request URL with this string, so it must be in canonical form.
            require(url.toString() == text) { "url is not in canonical form" }
            require(InterceptRule.origin(url.scheme, url.host, url.port) in LOAD_ORIGINS) { "url is not on the spike host" }
            return RunParams(run, url, capability)
        }
    }
}
