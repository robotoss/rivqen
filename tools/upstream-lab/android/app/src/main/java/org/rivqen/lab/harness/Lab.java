// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.util.Log;

/** Constants and the device log of the harness (tag {@code RivqenLab} only, CONTRACT section 5). */
final class Lab {

    static final String TAG = "RivqenLab";

    /** The only origin of a run (CONTRACT section 1). */
    static final String ORIGIN_PREFIX = "http://10.0.2.2:18090/";
    static final String PAGE_URL = ORIGIN_PREFIX + "catalog";

    /** Limits for untrusted input from the control channel and the network. */
    static final int MAX_CONTROL_BODY = 1024 * 1024;
    static final long MAX_INTERCEPT_BODY = 32L * 1024 * 1024;
    static final int MAX_STEPS = 10_000;
    static final long RUN_DEADLINE_MS = 20L * 60 * 1000;
    static final int MAX_EVENT_QUEUE = 20_000;
    static final int MAX_LOG_CHARS = 1000;
    static final int MAX_CACHE_FILES = 1000;
    static final int MAX_FUNCTION_CASES = 10_000;
    static final long MAX_FUNCTION_FILE = 8L * 1024 * 1024;

    private Lab() {
    }

    static void log(String message) {
        String text = message.length() > MAX_LOG_CHARS ? message.substring(0, MAX_LOG_CHARS) : message;
        Log.i(TAG, text);
    }

    /** Short, safe error text: the exception class and the first part of its message. */
    static String describe(Throwable e) {
        String message = e.getMessage();
        String text = e.getClass().getName() + (message == null ? "" : ": " + message);
        return text.length() > 300 ? text.substring(0, 300) : text;
    }
}
