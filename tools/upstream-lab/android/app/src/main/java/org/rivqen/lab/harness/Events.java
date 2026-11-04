// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.os.Looper;

import org.json.JSONException;
import org.json.JSONObject;

/**
 * Sink of the harness events of CONTRACT section 5. The harness sets {@code ev}, {@code src} and
 * {@code thread}; the conductor adds {@code seq} and {@code t}.
 */
public abstract class Events {

    /** Sink that drops every event (function mode has no event log). */
    public static final Events NONE = new Events() {
        @Override
        protected void send(JSONObject event) {
            // Function mode writes results, not events.
        }
    };

    /**
     * Records one event. Never throws: a member that cannot be encoded becomes a {@code harness.error}
     * event, so that the run shows the defect.
     */
    public final void emit(String name, JSONObject members) {
        JSONObject event = members == null ? new JSONObject() : members;
        try {
            event.put("ev", name);
            event.put("src", "harness");
            event.put("thread", threadName());
        } catch (JSONException e) {
            Lab.log("event " + name + " not encoded: " + e.getMessage());
            return;
        }
        send(event);
    }

    /** Shortcut for an event with members given as name and value pairs. */
    public final void emit(String name, Object... pairs) {
        emit(name, Json.object(pairs));
    }

    protected abstract void send(JSONObject event);

    /** {@code main} for the main thread, otherwise the thread name (a worker). */
    static String threadName() {
        if (Looper.getMainLooper().getThread() == Thread.currentThread()) {
            return "main";
        }
        return Thread.currentThread().getName();
    }
}
