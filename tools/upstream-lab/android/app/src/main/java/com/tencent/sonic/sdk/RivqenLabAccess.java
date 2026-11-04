// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package com.tencent.sonic.sdk;

import android.content.Intent;
import android.os.Bundle;

import org.json.JSONObject;
import org.rivqen.lab.harness.SessionListener;

/**
 * Rivqen lab code, not upstream code. It is in the SDK package only to call package-private and
 * protected members of the unmodified SDK (CONTRACT section 7; no reflection, no change of
 * visibility, no write to a field of an upstream object). Every member it uses is listed in
 * tools/upstream-lab/android/package-access.json.
 */
public final class RivqenLabAccess {

    private RivqenLabAccess() {
    }

    /**
     * Registers the listener for the session callbacks (package-private interface
     * {@code SonicSessionCallback}) and the state changes ({@code addSessionStateChangedCallback},
     * protected). The SDK keeps weak references: the caller keeps the returned object.
     */
    public static Object attach(SonicSession session, SessionListener listener) {
        Adapter adapter = new Adapter(listener);
        session.addSessionCallback(adapter);
        session.addSessionStateChangedCallback(adapter);
        return adapter;
    }

    /** {@code SonicUtils.separateTemplateAndData}: {template, data JSON, "true"/"false"}. */
    public static String[] split(String sessionId, String html) {
        StringBuilder template = new StringBuilder();
        StringBuilder data = new StringBuilder();
        boolean ok = SonicUtils.separateTemplateAndData(sessionId, html, template, data);
        return new String[] {template.toString(), data.toString(), String.valueOf(ok)};
    }

    /** {@code SonicUtils.getDiffData(JSONObject, JSONObject)}. */
    public static JSONObject diff(JSONObject local, JSONObject server) {
        return SonicUtils.getDiffData(local, server);
    }

    /** {@code SonicUtils.getCharset}. */
    public static String charset(String contentType) {
        return SonicUtils.getCharset(contentType);
    }

    /**
     * Runs {@code SonicServer.connect()} (protected) over the connection of the session
     * configuration and returns the value of the ETag field after it.
     */
    public static String etagAfterConnect(SonicSession session) {
        SonicServer server = new SonicServer(session, new Intent());
        int code = server.connect();
        if (code != SonicConstants.ERROR_CODE_SUCCESS) {
            throw new IllegalStateException("connect returned " + code);
        }
        return server.getResponseHeaderField(server.getCustomHeadFieldEtagKey());
    }

    private static final class Adapter implements SonicSessionCallback, SonicSession.Callback {

        private final SessionListener listener;

        Adapter(SessionListener listener) {
            this.listener = listener;
        }

        private void call(String name, int... args) {
            listener.onCallback(name, args);
        }

        @Override
        public void onSessionStateChange(SonicSession session, int oldState, int newState, Bundle extraData) {
            call("onSessionStateChange", oldState, newState);
        }

        @Override
        public void onSessionLoadLocalCache(String cacheHtml) {
            call("onSessionLoadLocalCache");
        }

        @Override
        public void onSessionDataUpdated(String serverRsp) {
            call("onSessionDataUpdated");
        }

        @Override
        public void onSessionFirstLoad(String html) {
            call("onSessionFirstLoad");
        }

        @Override
        public void onSessionHttpError(int responseCode) {
            call("onSessionHttpError", responseCode);
        }

        @Override
        public void onSessionHitCache() {
            call("onSessionHitCache");
        }

        @Override
        public void onSessionUnAvailable() {
            call("onSessionUnAvailable");
        }

        @Override
        public void onSessionTemplateChanged(String newHtml) {
            call("onSessionTemplateChanged");
        }

        @Override
        public void onSessionSaveCache(String htmlString, String templateString, String dataString) {
            call("onSessionSaveCache");
        }

        @Override
        public void onSonicSessionStart() {
            call("onSonicSessionStart");
        }

        @Override
        public void onSessionDestroy() {
            call("onSessionDestroy");
        }

        @Override
        public void onSonicSessionRefresh() {
            call("onSonicSessionRefresh");
        }
    }
}
