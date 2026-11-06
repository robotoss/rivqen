// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import java.io.FilterInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;

/**
 * A stream that passes the bytes on unchanged, counts and hashes the bytes that it gives to the
 * reader, and reports one {@link Outcome} when the reader reaches the end, a read fails, the
 * stream is closed, or {@link #fail} is called. Java and {@code java.*} only, so that a plain JVM
 * check can run it ({@code check.sh}).
 *
 * <p>Invariants (all state is guarded by {@link #lock}):
 *
 * <ul>
 *   <li>The outcome is reported at most once.
 *   <li>The outcome holds every byte that a read returned to the reader before the outcome, and no
 *       other byte: a read updates the digest and the length under the lock before it returns.
 *   <li>A close while a read is in flight does not report: the last read that leaves after the
 *       close of the source reports it (single owner). So a close and a last read that race give
 *       the same outcome as a close after that read. A read that returns before the close of the
 *       source is done is counted, and the close reports.
 *   <li>A read error after a close started is a close ({@code complete} false, no error).
 *   <li>Bytes that a read gives after the outcome (a source that still gives bytes after its
 *       close) are not counted: the event is already written.
 *   <li>The outcome is reported outside the lock, so the sink cannot block a reader or a closer.
 * </ul>
 */
class TallyStream extends FilterInputStream {

    /** The sink of the outcome. Called once, on the thread that ends the stream. */
    interface Outcome {
        /**
         * @param length bytes given to the reader
         * @param sha256 SHA-256 of those bytes
         * @param complete {@code true} when the reader reached the end of the stream
         * @param error the read error, or {@code null} (also for a close before the end)
         */
        void report(long length, byte[] sha256, boolean complete, Throwable error);
    }

    private final Outcome outcome;
    private final Object lock = new Object();
    private final MessageDigest digest;
    /** Guarded by {@link #lock}. */
    private long length;
    /** Reads in flight. Guarded by {@link #lock}. */
    private int reading;
    /** {@link #close} started. Guarded by {@link #lock}. */
    private boolean closeRequested;
    /** The close of the source returned (or failed). Guarded by {@link #lock}. */
    private boolean closeDone;
    /** Guarded by {@link #lock}. */
    private boolean reported;

    TallyStream(InputStream in, Outcome outcome) {
        super(in);
        this.outcome = outcome;
        try {
            this.digest = MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    @Override
    public int read() throws IOException {
        enter();
        int b;
        try {
            b = super.read();
        } catch (IOException | RuntimeException e) {
            leave(null, 0, 0, false, e);
            throw e;
        }
        if (b == -1) {
            leave(null, 0, 0, true, null);
        } else {
            leave(new byte[] {(byte) b}, 0, 1, false, null);
        }
        return b;
    }

    @Override
    public int read(byte[] buffer, int offset, int count) throws IOException {
        enter();
        int n;
        try {
            n = super.read(buffer, offset, count);
        } catch (IOException | RuntimeException e) {
            leave(null, 0, 0, false, e);
            throw e;
        }
        if (n == -1) {
            leave(null, 0, 0, true, null);
        } else {
            leave(buffer, offset, Math.max(0, n), false, null);
        }
        return n;
    }

    @Override
    public long skip(long n) throws IOException {
        // Skipped bytes would not be hashed; read them instead. Bounded: at most n bytes, 8 KiB buffer.
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

    /** A reset would give bytes to the reader twice and the digest would count them twice. */
    @Override
    public boolean markSupported() {
        return false;
    }

    @Override
    public void mark(int readLimit) {
        // No mark: see markSupported.
    }

    @Override
    public void reset() throws IOException {
        throw new IOException("mark and reset are not supported");
    }

    @Override
    public void close() throws IOException {
        // The flag is set before the close of the source, so a read that the close stops is a close
        // (complete false, no error) and not a read error.
        synchronized (lock) {
            closeRequested = true;
        }
        try {
            super.close();
        } finally {
            Snapshot s;
            synchronized (lock) {
                closeDone = true;
                s = reading == 0 ? take(false, null) : null;
            }
            deliver(s);
        }
    }

    /** Ends the stream with an error that the reader found (for example a body limit). */
    final void fail(Throwable error) {
        Snapshot s;
        synchronized (lock) {
            s = take(false, error);
        }
        deliver(s);
    }

    final boolean isReported() {
        synchronized (lock) {
            return reported;
        }
    }

    private void enter() {
        synchronized (lock) {
            reading++;
        }
    }

    /**
     * Counts the bytes of a read that returned and decides if this read ends the stream.
     *
     * @param end the source returned -1
     * @param error the read failed with this error
     */
    private void leave(byte[] buffer, int offset, int n, boolean end, Throwable error) {
        Snapshot s = null;
        synchronized (lock) {
            reading--;
            if (!reported) {
                if (n > 0) {
                    digest.update(buffer, offset, n);
                    length += n;
                }
                if (end) {
                    s = take(true, null);
                } else if (error != null) {
                    s = take(false, closeRequested ? null : error);
                } else if (closeDone && reading == 0) {
                    s = take(false, null);
                }
            }
        }
        deliver(s);
    }

    /** Takes the outcome once. Call with the lock held; returns null when it is already taken. */
    private Snapshot take(boolean complete, Throwable error) {
        if (reported) {
            return null;
        }
        reported = true;
        return new Snapshot(length, digest.digest(), complete, error);
    }

    private void deliver(Snapshot s) {
        if (s != null) {
            outcome.report(s.length, s.sha256, s.complete, s.error);
        }
    }

    private static final class Snapshot {
        final long length;
        final byte[] sha256;
        final boolean complete;
        final Throwable error;

        Snapshot(long length, byte[] sha256, boolean complete, Throwable error) {
            this.length = length;
            this.sha256 = sha256;
            this.complete = complete;
            this.error = error;
        }
    }
}
