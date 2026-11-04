// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import com.tencent.sonic.sdk.SonicConfig;
import com.tencent.sonic.sdk.SonicConstants;
import com.tencent.sonic.sdk.SonicSessionConfig;

import org.json.JSONException;
import org.json.JSONObject;

import java.util.Arrays;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Set;

/**
 * The scenario {@code config} and the mode, applied only through the public builders of the SDK
 * (H-118: Standard mode is set through the public session configuration). An unknown key stops the
 * run, so that a typing error in a scenario cannot pass as a default.
 */
final class RunConfig {

    static final String DEFAULT_ACCOUNT = "rq-test-account";

    private static final Set<String> KEYS = new HashSet<>(Arrays.asList(
            // runtime
            "account", "cookie", "network_valid",
            // session configuration
            "account_related", "reload_in_bad_network", "accept_diff", "support_cache_control",
            "support_local_server", "auto_start", "connect_timeout_ms", "read_timeout_ms",
            // engine configuration
            "get_cookie_when_session_create", "cache_verify_with_sha1", "unavailable_time_ms"));

    final String mode;
    final String account;
    final String cookie;
    final boolean networkValid;
    private final JSONObject config;

    RunConfig(String mode, JSONObject config) {
        if (!"quick".equals(mode) && !"standard".equals(mode)) {
            throw new IllegalArgumentException("mode must be quick or standard, got " + mode);
        }
        this.mode = mode;
        this.config = config == null ? new JSONObject() : config;
        for (Iterator<String> it = this.config.keys(); it.hasNext(); ) {
            String key = it.next();
            if (!KEYS.contains(key)) {
                throw new IllegalArgumentException("unknown config key: " + key);
            }
        }
        this.account = this.config.isNull("account") && this.config.has("account")
                ? null : this.config.optString("account", DEFAULT_ACCOUNT);
        this.cookie = this.config.isNull("cookie") ? null : this.config.optString("cookie");
        this.networkValid = bool("network_valid", true);
    }

    SonicConfig engineConfig() {
        SonicConfig.Builder b = new SonicConfig.Builder();
        if (config.has("get_cookie_when_session_create")) {
            b.setGetCookieWhenSessionCreate(bool("get_cookie_when_session_create", true));
        }
        if (config.has("cache_verify_with_sha1")) {
            b.setCacheVerifyWithSha1(bool("cache_verify_with_sha1", true));
        }
        if (config.has("unavailable_time_ms")) {
            b.setUnavailableTime(number("unavailable_time_ms"));
        }
        return b.build();
    }

    SonicSessionConfig sessionConfig() {
        SonicSessionConfig.Builder b = new SonicSessionConfig.Builder();
        b.setSessionMode("quick".equals(mode) ? SonicConstants.SESSION_MODE_QUICK : SonicConstants.SESSION_MODE_DEFAULT);
        if (config.has("account_related")) {
            b.setIsAccountRelated(bool("account_related", true));
        }
        if (config.has("reload_in_bad_network")) {
            b.setReloadInBadNetwork(bool("reload_in_bad_network", false));
        }
        if (config.has("accept_diff")) {
            b.setAcceptDiff(bool("accept_diff", true));
        }
        if (config.has("support_cache_control")) {
            b.setSupportCacheControl(bool("support_cache_control", false));
        }
        if (config.has("support_local_server")) {
            b.setSupportLocalServer(bool("support_local_server", false));
        }
        if (config.has("auto_start")) {
            b.setAutoStartWhenCreate(bool("auto_start", true));
        }
        if (config.has("connect_timeout_ms")) {
            b.setConnectTimeoutMillis((int) number("connect_timeout_ms"));
        }
        if (config.has("read_timeout_ms")) {
            b.setReadTimeoutMillis((int) number("read_timeout_ms"));
        }
        return b.build();
    }

    private boolean bool(String key, boolean fallback) {
        if (!config.has(key)) {
            return fallback;
        }
        try {
            Object v = config.get(key);
            if (!(v instanceof Boolean)) {
                throw new IllegalArgumentException("config " + key + " must be a boolean");
            }
            return (Boolean) v;
        } catch (JSONException e) {
            throw new IllegalArgumentException(e);
        }
    }

    private long number(String key) {
        try {
            Object v = config.get(key);
            if (!(v instanceof Number) || ((Number) v).longValue() < 0 || ((Number) v).longValue() > Integer.MAX_VALUE) {
                throw new IllegalArgumentException("config " + key + " must be a number 0.." + Integer.MAX_VALUE);
            }
            return ((Number) v).longValue();
        } catch (JSONException e) {
            throw new IllegalArgumentException(e);
        }
    }
}
