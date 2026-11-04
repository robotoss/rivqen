// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import org.json.JSONObject;

import java.io.FilterInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.security.MessageDigest;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * The stream of an intercepted response. It passes the bytes on unchanged, counts and hashes them,
 * and records one {@code client.intercept} event when the reader reaches the end, fails or closes it.
 */
final class RecordingStream extends FilterInputStream {

    private final Events events;
    private final MessageDigest digest = Json.sha256();
    private final AtomicBoolean recorded = new AtomicBoolean(false);
    private long length;
    private volatile JSONObject head = new JSONObject();

    /** The reader stopped at the body limit of the lab ({@link Lab#MAX_INTERCEPT_BODY}). */
    static final class LimitExceeded extends IOException {
        LimitExceeded() {
            super("intercept body over the lab limit");
        }
    }

    private final int session;

    /** @param session 1-based session index of the requesting client (0 when unknown). */
    RecordingStream(InputStream in, Events events, int session) {
        super(in);
        this.events = events;
        this.session = session;
    }

    /** Members of the response object that the runtime created around this stream. */
    void describe(int status, String reason, String mime, String encoding, Map<String, String> headers) {
        head = Json.object("status", status, "reason", reason, "mime", mime, "encoding", encoding,
                "headers", Json.headerPairs(headers));
    }

    @Override
    public int read() throws IOException {
        int b;
        try {
            b = super.read();
        } catch (IOException | RuntimeException e) {
            record(false, e);
            throw e;
        }
        if (b == -1) {
            record(true, null);
        } else {
            digest.update((byte) b);
            length++;
        }
        return b;
    }

    @Override
    public int read(byte[] buffer, int offset, int count) throws IOException {
        int n;
        try {
            n = super.read(buffer, offset, count);
        } catch (IOException | RuntimeException e) {
            record(false, e);
            throw e;
        }
        if (n == -1) {
            record(true, null);
        } else if (n > 0) {
            digest.update(buffer, offset, n);
            length += n;
        }
        return n;
    }

    @Override
    public long skip(long n) throws IOException {
        // Skipped bytes would not be hashed; read them instead.
        byte[] buf = new byte[(int) Math.min(8192, Math.max(0, n))];
        long done = 0;
        while (done < n) {
            int r = read(buf, 0, (int) Math.min(buf.length, n - done));
            if (r <= 0) {
                break;
            }
            done += r;
        }
        return done;
    }

    @Override
    public void close() throws IOException {
        try {
            super.close();
        } finally {
            record(false, null);
        }
    }

    /** Records the event once; a close before the end has {@code complete: false} and no error. */
    void record(boolean complete, Throwable error) {
        if (!recorded.compareAndSet(false, true)) {
            return;
        }
        JSONObject e = Json.object("session", session == 0 ? null : session, "body_len", length, "body_sha256", Json.hex(digest.digest()),
                "complete", complete, "error", error == null ? null : error.getClass().getName());
        try {
            for (java.util.Iterator<String> it = head.keys(); it.hasNext(); ) {
                String k = it.next();
                e.put(k, head.get(k));
            }
        } catch (org.json.JSONException x) {
            Lab.log("intercept members: " + x.getMessage());
        }
        events.emit("client.intercept", e);
    }

    boolean isRecorded() {
        return recorded.get();
    }
}
