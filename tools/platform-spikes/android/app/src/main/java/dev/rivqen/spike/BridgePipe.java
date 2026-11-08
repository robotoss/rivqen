// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package dev.rivqen.spike;

import java.io.IOException;
import java.io.InputStream;
import java.io.InterruptedIOException;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;

/**
 * A bounded byte pipe between the early fetch (the writer) and the WebView (the reader of
 * {@link #source()}). Java and {@code java.*} only, so that a plain JVM check runs it
 * ({@code check.sh}).
 *
 * <p>Invariants (all state is guarded by {@link #lock}):
 *
 * <ul>
 *   <li>I1. The reader gets the bytes in the order the writer gave them, unchanged, each byte once.
 *   <li>I2. At most {@code capacity} bytes wait in the pipe. A writer waits for space (back pressure).
 *   <li>I3. The outcome ({@link Kind#END}, {@link Kind#ERROR} or {@link Kind#CANCEL}) is reported at
 *       most once. It holds the count and the SHA-256 of exactly the bytes that the reader got before
 *       it. END: the reader read the end. ERROR: the reader read the writer failure (after all bytes
 *       before it), or a stall. CANCEL: the reader closed the stream before END or ERROR.
 *   <li>I4. Every wait is bounded by {@code stallMillis}. A wait that runs out breaks the pipe: the
 *       buffered bytes are dropped, the outcome is ERROR with a {@link StalledException}, and both
 *       sides get that error from then on.
 *   <li>I5. The listener is called outside the lock, so it cannot block the other side.
 * </ul>
 */
public final class BridgePipe {

    /** Largest buffer: 1 MiB. */
    public static final int MAX_CAPACITY = 1 << 20;

    /** Longest stall bound: 10 minutes. */
    public static final long MAX_STALL_MILLIS = 600_000L;

    /** How the delivery to the reader ended. */
    public enum Kind {
        END,
        ERROR,
        CANCEL
    }

    /** The outcome of the delivery to the reader. */
    public static final class Outcome {
        public final Kind kind;
        /** Bytes that the reader got. */
        public final long bytes;
        /** SHA-256 of those bytes. */
        public final byte[] sha256;
        /** The error for {@link Kind#ERROR}, else {@code null}. */
        public final Throwable error;

        Outcome(Kind kind, long bytes, byte[] sha256, Throwable error) {
            this.kind = kind;
            this.bytes = bytes;
            this.sha256 = sha256;
            this.error = error;
        }
    }

    /** Callbacks of the pipe. They must not throw. */
    public interface Listener {
        /** One read gave {@code bytes} (at least 1) bytes to the reader. Called on the reader thread. */
        void onRead(int bytes);

        /** The outcome; called once, on the thread that ended the delivery. */
        void onOutcome(Outcome outcome);
    }

    /** The reader closed the stream; the writer must stop. */
    public static final class CancelledException extends IOException {
        private static final long serialVersionUID = 1L;

        CancelledException() {
            super("the reader closed the stream");
        }
    }

    /** No progress within the stall bound. */
    public static final class StalledException extends IOException {
        private static final long serialVersionUID = 1L;

        StalledException(String message) {
            super(message);
        }
    }

    private final Object lock = new Object();
    private final byte[] ring;
    private final long stallNanos;
    private final Listener listener;
    private final MessageDigest digest;
    private final Source source = new Source();

    /** Index of the first buffered byte. Guarded by {@link #lock}. */
    private int head;
    /** Buffered bytes. Guarded by {@link #lock}. */
    private int count;
    /** Bytes given to the reader. Guarded by {@link #lock}. */
    private long delivered;
    /** The writer called {@link #finish}. Guarded by {@link #lock}. */
    private boolean finished;
    /** The writer called {@link #fail}. Guarded by {@link #lock}. */
    private Throwable failure;
    /** A stall broke the pipe. Guarded by {@link #lock}. */
    private StalledException broken;
    /** The reader closed the stream. Guarded by {@link #lock}. */
    private boolean closed;
    /** The outcome is taken. Guarded by {@link #lock}. */
    private boolean reported;

    /**
     * @param capacity buffer size, 1 to {@link #MAX_CAPACITY}
     * @param stallMillis bound of each wait, 1 to {@link #MAX_STALL_MILLIS}
     * @param listener the callbacks
     */
    public BridgePipe(int capacity, long stallMillis, Listener listener) {
        if (capacity < 1 || capacity > MAX_CAPACITY) {
            throw new IllegalArgumentException("capacity must be 1.." + MAX_CAPACITY);
        }
        if (stallMillis < 1 || stallMillis > MAX_STALL_MILLIS) {
            throw new IllegalArgumentException("stallMillis must be 1.." + MAX_STALL_MILLIS);
        }
        if (listener == null) {
            throw new NullPointerException("listener");
        }
        this.ring = new byte[capacity];
        this.stallNanos = stallMillis * 1_000_000L;
        this.listener = listener;
        try {
            this.digest = MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException("SHA-256 is not available", e);
        }
    }

    /** The reader side. The WebView reads it and closes it. */
    public InputStream source() {
        return source;
    }

    /**
     * Gives bytes to the reader. Waits while the pipe is full.
     *
     * @throws CancelledException the reader closed the stream
     * @throws StalledException the pipe is broken by a stall
     * @throws InterruptedIOException the thread was interrupted while it waited
     * @throws IllegalStateException after {@link #finish} or {@link #fail}
     */
    public void write(byte[] buffer, int offset, int length) throws IOException {
        checkRange(buffer, offset, length);
        int done = 0;
        while (done < length) {
            Outcome stalled = null;
            synchronized (lock) {
                if (finished || failure != null) {
                    throw new IllegalStateException("write after the end of the stream");
                }
                long deadline = System.nanoTime() + stallNanos;
                while (count == ring.length && !closed && broken == null) {
                    long left = deadline - System.nanoTime();
                    if (left <= 0) {
                        stalled = breakLocked(new StalledException("the reader made no space"));
                        break;
                    }
                    waitLocked(left);
                }
                if (stalled == null) {
                    if (closed) {
                        throw new CancelledException();
                    }
                    if (broken != null) {
                        throw broken;
                    }
                    int n = Math.min(length - done, ring.length - count);
                    int tail = (head + count) % ring.length;
                    int first = Math.min(n, ring.length - tail);
                    System.arraycopy(buffer, offset + done, ring, tail, first);
                    System.arraycopy(buffer, offset + done + first, ring, 0, n - first);
                    count += n;
                    done += n;
                    lock.notifyAll();
                }
            }
            if (stalled != null) {
                deliver(stalled);
                throw (StalledException) stalled.error;
            }
        }
    }

    /** The writer reached the end. The reader gets the end after the buffered bytes. Once only. */
    public void finish() {
        synchronized (lock) {
            if (finished || failure != null) {
                return;
            }
            finished = true;
            lock.notifyAll();
        }
    }

    /** The writer failed. The reader gets the error after the buffered bytes. Once only. */
    public void fail(Throwable error) {
        if (error == null) {
            throw new NullPointerException("error");
        }
        synchronized (lock) {
            if (finished || failure != null) {
                return;
            }
            failure = error;
            lock.notifyAll();
        }
    }

    /** True when the reader closed the stream. */
    public boolean isClosed() {
        synchronized (lock) {
            return closed;
        }
    }

    /** True when the outcome is reported. */
    public boolean isReported() {
        synchronized (lock) {
            return reported;
        }
    }

    private int read(byte[] buffer, int offset, int length) throws IOException {
        checkRange(buffer, offset, length);
        if (length == 0) {
            return 0;
        }
        Outcome outcome = null;
        IOException error = null;
        int n = 0;
        synchronized (lock) {
            if (closed) {
                throw new IOException("the stream is closed");
            }
            long deadline = System.nanoTime() + stallNanos;
            while (count == 0 && !finished && failure == null && broken == null && !closed) {
                long left = deadline - System.nanoTime();
                if (left <= 0) {
                    outcome = breakLocked(new StalledException("the writer gave no bytes"));
                    break;
                }
                waitLocked(left);
            }
            if (broken != null) {
                error = broken;
            } else if (closed) {
                error = new IOException("the stream is closed");
            } else if (count > 0) {
                n = Math.min(length, count);
                int first = Math.min(n, ring.length - head);
                System.arraycopy(ring, head, buffer, offset, first);
                System.arraycopy(ring, 0, buffer, offset + first, n - first);
                digest.update(buffer, offset, n);
                delivered += n;
                head = (head + n) % ring.length;
                count -= n;
                lock.notifyAll();
            } else if (finished) {
                outcome = takeLocked(Kind.END, null);
                n = -1;
            } else {
                outcome = takeLocked(Kind.ERROR, failure);
                error = new IOException("the fetch failed: " + failure.getClass().getName(), failure);
            }
        }
        deliver(outcome);
        if (error != null) {
            throw error;
        }
        if (n > 0) {
            listener.onRead(n);
        }
        return n;
    }

    private int available() {
        synchronized (lock) {
            return closed || broken != null ? 0 : count;
        }
    }

    private void close() {
        Outcome outcome;
        synchronized (lock) {
            if (closed) {
                return;
            }
            closed = true;
            count = 0;
            outcome = takeLocked(Kind.CANCEL, null);
            lock.notifyAll();
        }
        deliver(outcome);
    }

    /** Breaks the pipe after a stall. Call with the lock held. */
    private Outcome breakLocked(StalledException e) {
        broken = e;
        count = 0;
        lock.notifyAll();
        return takeLocked(Kind.ERROR, e);
    }

    /** Takes the outcome once. Call with the lock held; returns null when it is already taken. */
    private Outcome takeLocked(Kind kind, Throwable error) {
        if (reported) {
            return null;
        }
        reported = true;
        return new Outcome(kind, delivered, digest.digest(), error);
    }

    /** Waits at most {@code nanos} (more than 0). Call with the lock held. */
    private void waitLocked(long nanos) throws InterruptedIOException {
        try {
            lock.wait(nanos / 1_000_000L, (int) (nanos % 1_000_000L));
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new InterruptedIOException("interrupted while waiting for the pipe");
        }
    }

    private void deliver(Outcome outcome) {
        if (outcome != null) {
            listener.onOutcome(outcome);
        }
    }

    private static void checkRange(byte[] buffer, int offset, int length) {
        if (buffer == null) {
            throw new NullPointerException("buffer");
        }
        if (offset < 0 || length < 0 || length > buffer.length - offset) {
            throw new IndexOutOfBoundsException("offset " + offset + ", length " + length + ", size " + buffer.length);
        }
    }

    /** The reader side. */
    private final class Source extends InputStream {
        @Override
        public int read() throws IOException {
            byte[] one = new byte[1];
            int n = BridgePipe.this.read(one, 0, 1);
            return n == -1 ? -1 : one[0] & 0xFF;
        }

        @Override
        public int read(byte[] buffer, int offset, int length) throws IOException {
            return BridgePipe.this.read(buffer, offset, length);
        }

        /** Skipped bytes are read, so that the count and the hash hold every byte that left the pipe. */
        @Override
        public long skip(long n) throws IOException {
            if (n <= 0) {
                return 0;
            }
            byte[] scratch = new byte[(int) Math.min(8192, n)];
            long done = 0;
            while (done < n) {
                int r = BridgePipe.this.read(scratch, 0, (int) Math.min(scratch.length, n - done));
                if (r < 0) {
                    break;
                }
                done += r;
            }
            return done;
        }

        @Override
        public int available() {
            return BridgePipe.this.available();
        }

        @Override
        public boolean markSupported() {
            return false;
        }

        @Override
        public void close() {
            BridgePipe.this.close();
        }
    }
}
