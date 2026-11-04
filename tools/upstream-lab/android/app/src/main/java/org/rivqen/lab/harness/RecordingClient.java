// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.webkit.WebView;

import com.tencent.sonic.sdk.SonicSessionClient;

import org.json.JSONArray;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;

/**
 * The web view side of a session (the session client of docs/engineering/protocol/legacy-client.md).
 * Each page action of the SDK becomes an event. In a WebView run the action also goes to the real
 * WebView on the main thread; in a scripted run nothing is loaded.
 */
final class RecordingClient extends SonicSessionClient {

    private final Events events;
    private final WebView webView;
    private final Handler main = new Handler(Looper.getMainLooper());

    /** @param webView the WebView of a WebView run, or {@code null} for a scripted run. */
    RecordingClient(Events events, WebView webView) {
        this.events = events;
        this.webView = webView;
    }

    @Override
    public void loadUrl(String url, Bundle extraData) {
        JSONArray keys = new JSONArray();
        if (extraData != null) {
            List<String> names = new ArrayList<>(extraData.keySet());
            Collections.sort(names);
            for (String k : names) {
                keys.put(k);
            }
        }
        events.emit("client.load_url", "url", url, "extras", keys);
        if (webView != null) {
            onMain(() -> webView.loadUrl(url));
        }
    }

    @Override
    public void loadDataWithBaseUrl(String baseUrl, String data, String mimeType, String encoding, String historyUrl) {
        recordData(baseUrl, data, mimeType, encoding, historyUrl, null, false);
    }

    @Override
    public void loadDataWithBaseUrlAndHeader(String baseUrl, String data, String mimeType, String encoding,
                                             String historyUrl, HashMap<String, String> headers) {
        recordData(baseUrl, data, mimeType, encoding, historyUrl, headers, true);
    }

    @Override
    public void clearHistory() {
        events.emit("client.clear_history");
        if (webView != null) {
            onMain(webView::clearHistory);
        }
    }

    private void recordData(String baseUrl, String data, String mimeType, String encoding, String historyUrl,
                            HashMap<String, String> headers, boolean withHeaders) {
        String body = data == null ? "" : data;
        events.emit("client.load_data", Json.object(
                "base_url", baseUrl, "mime", mimeType, "encoding", encoding, "history_url", historyUrl,
                // null: the variant without headers was called.
                "headers", withHeaders ? Json.headerPairs(headers) : null,
                "body_len", body.getBytes(StandardCharsets.UTF_8).length,
                "body_sha256", Json.sha256Utf8(body)));
        if (webView != null) {
            // A WebView takes no response headers with data (the Quick mode limit of the legacy page).
            onMain(() -> webView.loadDataWithBaseURL(baseUrl, body, mimeType, encoding, historyUrl));
        }
    }

    private void onMain(Runnable action) {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            action.run();
        } else {
            main.post(action);
        }
    }
}
