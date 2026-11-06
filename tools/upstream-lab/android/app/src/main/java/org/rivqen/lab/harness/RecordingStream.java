// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import org.json.JSONObject;

import java.io.IOException;
import java.io.InputStream;
import java.util.Map;

/**
 * The stream of an intercepted response. It passes the bytes on unchanged, counts and hashes them,
 * and records one {@code client.intercept} event when the reader reaches the end, fails or closes it.
 * The count, the hash and the once-only rule are in {@link TallyStream} (one lock; a close during a
 * read leaves the event to that read). This class only adds the response members and the event.
 */
final class RecordingStream extends TallyStream {

    /** The reader stopped at the body limit of the lab ({@link Lab#MAX_INTERCEPT_BODY}). */
    static final class LimitExceeded extends IOException {
        LimitExceeded() {
            super("intercept body over the lab limit");
        }
    }

    /** Builds the event when the stream ends; holds the response members until then. */
    private static final class Sink implements Outcome {
        private final Events events;
        private final int session;
        private volatile JSONObject head = new JSONObject();

        Sink(Events events, int session) {
            this.events = events;
            this.session = session;
        }

        @Override
        public void report(long length, byte[] sha256, boolean complete, Throwable error) {
            JSONObject e = Json.object("session", session == 0 ? null : session, "body_len", length,
                    "body_sha256", Json.hex(sha256), "complete", complete,
                    "error", error == null ? null : error.getClass().getName());
            JSONObject members = head;
            try {
                for (java.util.Iterator<String> it = members.keys(); it.hasNext(); ) {
                    String k = it.next();
                    e.put(k, members.get(k));
                }
            } catch (org.json.JSONException x) {
                Lab.log("intercept members: " + x.getMessage());
            }
            events.emit("client.intercept", e);
        }
    }

    private final Sink sink;

    /** @param session 1-based session index of the requesting client (0 when unknown). */
    RecordingStream(InputStream in, Events events, int session) {
        this(in, new Sink(events, session));
    }

    private RecordingStream(InputStream in, Sink sink) {
        super(in, sink);
        this.sink = sink;
    }

    /** Members of the response object that the runtime created around this stream. */
    void describe(int status, String reason, String mime, String encoding, Map<String, String> headers) {
        sink.head = Json.object("status", status, "reason", reason, "mime", mime, "encoding", encoding,
                "headers", Json.headerPairs(headers));
    }
}
