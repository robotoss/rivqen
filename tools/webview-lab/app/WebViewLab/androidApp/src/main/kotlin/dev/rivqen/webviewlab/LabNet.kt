// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

package dev.rivqen.webviewlab

import android.webkit.WebResourceResponse
import java.io.ByteArrayInputStream

/**
 * The request rule of the lab (CONTRACT section 6): the WebView may load only from the lab
 * server, `http://10.0.2.2:<port>`. Everything else is blocked.
 */
internal object LabNet {
    /** The host loopback as seen from the Android emulator. */
    const val HOST: String = "10.0.2.2"

    const val MIN_PORT: Int = 1024
    const val MAX_PORT: Int = 65535

    fun isValidPort(port: Int): Boolean = port in MIN_PORT..MAX_PORT

    /** The origin of the lab server, as used in the origin rules of androidx.webkit. */
    fun origin(port: Int): String = "http://$HOST:$port"

    fun caseUrl(port: Int, id: String): String = "${origin(port)}/c/$id"

    /**
     * True when a URL goes to the lab server: scheme `http`, host [HOST], the exact [port], no
     * user information. The parts come from `android.net.Uri`; a missing port is `-1`.
     */
    fun isLabUrl(scheme: String?, host: String?, urlPort: Int, userInfo: String?, port: Int): Boolean =
        scheme.equals("http", ignoreCase = true) &&
            host.equals(HOST, ignoreCase = true) &&
            urlPort == port &&
            userInfo == null

    /** The response for a blocked subresource: 403 and an empty body, no network access. */
    fun blockedResponse(): WebResourceResponse = WebResourceResponse(
        "text/plain",
        "utf-8",
        403,
        "Blocked by the lab",
        emptyMap(),
        ByteArrayInputStream(ByteArray(0)),
    )
}
