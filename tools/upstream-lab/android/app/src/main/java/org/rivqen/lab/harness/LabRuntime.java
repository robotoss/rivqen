// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.content.Context;
import android.os.Handler;
import android.os.HandlerThread;
import android.util.Log;
import android.webkit.WebResourceResponse;

import com.tencent.sonic.sdk.SonicRuntime;
import com.tencent.sonic.sdk.SonicSessionClient;

import org.json.JSONArray;

import java.io.InputStream;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * The host side that the SDK asks for platform services (the runtime of
 * docs/engineering/protocol/legacy-client.md). Account, cookie and network validity are scripted
 * by the scenario; cookies are synthetic. Each call that the contract names becomes an event.
 */
final class LabRuntime extends SonicRuntime {

    /** Fixed synthetic user agent of the lab. */
    static final String USER_AGENT = "RivqenLab/1 (Android harness)";
    /** Longest time a session flow waits for the harness to register its callbacks. */
    private static final long GATE_TIMEOUT_MS = 5_000;

    private final Events events;
    private final Handler worker;
    private volatile String account;
    private volatile String cookie;
    private volatile boolean networkValid = true;
    private volatile CountDownLatch sessionGate;

    LabRuntime(Context context, Events events, String account, String cookie, boolean networkValid) {
        super(context);
        this.events = events;
        this.account = account;
        this.cookie = cookie;
        this.networkValid = networkValid;
        HandlerThread thread = new HandlerThread("rq-sdk-worker");
        thread.start();
        this.worker = new Handler(thread.getLooper());
    }

    void setAccount(String value) {
        account = value;
    }

    void setCookie(String value) {
        cookie = value;
    }

    void setNetworkValid(boolean value) {
        networkValid = value;
    }

    /**
     * Holds session flow tasks until {@link #openSessionGate()}. The harness closes the gate around
     * {@code createSession}, so that a session that starts inside {@code createSession} does not run
     * its flow before the harness callbacks are registered.
     */
    void closeSessionGate() {
        sessionGate = new CountDownLatch(1);
    }

    void openSessionGate() {
        CountDownLatch gate = sessionGate;
        sessionGate = null;
        if (gate != null) {
            gate.countDown();
        }
    }

    @Override
    public void postTaskToSessionThread(final Runnable task) {
        final CountDownLatch gate = sessionGate;
        if (gate == null) {
            super.postTaskToSessionThread(task);
            return;
        }
        super.postTaskToSessionThread(() -> {
            try {
                if (!gate.await(GATE_TIMEOUT_MS, TimeUnit.MILLISECONDS)) {
                    Lab.log("session gate not opened in " + GATE_TIMEOUT_MS + " ms; the flow continues");
                }
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
            task.run();
        });
    }

    @Override
    public void log(String tag, int level, String message) {
        // SDK log lines go to the device log under the harness tag only (CONTRACT section 5).
        if (level >= Log.INFO) {
            Lab.log("sdk " + tag + ": " + message);
        }
    }

    @Override
    public String getCookie(String url) {
        String value = cookie;
        events.emit("runtime.cookie_get", "url", url, "value", value);
        return value;
    }

    @Override
    public boolean setCookie(String url, List<String> cookies) {
        JSONArray values = new JSONArray();
        if (cookies != null) {
            for (String c : cookies) {
                values.put(c);
            }
        }
        events.emit("runtime.cookie_set", "url", url, "value", values);
        return true;
    }

    @Override
    public String getUserAgent() {
        return USER_AGENT;
    }

    @Override
    public String getCurrentUserAccount() {
        return account;
    }

    /** Every URL is accepted: the lab decides what to load, not the runtime. */
    @Override
    public boolean isSonicUrl(String url) {
        return url != null && !url.isEmpty();
    }

    /**
     * A real {@link WebResourceResponse} (3-argument constructor plus the headers) around a
     * recording stream, in both the WebView run and the scripted run.
     */
    @Override
    public Object createWebResourceResponse(String mimeType, String encoding, InputStream data, Map<String, String> headers) {
        RecordingStream stream = new RecordingStream(data, events);
        WebResourceResponse response = new WebResourceResponse(mimeType, encoding, stream);
        if (headers != null) {
            response.setResponseHeaders(headers);
        }
        stream.describe(response.getStatusCode(), response.getReasonPhrase(), mimeType, encoding, headers);
        return response;
    }

    @Override
    public boolean isNetworkValid() {
        return networkValid;
    }

    @Override
    public void showToast(CharSequence text, int duration) {
        events.emit("runtime.toast", "text", text == null ? null : text.toString(), "duration", duration);
    }

    @Override
    public void postTaskToThread(Runnable task, long delayMillis) {
        worker.postDelayed(task, Math.max(0L, delayMillis));
    }

    @Override
    public void notifyError(SonicSessionClient client, String url, int errorCode) {
        events.emit("runtime.notify_error", "url", url, "code", errorCode);
    }
}
