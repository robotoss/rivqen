// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.app.Activity;
import android.graphics.Bitmap;
import android.os.Bundle;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebView;
import android.webkit.WebViewClient;

/**
 * Hosts the real WebView of a WebView run (DL-100: one smoke run per mode). The WebView asks the
 * bound session client for the main document and reports the page end, as an integrating app
 * would. No JavaScript interface is added: the harness makes the bridge call itself (step
 * {@code get_diff}).
 */
public final class WebViewActivity extends Activity {

    private WebView view;
    private volatile Events events = Events.NONE;
    private volatile RecordingClient client;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        view = new WebView(this);
        view.getSettings().setJavaScriptEnabled(true);
        view.getSettings().setAllowFileAccess(false);
        view.getSettings().setAllowContentAccess(false);
        view.setWebViewClient(new Client());
        setContentView(view);
    }

    @Override
    protected void onDestroy() {
        if (view != null) {
            view.destroy();
            view = null;
        }
        super.onDestroy();
    }

    void attach(Events sink) {
        events = sink;
    }

    WebView webView() {
        return view;
    }

    /** Sends the intercepted main-frame requests and the page end to this client. */
    void route(RecordingClient target) {
        client = target;
    }

    private final class Client extends WebViewClient {

        @Override
        public WebResourceResponse shouldInterceptRequest(WebView v, WebResourceRequest request) {
            RecordingClient c = client;
            if (c == null || !request.isForMainFrame()) {
                return null;
            }
            String url = request.getUrl().toString();
            events.emit("webview.intercept_request", "url", url, "method", request.getMethod());
            Object response = c.requestResource(url);
            if (response == null) {
                events.emit("client.intercept", "session", c.session(), "null", true);
                return null;
            }
            if (!(response instanceof WebResourceResponse)) {
                events.emit("harness.error", "error", "intercept object is " + response.getClass().getName());
                return null;
            }
            return (WebResourceResponse) response;
        }

        @Override
        public void onPageStarted(WebView v, String url, Bitmap favicon) {
            events.emit("webview.page_started", "url", url);
        }

        @Override
        public void onPageFinished(WebView v, String url) {
            events.emit("webview.page_finished", "url", url);
            RecordingClient c = client;
            if (c != null) {
                c.pageFinish(url);
            }
        }
    }
}
