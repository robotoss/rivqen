// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import java.io.IOException;
import java.io.InputStream;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.CyclicBarrier;
import java.util.concurrent.TimeUnit;

/**
 * Plain JVM check of {@link TallyStream} (no Android SDK, no UPSTREAM, no test library). Run with
 * {@code sh tools/upstream-lab/android/check.sh}. Threads meet at latches, never at sleeps; every
 * wait has a 10 s bound, so a defect fails the check and never hangs it.
 */
public final class TallyStreamCheck {

    private static final long WAIT_S = 10;

    /** One reported outcome. */
    static final class Report {
        final long length;
        final byte[] sha256;
        final boolean complete;
        final Throwable error;

        Report(long length, byte[] sha256, boolean complete, Throwable error) {
            this.length = length;
            this.sha256 = sha256;
            this.complete = complete;
            this.error = error;
        }
    }

    /** Collects every report, so a second report is visible. */
    static final class Reports implements TallyStream.Outcome {
        final List<Report> all = Collections.synchronizedList(new ArrayList<Report>());

        @Override
        public void report(long length, byte[] sha256, boolean complete, Throwable error) {
            all.add(new Report(length, sha256, complete, error));
        }

        Report only() {
            check(all.size() == 1, "one report, got " + all.size());
            return all.get(0);
        }
    }

    /** A source error with its own class, to check the class in the report. */
    static final class SourceError extends IOException {
        private static final long serialVersionUID = 1L;

        SourceError() {
            super("source error");
        }
    }

    /**
     * A source that gives {@code first} in one read, then stops the next read at a gate. When the
     * gate opens, that read gives {@code -1}, gives {@code second}, or throws.
     */
    static final class GateSource extends InputStream {
        final CountDownLatch entered = new CountDownLatch(1);
        final CountDownLatch release = new CountDownLatch(1);
        private final byte[] first;
        private final byte[] second;
        private final boolean throwAtGate;
        private int reads;

        GateSource(byte[] first, byte[] second, boolean throwAtGate) {
            this.first = first;
            this.second = second;
            this.throwAtGate = throwAtGate;
        }

        @Override
        public int read() throws IOException {
            byte[] one = new byte[1];
            int n = read(one, 0, 1);
            return n == -1 ? -1 : one[0] & 0xff;
        }

        @Override
        public int read(byte[] b, int off, int len) throws IOException {
            int k = reads++;
            if (k == 0 && first.length > 0) {
                System.arraycopy(first, 0, b, off, first.length);
                return first.length;
            }
            entered.countDown();
            try {
                if (!release.await(WAIT_S, TimeUnit.SECONDS)) {
                    throw new IOException("gate not opened");
                }
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new IOException("interrupted");
            }
            if (throwAtGate) {
                throw new IOException("stream closed");
            }
            if (second == null || reads > 2) {
                return -1;
            }
            System.arraycopy(second, 0, b, off, second.length);
            return second.length;
        }
    }

    /** A thread-safe source that throws on every read after its close, as a socket does. */
    static final class ClosableSource extends InputStream {
        private final byte[] data;
        private int pos;
        private boolean closed;

        ClosableSource(byte[] data) {
            this.data = data;
        }

        @Override
        public synchronized int read() throws IOException {
            if (closed) {
                throw new IOException("closed");
            }
            return pos < data.length ? data[pos++] & 0xff : -1;
        }

        @Override
        public synchronized int read(byte[] b, int off, int len) throws IOException {
            if (closed) {
                throw new IOException("closed");
            }
            if (pos >= data.length) {
                return -1;
            }
            int n = Math.min(len, data.length - pos);
            System.arraycopy(data, pos, b, off, n);
            pos += n;
            return n;
        }

        @Override
        public synchronized void close() {
            closed = true;
        }
    }

    /**
     * A source with one gate per read call and one gate for the close. Read call {@code i} waits at
     * its gate, then gives {@code chunks[i]} (or -1 after the last chunk). The close waits at its
     * gate. Reads still give bytes while the close waits (bytes already in a buffer).
     */
    static final class ScriptSource extends InputStream {
        final CountDownLatch[] entered;
        final CountDownLatch[] release;
        final CountDownLatch closeEntered = new CountDownLatch(1);
        final CountDownLatch closeRelease = new CountDownLatch(1);
        private final byte[][] chunks;
        private int calls;

        ScriptSource(byte[]... chunks) {
            this.chunks = chunks;
            entered = new CountDownLatch[chunks.length + 1];
            release = new CountDownLatch[chunks.length + 1];
            for (int i = 0; i <= chunks.length; i++) {
                entered[i] = new CountDownLatch(1);
                release[i] = new CountDownLatch(1);
            }
        }

        @Override
        public int read() throws IOException {
            throw new IOException("not used");
        }

        @Override
        public int read(byte[] b, int off, int len) throws IOException {
            int k;
            synchronized (this) {
                k = calls++;
            }
            check(k <= chunks.length, "too many reads");
            entered[k].countDown();
            gate(release[k]);
            if (k == chunks.length) {
                return -1;
            }
            System.arraycopy(chunks[k], 0, b, off, chunks[k].length);
            return chunks[k].length;
        }

        @Override
        public void close() throws IOException {
            closeEntered.countDown();
            gate(closeRelease);
        }

        private static void gate(CountDownLatch latch) throws IOException {
            try {
                if (!latch.await(WAIT_S, TimeUnit.SECONDS)) {
                    throw new IOException("gate not opened");
                }
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new IOException("interrupted");
            }
        }
    }

    // ------------------------------------------------------------------------------------------

    /**
     * While the close of the source is still in progress, reads that start and return are counted,
     * and the close reports when it is done: no read can report before the close is done.
     */
    static void readsDuringTheSourceCloseAreCounted() throws Exception {
        byte[] a = bytes(30, 1);
        byte[] b = bytes(40, 2);
        ScriptSource src = new ScriptSource(a, b);
        Reports reports = new Reports();
        final TallyStream s = new TallyStream(src, reports);
        Thread closer = start(() -> s.close());
        await(src.closeEntered);
        src.release[0].countDown();
        check(s.read(new byte[64], 0, 64) == 30, "first read during the close");
        check(reports.all.isEmpty(), "no report before the close is done");
        src.release[1].countDown();
        check(s.read(new byte[64], 0, 64) == 40, "second read during the close");
        check(reports.all.isEmpty(), "no report before the close is done");
        src.closeRelease.countDown();
        join(closer);
        Report r = reports.only();
        check(r.length == 70 && Arrays.equals(r.sha256, sha256(concat(a, b))), "both reads counted: " + r.length);
        check(!r.complete && r.error == null, "a close");
    }

    /** With two reads in flight at the close, the last read that leaves reports, with both reads. */
    static void lastOfTwoReadsReports() throws Exception {
        byte[] a = bytes(11, 4);
        byte[] b = bytes(13, 6);
        ScriptSource src = new ScriptSource(a, b);
        src.closeRelease.countDown();
        Reports reports = new Reports();
        final TallyStream s = new TallyStream(src, reports);
        Thread first = start(() -> s.read(new byte[64], 0, 64));
        await(src.entered[0]);
        Thread second = start(() -> s.read(new byte[64], 0, 64));
        await(src.entered[1]);
        s.close();
        check(reports.all.isEmpty(), "no report while two reads are in flight");
        src.release[0].countDown();
        join(first);
        check(reports.all.isEmpty(), "no report while one read is in flight");
        src.release[1].countDown();
        join(second);
        Report r = reports.only();
        check(r.length == 24 && Arrays.equals(r.sha256, sha256(concat(a, b))), "both reads counted: " + r.length);
        check(!r.complete && r.error == null, "a close");
    }

    /** The S1 race: a close while the last read is in flight must not report without its bytes. */
    static void closeDuringReadLeavesTheReportToTheRead() throws Exception {
        byte[] body = bytes(100, 7);
        GateSource src = new GateSource(new byte[0], body, false);
        Reports reports = new Reports();
        final TallyStream s = new TallyStream(src, reports);
        final int[] got = new int[1];
        Thread reader = start(() -> got[0] = s.read(new byte[256], 0, 256));
        await(src.entered);
        Thread closer = start(() -> s.close());
        join(closer);
        check(reports.all.isEmpty(), "no report while the read is in flight");
        src.release.countDown();
        join(reader);
        check(got[0] == 100, "the reader got the body");
        Report r = reports.only();
        check(r.length == 100, "length counts the in-flight read: " + r.length);
        check(Arrays.equals(r.sha256, sha256(body)), "digest counts the in-flight read");
        check(!r.complete, "no end of stream: complete is false");
        check(r.error == null, "a close is not an error");
    }

    /** The end of the stream that a read reaches during a close is a complete body. */
    static void endDuringCloseIsComplete() throws Exception {
        byte[] body = bytes(10, 3);
        GateSource src = new GateSource(body, null, false);
        Reports reports = new Reports();
        final TallyStream s = new TallyStream(src, reports);
        check(s.read(new byte[64], 0, 64) == 10, "first read");
        Thread reader = start(() -> s.read(new byte[64], 0, 64));
        await(src.entered);
        Thread closer = start(() -> s.close());
        join(closer);
        check(reports.all.isEmpty(), "no report while the read is in flight");
        src.release.countDown();
        join(reader);
        Report r = reports.only();
        check(r.complete, "end of stream: complete");
        check(r.length == 10 && Arrays.equals(r.sha256, sha256(body)), "all bytes");
        check(r.error == null, "no error");
    }

    /** A read that fails because of the close is a close, not a read error. */
    static void readErrorAfterCloseIsAClose() throws Exception {
        byte[] body = bytes(5, 1);
        GateSource src = new GateSource(body, null, true);
        Reports reports = new Reports();
        final TallyStream s = new TallyStream(src, reports);
        check(s.read(new byte[64], 0, 64) == 5, "first read");
        final IOException[] thrown = new IOException[1];
        Thread reader = start(() -> {
            try {
                s.read(new byte[64], 0, 64);
            } catch (IOException e) {
                thrown[0] = e;
            }
        });
        await(src.entered);
        Thread closer = start(() -> s.close());
        join(closer);
        src.release.countDown();
        join(reader);
        check(thrown[0] != null, "the read error reaches the reader");
        Report r = reports.only();
        check(!r.complete && r.error == null, "a close: complete false, no error");
        check(r.length == 5 && Arrays.equals(r.sha256, sha256(body)), "bytes before the close");
    }

    /** A read error without a close is reported with its class. */
    static void readErrorIsReported() {
        InputStream src = new InputStream() {
            @Override
            public int read() throws IOException {
                throw new SourceError();
            }
        };
        Reports reports = new Reports();
        TallyStream s = new TallyStream(src, reports);
        try {
            s.read();
            check(false, "the read must throw");
        } catch (IOException e) {
            check(e instanceof SourceError, "the source error reaches the reader");
        }
        Report r = reports.only();
        check(!r.complete && r.error instanceof SourceError, "error class in the report");
        check(r.length == 0, "no bytes");
    }

    /** A close after the end does not add a second report; single-byte reads are counted. */
    static void closeAfterEndReportsOnce() throws Exception {
        byte[] body = bytes(3, 9);
        Reports reports = new Reports();
        TallyStream s = new TallyStream(new ClosableSource(body), reports);
        int n = 0;
        while (s.read() != -1) {
            n++;
        }
        s.close();
        s.close();
        check(n == 3, "three bytes");
        Report r = reports.only();
        check(r.complete && r.length == 3 && Arrays.equals(r.sha256, sha256(body)), "complete body");
        check(s.isReported(), "reported");
    }

    /** fail() reports the error of the reader once; the close after it adds nothing. */
    static void failReportsOnce() throws IOException {
        Reports reports = new Reports();
        TallyStream s = new TallyStream(new ClosableSource(bytes(20, 2)), reports);
        check(s.read(new byte[8], 0, 8) == 8, "read 8");
        s.fail(new SourceError());
        s.close();
        Report r = reports.only();
        check(!r.complete && r.error instanceof SourceError && r.length == 8, "limit report");
    }

    /** A reset would hash bytes twice, so the stream has no mark. */
    static void resetIsNotSupported() {
        TallyStream s = new TallyStream(new ClosableSource(bytes(4, 0)), new Reports());
        check(!s.markSupported(), "no mark");
        try {
            s.reset();
            check(false, "reset must throw");
        } catch (IOException expected) {
            // The reader gets an error, not bytes for the second time.
        }
    }

    /** skip() reads the skipped bytes, so they are in the digest. */
    static void skipIsCounted() throws Exception {
        byte[] body = bytes(20000, 5);
        Reports reports = new Reports();
        TallyStream s = new TallyStream(new ClosableSource(body), reports);
        check(s.skip(15000) == 15000, "skip 15000");
        check(s.skip(-1) == 0, "negative skip");
        while (s.read(new byte[4096], 0, 4096) != -1) {
            // read to the end
        }
        Report r = reports.only();
        check(r.complete && r.length == 20000 && Arrays.equals(r.sha256, sha256(body)), "all bytes");
    }

    /**
     * Free race of one reader and one closer, many rounds. In every round: exactly one report; its
     * length is the number of bytes that the reader got; its digest is the digest of those bytes;
     * complete is true exactly when the reader got -1.
     */
    static void raceKeepsTheInvariants() throws Exception {
        final byte[] body = bytes(64 * 1024, 11);
        for (int round = 0; round < 3000; round++) {
            final Reports reports = new Reports();
            final TallyStream s = new TallyStream(new ClosableSource(body), reports);
            final CyclicBarrier go = new CyclicBarrier(2);
            final long[] got = new long[1];
            final boolean[] end = new boolean[1];
            final int chunk = 1 + round % 4096;
            final int closeAfter = round % 7;
            Thread reader = start(() -> {
                go.await(WAIT_S, TimeUnit.SECONDS);
                byte[] buf = new byte[chunk];
                try {
                    for (int i = 0; ; i++) {
                        int n = (i % 3 == 0) ? s.read() : s.read(buf, 0, buf.length);
                        if (n == -1) {
                            end[0] = true;
                            return;
                        }
                        got[0] += (i % 3 == 0) ? 1 : n;
                    }
                } catch (IOException closed) {
                    // The closer won: the source throws after its close.
                }
            });
            Thread closer = start(() -> {
                go.await(WAIT_S, TimeUnit.SECONDS);
                for (int i = 0; i < closeAfter; i++) {
                    Thread.yield();
                }
                s.close();
            });
            join(reader);
            join(closer);
            Report r = reports.only();
            check(r.length == got[0], "round " + round + ": length " + r.length + " != read " + got[0]);
            check(Arrays.equals(r.sha256, sha256(Arrays.copyOf(body, (int) got[0]))), "round " + round + ": digest");
            check(r.complete == end[0], "round " + round + ": complete " + r.complete + " != end " + end[0]);
            check(r.error == null, "round " + round + ": a close is not an error");
        }
    }

    // ------------------------------------------------------------------------------------------

    interface Body {
        void run() throws Exception;
    }

    private static final List<Throwable> THREAD_ERRORS = Collections.synchronizedList(new ArrayList<Throwable>());

    private static Thread start(final Body body) {
        Thread t = new Thread(() -> {
            try {
                body.run();
            } catch (Throwable e) {
                THREAD_ERRORS.add(e);
            }
        });
        t.setDaemon(true);
        t.start();
        return t;
    }

    private static void join(Thread t) throws InterruptedException {
        t.join(TimeUnit.SECONDS.toMillis(WAIT_S));
        check(!t.isAlive(), "thread did not end in " + WAIT_S + " s");
        if (!THREAD_ERRORS.isEmpty()) {
            Throwable e = THREAD_ERRORS.get(0);
            throw new AssertionError("thread failed: " + e, e);
        }
    }

    private static void await(CountDownLatch latch) throws InterruptedException {
        check(latch.await(WAIT_S, TimeUnit.SECONDS), "latch not reached in " + WAIT_S + " s");
    }

    static void check(boolean ok, String what) {
        if (!ok) {
            throw new AssertionError(what);
        }
    }

    private static byte[] bytes(int n, int seed) {
        byte[] b = new byte[n];
        for (int i = 0; i < n; i++) {
            b[i] = (byte) (i * 31 + seed);
        }
        return b;
    }

    private static byte[] concat(byte[] a, byte[] b) {
        byte[] c = Arrays.copyOf(a, a.length + b.length);
        System.arraycopy(b, 0, c, a.length, b.length);
        return c;
    }

    private static byte[] sha256(byte[] b) throws Exception {
        return MessageDigest.getInstance("SHA-256").digest(b);
    }

    public static void main(String[] args) throws Exception {
        String[] names = {
            "closeDuringReadLeavesTheReportToTheRead", "endDuringCloseIsComplete", "readErrorAfterCloseIsAClose",
            "readsDuringTheSourceCloseAreCounted", "lastOfTwoReadsReports", "readErrorIsReported", "closeAfterEndReportsOnce", "failReportsOnce", "resetIsNotSupported",
            "skipIsCounted", "raceKeepsTheInvariants",
        };
        int failed = 0;
        for (String name : names) {
            try {
                TallyStreamCheck.class.getDeclaredMethod(name).invoke(null);
                System.out.println("ok " + name);
            } catch (java.lang.reflect.InvocationTargetException e) {
                failed++;
                System.out.println("FAIL " + name + ": " + e.getCause());
            }
            THREAD_ERRORS.clear();
        }
        if (failed > 0) {
            System.out.println(failed + " of " + names.length + " checks failed");
            System.exit(1);
        }
        System.out.println(names.length + " checks passed");
    }
}
