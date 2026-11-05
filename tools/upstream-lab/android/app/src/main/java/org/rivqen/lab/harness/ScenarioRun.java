// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.content.Context;
import android.webkit.WebResourceResponse;

import com.tencent.sonic.sdk.RivqenLabAccess;
import com.tencent.sonic.sdk.SonicDiffDataCallback;
import com.tencent.sonic.sdk.SonicEngine;
import com.tencent.sonic.sdk.SonicSession;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * One scenario run: it reads the scenario from the control channel, then executes the harness
 * steps of CONTRACT section 3 one by one in the order the conductor gives them (DL-100).
 */
final class ScenarioRun {

    private static final long MAIN_TIMEOUT_MS = 10_000;
    /** Default and largest time the step {@code request_resource} waits for the call to return. */
    private static final long INTERCEPT_ACK_MS = 1_000;
    private static final long INTERCEPT_ACK_MAX_MS = 30_000;
    private static final long BRIDGE_TIMEOUT_MS = 5_000;
    private static final long READERS_AT_END_MS = 10_000;
    private static final long MAX_HEADER_TEXT = 64 * 1024;

    private final Context context;
    private final ControlChannel control;
    private final ControlEvents events;
    private final String run;
    private final WebViewLauncher launcher;

    private final List<Bound> sessions = new ArrayList<>();
    private final List<Thread> readers = Collections.synchronizedList(new ArrayList<Thread>());
    private RunConfig config;
    private LabRuntime runtime;
    private String pageUrl = Lab.PAGE_URL;
    private WebViewActivity webView;
    private int workers;

    /** Starts the WebView activity for a WebView run (the instrumentation owns the activity). */
    interface WebViewLauncher {
        WebViewActivity launch() throws Exception;
    }

    ScenarioRun(Context context, ControlChannel control, ControlEvents events, String run, WebViewLauncher launcher) {
        this.context = context;
        this.control = control;
        this.events = events;
        this.run = run;
        this.launcher = launcher;
    }

    /** Executes the run. Returns {@code null} when it ended normally, else the reason it stopped. */
    String execute() throws Exception {
        JSONObject answer = control.scenario(run);
        JSONObject scenario = answer.getJSONObject("scenario");
        config = new RunConfig(answer.getString("mode"), scenario.optJSONObject("config"));
        if (scenario.has("url")) {
            pageUrl = scenario.getString("url");
            if (!pageUrl.startsWith(Lab.ORIGIN_PREFIX)) {
                throw new IllegalArgumentException("scenario url must start with " + Lab.ORIGIN_PREFIX);
            }
        }
        runtime = new LabRuntime(context, events, config.account, config.cookie, config.networkValid);
        Threads.onMain(() -> SonicEngine.createInstance(runtime, config.engineConfig()), MAIN_TIMEOUT_MS);
        if (scenario.optBoolean("webview", false)) {
            webView = launcher.launch();
        }

        long deadline = System.currentTimeMillis() + Lab.RUN_DEADLINE_MS;
        int steps = 0;
        while (true) {
            if (System.currentTimeMillis() > deadline) {
                return "run over " + Lab.RUN_DEADLINE_MS + " ms";
            }
            if (events.failure() != null) {
                return events.failure();
            }
            JSONObject step = control.next(run);
            if (step == null) {
                continue;
            }
            String what = step.optString("do", "");
            if ("end".equals(what)) {
                break;
            }
            if (++steps > Lab.MAX_STEPS) {
                return "over " + Lab.MAX_STEPS + " steps";
            }
            // The conductor writes step.start (CONTRACT section 5); the harness acknowledges with step.done.
            String error;
            try {
                error = step(what, step);
            } catch (Exception e) {
                error = Lab.describe(e);
            }
            // error is a member only on failure (CONTRACT section 5).
            if (error == null) {
                events.emit("step.done", "step", step);
            } else {
                events.emit("step.done", "step", step, "error", error);
            }
            if (!events.flush(30_000)) {
                return events.failure() != null ? events.failure() : "events not delivered in 30 s";
            }
        }
        finishReaders();
        return null;
    }

    /** Closes the WebView activity, if any. */
    void close() {
        if (webView != null) {
            try {
                Threads.onMain(() -> {
                    webView.finish();
                    return null;
                }, MAIN_TIMEOUT_MS);
            } catch (Exception e) {
                Lab.log("WebView activity not closed: " + Lab.describe(e));
            }
        }
    }

    /** Executes one harness step; returns an error text or {@code null}. */
    private String step(String what, JSONObject step) throws Exception {
        switch (what) {
            case "create_session":
                return createSession();
            case "bind_client":
                return bindClient(step);
            case "client_ready":
                return onSessionMain(step, b -> b.client.clientReady());
            case "request_resource":
                return requestResource(step);
            case "page_finished":
                if (webView != null) {
                    return "webview_drives: the WebView reports the page end itself";
                }
                return onSessionMain(step, b -> b.client.pageFinish(pageUrl));
            case "get_diff":
                return getDiff(step);
            case "refresh":
                return onSessionMain(step, b -> b.session.refresh());
            case "destroy":
                return destroy(step);
            case "set_account":
                runtime.setAccount(step.isNull("value") ? null : step.getString("value"));
                return null;
            case "set_cookie":
                runtime.setCookie(step.isNull("value") ? null : step.getString("value"));
                return null;
            case "set_network":
                runtime.setNetworkValid(step.getBoolean("valid"));
                return null;
            case "snapshot_cache":
                snapshotCache();
                return null;
            default:
                return "unknown harness step: " + what;
        }
    }

    private String createSession() throws Exception {
        final int index = sessions.size() + 1;
        Bound created = Threads.onMain(() -> {
            runtime.closeSessionGate();
            try {
                SonicSession s = SonicEngine.getInstance().createSession(pageUrl, config.sessionConfig());
                if (s == null) {
                    return null;
                }
                Bound b = new Bound(index, s);
                b.listenerRef = RivqenLabAccess.attach(s, (name, args) -> {
                    JSONArray a = new JSONArray();
                    for (int v : args) {
                        a.put(v);
                    }
                    events.emit("session.callback", "session", b.index, "name", name, "args", a);
                });
                return b;
            } finally {
                runtime.openSessionGate();
            }
        }, MAIN_TIMEOUT_MS);
        if (created == null) {
            // A refused session has no index: no session member (CONTRACT section 5: a 1-based index).
            events.emit("session.created", "session_id", null);
            return null;
        }
        sessions.add(created);
        events.emit("session.created", "session", created.index, "session_id", created.session.id);
        return null;
    }

    private String bindClient(JSONObject step) throws Exception {
        Bound b = target(step);
        if (b == null) {
            return "no session";
        }
        if (b.client != null) {
            return "client already bound";
        }
        RecordingClient client = new RecordingClient(events, b.index, webView == null ? null : webView.webView());
        Boolean ok = Threads.onMain(() -> b.session.bindClient(client), MAIN_TIMEOUT_MS);
        if (!Boolean.TRUE.equals(ok)) {
            return "bindClient returned false";
        }
        b.client = client;
        if (webView != null) {
            webView.route(client);
        }
        return null;
    }

    private interface SessionCall {
        void call(Bound b) throws Exception;
    }

    private String onSessionMain(JSONObject step, SessionCall call) throws Exception {
        Bound b = target(step);
        if (b == null) {
            return "no session";
        }
        if (b.client == null) {
            return "no client bound";
        }
        Threads.onMain(() -> {
            call.call(b);
            return null;
        }, MAIN_TIMEOUT_MS);
        return null;
    }

    /**
     * Calls the client as a web view would for the main document, on a worker thread. The step is
     * done when the call returns or after {@code wait_ms} (default 1000 ms); a stream is then read
     * to the end on the same worker thread and recorded by {@link RecordingStream}.
     */
    private String requestResource(JSONObject step) throws Exception {
        if (webView != null) {
            return "webview_drives: the WebView requests the page itself";
        }
        final Bound b = target(step);
        if (b == null) {
            return "no session";
        }
        if (b.client == null) {
            return "no client bound";
        }
        long waitMs = Math.max(0, Math.min(INTERCEPT_ACK_MAX_MS, step.optLong("wait_ms", INTERCEPT_ACK_MS)));
        final CountDownLatch returned = new CountDownLatch(1);
        final String[] failure = new String[1];
        Thread t = new Thread(() -> {
            Object response;
            try {
                response = b.client.requestResource(pageUrl);
            } catch (RuntimeException e) {
                failure[0] = Lab.describe(e);
                returned.countDown();
                return;
            }
            returned.countDown();
            readToEnd(b.index, response);
        }, "rq-intercept-" + (++workers));
        t.setDaemon(true);
        readers.add(t);
        t.start();
        if (returned.await(waitMs, TimeUnit.MILLISECONDS)) {
            return failure[0];
        }
        return null;
    }

    private void readToEnd(int session, Object response) {
        if (response == null) {
            events.emit("client.intercept", "session", session, "null", true);
            return;
        }
        if (!(response instanceof WebResourceResponse)) {
            events.emit("harness.error", "error", "intercept object is " + response.getClass().getName());
            return;
        }
        InputStream data = ((WebResourceResponse) response).getData();
        if (data == null) {
            events.emit("harness.error", "error", "intercept response has no stream");
            return;
        }
        byte[] buf = new byte[8192];
        long total = 0;
        try (InputStream in = data) {
            int n;
            while ((n = in.read(buf)) != -1) {
                total += n;
                if (total > Lab.MAX_INTERCEPT_BODY && in instanceof RecordingStream) {
                    ((RecordingStream) in).record(false, new RecordingStream.LimitExceeded());
                    return;
                }
            }
        } catch (IOException e) {
            // RecordingStream recorded the error class in client.intercept.
            Lab.log("intercept stream: " + Lab.describe(e));
        }
    }

    /** Asks for the page message as the JS bridge would, on a worker thread. */
    private String getDiff(JSONObject step) throws Exception {
        final Bound b = target(step);
        if (b == null) {
            return "no session";
        }
        if (b.client == null) {
            return "no client bound";
        }
        final SonicDiffDataCallback callback = resultData ->
                events.emit("session.message", "session", b.index, "json", resultData);
        Threads.Waiter<Void> w = Threads.onWorker("rq-bridge-" + (++workers), () -> {
            b.client.getDiffData(callback);
            return null;
        });
        w.await(BRIDGE_TIMEOUT_MS, "bridge call");
        b.diffCallbacks.add(callback);
        return null;
    }

    private String destroy(JSONObject step) throws Exception {
        final Bound b = target(step);
        if (b == null) {
            return "no session";
        }
        events.emit("session.result", "session", b.index,
                "src_code", b.session.getSrcResultCode(), "final_code", b.session.getFinalResultCode());
        Threads.onMain(() -> {
            b.session.destroy();
            return null;
        }, MAIN_TIMEOUT_MS);
        return null;
    }

    private void snapshotCache() throws IOException, org.json.JSONException {
        File dir = runtime.getSonicCacheDir();
        List<String> names = new ArrayList<>();
        collect(dir, "", names);
        Collections.sort(names);
        JSONArray files = new JSONArray();
        for (String name : names) {
            File f = new File(dir, name);
            JSONObject entry = Json.object("name", name, "size", f.length(), "sha256", sha256(f));
            if (name.endsWith(".header")) {
                // CONTRACT sections 5 and 6: the header file text (synthetic lab fields), so that the
                // determinism check can blank the time fields of the Android HTTP stack.
                entry.put("text", headerText(f));
            }
            files.put(entry);
        }
        events.emit("cache.snapshot", "files", files, "db", null);
    }

    private static void collect(File dir, String prefix, List<String> out) throws IOException {
        File[] children = dir.listFiles();
        if (children == null) {
            return;
        }
        for (File c : children) {
            if (out.size() >= Lab.MAX_CACHE_FILES) {
                throw new IOException("cache over " + Lab.MAX_CACHE_FILES + " files");
            }
            if (c.isDirectory()) {
                collect(c, prefix + c.getName() + "/", out);
            } else {
                out.add(prefix + c.getName());
            }
        }
    }

    /** Header file content as UTF-8, at most {@link #MAX_HEADER_TEXT} bytes. */
    private static String headerText(File f) throws IOException {
        if (f.length() > MAX_HEADER_TEXT) {
            throw new IOException(f.getName() + " over " + MAX_HEADER_TEXT + " bytes");
        }
        byte[] data = new byte[(int) f.length()];
        try (InputStream in = new FileInputStream(f)) {
            int off = 0;
            while (off < data.length) {
                int n = in.read(data, off, data.length - off);
                if (n == -1) {
                    break;
                }
                off += n;
            }
            return new String(data, 0, off, java.nio.charset.StandardCharsets.UTF_8);
        }
    }

    private static String sha256(File f) throws IOException {
        MessageDigest d = Json.sha256();
        byte[] buf = new byte[8192];
        try (InputStream in = new FileInputStream(f)) {
            int n;
            while ((n = in.read(buf)) != -1) {
                d.update(buf, 0, n);
            }
        }
        return Json.hex(d.digest());
    }

    /** The session of the step: member {@code session} (1-based) or the latest created one. */
    private Bound target(JSONObject step) {
        int index = step.optInt("session", sessions.size());
        if (index < 1 || index > sessions.size()) {
            return null;
        }
        return sessions.get(index - 1);
    }

    /** Waits for open intercept readers, so that their events are in the log. */
    private void finishReaders() throws InterruptedException {
        long end = System.currentTimeMillis() + READERS_AT_END_MS;
        List<Thread> open;
        synchronized (readers) {
            open = new ArrayList<>(readers);
        }
        for (Thread t : open) {
            long left = end - System.currentTimeMillis();
            if (left > 0) {
                t.join(left);
            }
            if (t.isAlive()) {
                events.emit("harness.error", "error", "intercept reader still open at the end of the run");
            }
        }
        events.flush(30_000);
    }

    /** A created session, its client and the objects the SDK holds by weak reference. */
    static final class Bound {
        final int index;
        final SonicSession session;
        volatile RecordingClient client;
        Object listenerRef;
        final List<SonicDiffDataCallback> diffCallbacks = Collections.synchronizedList(new ArrayList<SonicDiffDataCallback>());

        Bound(int index, SonicSession session) {
            this.index = index;
            this.session = session;
        }
    }
}
