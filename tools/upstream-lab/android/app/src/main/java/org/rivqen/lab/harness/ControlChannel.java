// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import org.json.JSONException;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.util.regex.Pattern;

/**
 * Client of the conductor control channel (CONTRACT section 4): HTTP/1.1, JSON bodies. Every answer
 * is untrusted: the body is bounded and must be a JSON object.
 */
final class ControlChannel {

    /** Only the host loopback of the emulator; the network security config allows nothing else. */
    private static final Pattern CONTROL_URL = Pattern.compile("^http://10\\.0\\.2\\.2:[0-9]{1,5}$");
    private static final int CONNECT_TIMEOUT_MS = 5_000;
    /** Long poll of the conductor is at most 30 s. */
    private static final int POLL_READ_TIMEOUT_MS = 40_000;
    private static final int READ_TIMEOUT_MS = 10_000;

    private final String base;

    ControlChannel(String controlUrl) {
        if (controlUrl == null || !CONTROL_URL.matcher(controlUrl).matches()) {
            throw new IllegalArgumentException("control URL must be http://10.0.2.2:<port>");
        }
        this.base = controlUrl;
    }

    /** {@code GET /run/scenario}: the scenario and the mode of the run. */
    JSONObject scenario(String run) throws IOException {
        Answer a = request("GET", "/run/scenario?run=" + encode(run), null, READ_TIMEOUT_MS);
        if (a.status != 200) {
            throw new IOException("GET /run/scenario: status " + a.status);
        }
        return a.object();
    }

    /** {@code GET /run/next}: the next harness step, or {@code null} on a poll timeout (204). */
    JSONObject next(String run) throws IOException {
        Answer a = request("GET", "/run/next?run=" + encode(run), null, POLL_READ_TIMEOUT_MS);
        if (a.status == 204) {
            return null;
        }
        if (a.status != 200) {
            throw new IOException("GET /run/next: status " + a.status);
        }
        return a.object();
    }

    /** {@code POST /run/event}: one event; the conductor answers 204. */
    void event(JSONObject event) throws IOException {
        Answer a = request("POST", "/run/event", event.toString().getBytes(StandardCharsets.UTF_8), READ_TIMEOUT_MS);
        if (a.status != 204 && a.status != 200) {
            throw new IOException("POST /run/event: status " + a.status);
        }
    }

    private Answer request(String method, String path, byte[] body, int readTimeout) throws IOException {
        HttpURLConnection c = (HttpURLConnection) new URL(base + path).openConnection();
        try {
            c.setRequestMethod(method);
            c.setConnectTimeout(CONNECT_TIMEOUT_MS);
            c.setReadTimeout(readTimeout);
            c.setUseCaches(false);
            c.setInstanceFollowRedirects(false);
            c.setRequestProperty("Accept", "application/json");
            if (body != null) {
                c.setDoOutput(true);
                c.setRequestProperty("Content-Type", "application/json");
                c.setFixedLengthStreamingMode(body.length);
                try (OutputStream out = c.getOutputStream()) {
                    out.write(body);
                }
            }
            int status = c.getResponseCode();
            byte[] data;
            InputStream in = status >= 400 ? c.getErrorStream() : c.getInputStream();
            data = in == null ? new byte[0] : readBounded(in);
            return new Answer(status, data);
        } finally {
            c.disconnect();
        }
    }

    private static byte[] readBounded(InputStream in) throws IOException {
        try (InputStream input = in) {
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            byte[] buf = new byte[8192];
            int n;
            while ((n = input.read(buf)) != -1) {
                if (out.size() + n > Lab.MAX_CONTROL_BODY) {
                    throw new IOException("control answer over " + Lab.MAX_CONTROL_BODY + " bytes");
                }
                out.write(buf, 0, n);
            }
            return out.toByteArray();
        }
    }

    private static String encode(String value) throws IOException {
        return URLEncoder.encode(value, "UTF-8");
    }

    private static final class Answer {
        final int status;
        final byte[] body;

        Answer(int status, byte[] body) {
            this.status = status;
            this.body = body;
        }

        JSONObject object() throws IOException {
            try {
                return new JSONObject(new String(body, StandardCharsets.UTF_8));
            } catch (JSONException e) {
                throw new IOException("control answer is not a JSON object", e);
            }
        }
    }
}
