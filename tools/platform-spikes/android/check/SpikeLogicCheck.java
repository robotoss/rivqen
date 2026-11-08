// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package dev.rivqen.spike;

import java.io.IOException;
import java.io.InputStream;
import java.io.InterruptedIOException;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.Set;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Plain JVM check of {@link BridgePipe} and {@link InterceptRule} (no Android SDK, no test library;
 * DL-139). Run with {@code sh tools/platform-spikes/android/check.sh}. Threads meet at latches;
 * every wait has a bound, so a defect fails the check and never hangs it.
 */
public final class SpikeLogicCheck {

    private static final long WAIT_S = 10;
    private static final long LONG_STALL_MS = 60_000;
    private static int checks;
    private static final List<String> failures = new ArrayList<String>();

    /** Records every callback, so a second outcome is visible. */
    static final class Recorder implements BridgePipe.Listener {
        final List<BridgePipe.Outcome> outcomes = Collections.synchronizedList(new ArrayList<BridgePipe.Outcome>());
        final AtomicInteger readBytes = new AtomicInteger();
        final AtomicInteger reads = new AtomicInteger();

        @Override
        public void onRead(int bytes) {
            reads.incrementAndGet();
            readBytes.addAndGet(bytes);
        }

        @Override
        public void onOutcome(BridgePipe.Outcome outcome) {
            outcomes.add(outcome);
        }

        BridgePipe.Outcome only() {
            check(outcomes.size() == 1, "one outcome, got " + outcomes.size());
            return outcomes.isEmpty() ? null : outcomes.get(0);
        }
    }

    /** A writer error with its own class. */
    static final class SourceError extends IOException {
        private static final long serialVersionUID = 1L;

        SourceError() {
            super("source error");
        }
    }

    public static void main(String[] args) throws Exception {
        run("pipe_end_gives_bytes_unchanged", SpikeLogicCheck::pipeEnd);
        run("pipe_wraps_and_applies_back_pressure", SpikeLogicCheck::pipeBackPressure);
        run("pipe_error_after_buffered_bytes", SpikeLogicCheck::pipeError);
        run("pipe_cancel_by_close", SpikeLogicCheck::pipeCancel);
        run("pipe_close_wakes_blocked_writer", SpikeLogicCheck::pipeCloseWakesWriter);
        run("pipe_close_wakes_blocked_reader", SpikeLogicCheck::pipeCloseWakesReader);
        run("pipe_close_after_end_keeps_end", SpikeLogicCheck::pipeCloseAfterEnd);
        run("pipe_reader_stall_breaks", SpikeLogicCheck::pipeReaderStall);
        run("pipe_writer_stall_breaks", SpikeLogicCheck::pipeWriterStall);
        run("pipe_single_byte_read_and_skip", SpikeLogicCheck::pipeSingleByteAndSkip);
        run("pipe_arguments", SpikeLogicCheck::pipeArguments);
        run("pipe_interrupt_gives_no_outcome", SpikeLogicCheck::pipeInterrupt);
        run("pipe_close_races_last_read_once_only", SpikeLogicCheck::pipeCloseRace);
        run("rule_streams_once_on_exact_match", SpikeLogicCheck::ruleMatch);
        run("rule_passes_other_requests", SpikeLogicCheck::rulePass);
        run("rule_origin_form", SpikeLogicCheck::ruleOrigin);
        run("head_status_reason_and_type", SpikeLogicCheck::headStatus);
        run("head_fields", SpikeLogicCheck::headFields);
        run("media_type", SpikeLogicCheck::mediaType);
        run("loggable_values", SpikeLogicCheck::loggable);
        if (!failures.isEmpty()) {
            for (String f : failures) {
                System.out.println("FAIL " + f);
            }
            System.out.println("spike-logic-check: " + failures.size() + " of " + checks + " checks failed");
            System.exit(1);
        }
        System.out.println("spike-logic-check: " + checks + " checks passed");
    }

    interface Body {
        void run() throws Exception;
    }

    private static String current = "";

    private static void run(String name, Body body) {
        current = name;
        try {
            body.run();
        } catch (Exception | AssertionError e) {
            failures.add(name + ": unexpected " + e);
        }
    }

    static void check(boolean ok, String what) {
        checks++;
        if (!ok) {
            failures.add(current + ": " + what);
        }
    }

    // ---- BridgePipe ----

    private static byte[] data(int n, long seed) {
        byte[] b = new byte[n];
        new Random(seed).nextBytes(b);
        return b;
    }

    private static byte[] sha(byte[] b, int len) throws Exception {
        MessageDigest md = MessageDigest.getInstance("SHA-256");
        md.update(b, 0, len);
        return md.digest();
    }

    /** Reads until the end with a buffer of {@code chunk} bytes. */
    private static byte[] drain(InputStream in, int chunk) throws IOException {
        java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
        byte[] buf = new byte[chunk];
        int n;
        while ((n = in.read(buf, 0, buf.length)) != -1) {
            check(n > 0, "a read returns at least one byte or -1, got " + n);
            out.write(buf, 0, n);
        }
        return out.toByteArray();
    }

    private static Thread start(final Body body, final AtomicReference<Throwable> error) {
        Thread t = new Thread(new Runnable() {
            @Override
            public void run() {
                try {
                    body.run();
                } catch (Throwable e) {
                    error.set(e);
                }
            }
        });
        t.setDaemon(true);
        t.start();
        return t;
    }

    private static void pipeEnd() throws Exception {
        Recorder r = new Recorder();
        BridgePipe p = new BridgePipe(64, LONG_STALL_MS, r);
        byte[] d = data(50, 1);
        p.write(d, 0, 20);
        p.write(d, 20, 30);
        p.finish();
        byte[] got = drain(p.source(), 7);
        check(Arrays.equals(got, d), "the reader gets the written bytes unchanged");
        check(r.outcomes.size() == 1, "one outcome at the end");
        BridgePipe.Outcome o = r.only();
        check(o != null && o.kind == BridgePipe.Kind.END, "END outcome");
        check(o != null && o.bytes == 50, "outcome count 50");
        check(o != null && Arrays.equals(o.sha256, sha(d, 50)), "outcome SHA-256 of the delivered bytes");
        check(o != null && o.error == null, "END has no error");
        check(r.readBytes.get() == 50, "onRead total equals the delivered bytes");
        check(p.source().read(new byte[4], 0, 4) == -1, "a read after the end gives -1 again");
        p.source().close();
        check(r.outcomes.size() == 1, "no second outcome after more reads and a close");
        check(p.isReported(), "isReported after the end");
    }

    private static void pipeBackPressure() throws Exception {
        final Recorder r = new Recorder();
        final BridgePipe p = new BridgePipe(5, LONG_STALL_MS, r);
        final byte[] d = data(1000, 2);
        AtomicReference<Throwable> err = new AtomicReference<Throwable>();
        Thread w = start(new Body() {
            @Override
            public void run() throws Exception {
                for (int i = 0; i < d.length; i += 13) {
                    p.write(d, i, Math.min(13, d.length - i));
                }
                p.finish();
            }
        }, err);
        java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
        byte[] buf = new byte[3];
        int maxAvailable = 0;
        int n;
        while (true) {
            maxAvailable = Math.max(maxAvailable, p.source().available());
            n = p.source().read(buf, 0, buf.length);
            if (n == -1) {
                break;
            }
            out.write(buf, 0, n);
        }
        w.join(TimeUnit.SECONDS.toMillis(WAIT_S));
        check(!w.isAlive(), "the writer ends");
        check(err.get() == null, "the writer has no error: " + err.get());
        check(maxAvailable <= 5, "at most the capacity is buffered, saw " + maxAvailable);
        check(Arrays.equals(out.toByteArray(), d), "1000 bytes through a 5-byte ring arrive unchanged");
        BridgePipe.Outcome o = r.only();
        check(o != null && o.kind == BridgePipe.Kind.END && o.bytes == 1000, "END with 1000 bytes");
        check(o != null && Arrays.equals(o.sha256, sha(d, d.length)), "hash over all bytes");
    }

    private static void pipeError() throws Exception {
        Recorder r = new Recorder();
        BridgePipe p = new BridgePipe(64, LONG_STALL_MS, r);
        byte[] d = data(10, 3);
        p.write(d, 0, 10);
        SourceError cause = new SourceError();
        p.fail(cause);
        p.fail(new SourceError());
        p.finish();
        byte[] buf = new byte[64];
        int n = p.source().read(buf, 0, 64);
        check(n == 10, "the buffered bytes come before the error, got " + n);
        check(r.outcomes.isEmpty(), "no outcome while bytes remain");
        IOException thrown = null;
        try {
            p.source().read(buf, 0, 64);
        } catch (IOException e) {
            thrown = e;
        }
        check(thrown != null && thrown.getCause() == cause, "the reader gets the first writer error as the cause");
        BridgePipe.Outcome o = r.only();
        check(o != null && o.kind == BridgePipe.Kind.ERROR, "ERROR outcome");
        check(o != null && o.error == cause, "outcome error is the first writer error");
        check(o != null && o.bytes == 10 && Arrays.equals(o.sha256, sha(d, 10)), "outcome holds the 10 delivered bytes");
        boolean again = false;
        try {
            p.source().read(buf, 0, 64);
        } catch (IOException e) {
            again = true;
        }
        check(again, "a second read throws again");
        p.source().close();
        check(r.outcomes.size() == 1, "one outcome after a second read and a close");
        boolean ise = false;
        try {
            p.write(d, 0, 1);
        } catch (IllegalStateException e) {
            ise = true;
        }
        check(ise, "write after fail is a programming error");
    }

    private static void pipeCancel() throws Exception {
        Recorder r = new Recorder();
        BridgePipe p = new BridgePipe(64, LONG_STALL_MS, r);
        byte[] d = data(5, 4);
        p.write(d, 0, 5);
        byte[] buf = new byte[3];
        check(p.source().read(buf, 0, 3) == 3, "read 3 of 5");
        p.source().close();
        check(p.isClosed(), "isClosed after close");
        BridgePipe.Outcome o = r.only();
        check(o != null && o.kind == BridgePipe.Kind.CANCEL, "CANCEL outcome");
        check(o != null && o.bytes == 3 && Arrays.equals(o.sha256, sha(d, 3)), "CANCEL holds the 3 delivered bytes");
        boolean cancelled = false;
        try {
            p.write(d, 0, 1);
        } catch (BridgePipe.CancelledException e) {
            cancelled = true;
        }
        check(cancelled, "the writer gets CancelledException after the close");
        boolean closedRead = false;
        try {
            p.source().read(buf, 0, 1);
        } catch (IOException e) {
            closedRead = true;
        }
        check(closedRead, "a read after close throws");
        check(p.source().available() == 0, "available is 0 after close");
        p.source().close();
        p.finish();
        check(r.outcomes.size() == 1, "a second close and a finish give no second outcome");
    }

    private static void pipeCloseWakesWriter() throws Exception {
        Recorder r = new Recorder();
        final BridgePipe p = new BridgePipe(4, LONG_STALL_MS, r);
        final byte[] d = data(16, 5);
        final CountDownLatch wrote = new CountDownLatch(1);
        AtomicReference<Throwable> err = new AtomicReference<Throwable>();
        Thread w = start(new Body() {
            @Override
            public void run() throws Exception {
                p.write(d, 0, 4);
                wrote.countDown();
                p.write(d, 4, 12);
            }
        }, err);
        check(wrote.await(WAIT_S, TimeUnit.SECONDS), "the first write fills the pipe");
        waitBlocked(w);
        p.source().close();
        w.join(TimeUnit.SECONDS.toMillis(WAIT_S));
        check(!w.isAlive(), "the blocked writer wakes up");
        check(err.get() instanceof BridgePipe.CancelledException, "the blocked writer gets CancelledException, got " + err.get());
        BridgePipe.Outcome o = r.only();
        check(o != null && o.kind == BridgePipe.Kind.CANCEL && o.bytes == 0, "CANCEL with 0 bytes");
    }

    private static void pipeCloseWakesReader() throws Exception {
        Recorder r = new Recorder();
        final BridgePipe p = new BridgePipe(4, LONG_STALL_MS, r);
        AtomicReference<Throwable> err = new AtomicReference<Throwable>();
        Thread reader = start(new Body() {
            @Override
            public void run() throws Exception {
                p.source().read(new byte[4], 0, 4);
            }
        }, err);
        waitBlocked(reader);
        p.source().close();
        reader.join(TimeUnit.SECONDS.toMillis(WAIT_S));
        check(!reader.isAlive(), "the blocked reader wakes up");
        check(err.get() instanceof IOException, "the blocked reader gets an IOException, got " + err.get());
        BridgePipe.Outcome o = r.only();
        check(o != null && o.kind == BridgePipe.Kind.CANCEL, "CANCEL outcome");
    }

    private static void pipeCloseAfterEnd() throws Exception {
        Recorder r = new Recorder();
        BridgePipe p = new BridgePipe(8, LONG_STALL_MS, r);
        p.write(new byte[] {1, 2}, 0, 2);
        p.finish();
        drain(p.source(), 8);
        p.source().close();
        BridgePipe.Outcome o = r.only();
        check(o != null && o.kind == BridgePipe.Kind.END && o.bytes == 2, "END stays the outcome after close");
    }

    private static void pipeReaderStall() throws Exception {
        Recorder r = new Recorder();
        BridgePipe p = new BridgePipe(8, 50, r);
        long t0 = System.nanoTime();
        IOException thrown = null;
        try {
            p.source().read(new byte[4], 0, 4);
        } catch (IOException e) {
            thrown = e;
        }
        long ms = (System.nanoTime() - t0) / 1_000_000L;
        check(thrown instanceof BridgePipe.StalledException, "a reader without bytes gets StalledException, got " + thrown);
        check(ms >= 45, "the stall waits about the bound, waited " + ms + " ms");
        BridgePipe.Outcome o = r.only();
        check(o != null && o.kind == BridgePipe.Kind.ERROR && o.error instanceof BridgePipe.StalledException, "ERROR outcome with the stall");
        boolean writerSees = false;
        try {
            p.write(new byte[1], 0, 1);
        } catch (BridgePipe.StalledException e) {
            writerSees = true;
        }
        check(writerSees, "the writer gets the stall too");
        p.source().close();
        check(r.outcomes.size() == 1, "close after a stall gives no second outcome");
    }

    private static void pipeWriterStall() throws Exception {
        Recorder r = new Recorder();
        BridgePipe p = new BridgePipe(4, 50, r);
        byte[] d = data(8, 6);
        IOException thrown = null;
        try {
            p.write(d, 0, 8);
        } catch (IOException e) {
            thrown = e;
        }
        check(thrown instanceof BridgePipe.StalledException, "a writer with no reader gets StalledException, got " + thrown);
        BridgePipe.Outcome o = r.only();
        check(o != null && o.kind == BridgePipe.Kind.ERROR && o.bytes == 0, "ERROR outcome with 0 bytes");
        boolean readerSees = false;
        try {
            p.source().read(new byte[8], 0, 8);
        } catch (BridgePipe.StalledException e) {
            readerSees = true;
        }
        check(readerSees, "the reader gets the stall and no dropped bytes");
        check(r.outcomes.size() == 1, "one outcome");
    }

    private static void pipeSingleByteAndSkip() throws Exception {
        Recorder r = new Recorder();
        BridgePipe p = new BridgePipe(16, LONG_STALL_MS, r);
        byte[] d = {(byte) 0xFF, 0, 7, 8, 9, 10};
        p.write(d, 0, d.length);
        p.finish();
        InputStream in = p.source();
        check(in.read() == 255, "read() gives the byte as 0..255");
        check(in.read() == 0, "read() gives 0");
        check(in.skip(0) == 0 && in.skip(-3) == 0, "skip of 0 or less skips nothing");
        check(in.skip(3) == 3, "skip reads 3 bytes");
        check(in.read() == 10, "the byte after the skipped bytes");
        check(in.skip(5) == 0, "skip at the end gives 0");
        check(in.read() == -1, "read() gives -1 at the end");
        check(!in.markSupported(), "no mark");
        BridgePipe.Outcome o = r.only();
        check(o != null && o.bytes == 6 && Arrays.equals(o.sha256, sha(d, 6)), "skipped bytes count in the hash");
    }

    private static void pipeArguments() throws Exception {
        Recorder r = new Recorder();
        check(throwsType(IllegalArgumentException.class, () -> new BridgePipe(0, 10, r)), "capacity 0 is refused");
        check(throwsType(IllegalArgumentException.class, () -> new BridgePipe(BridgePipe.MAX_CAPACITY + 1, 10, r)), "capacity above the limit is refused");
        check(throwsType(IllegalArgumentException.class, () -> new BridgePipe(4, 0, r)), "stall 0 is refused");
        check(throwsType(IllegalArgumentException.class, () -> new BridgePipe(4, BridgePipe.MAX_STALL_MILLIS + 1, r)), "stall above the limit is refused");
        check(throwsType(NullPointerException.class, () -> new BridgePipe(4, 10, null)), "a null listener is refused");
        new BridgePipe(BridgePipe.MAX_CAPACITY, BridgePipe.MAX_STALL_MILLIS, r);
        final BridgePipe p = new BridgePipe(4, LONG_STALL_MS, r);
        check(throwsType(IndexOutOfBoundsException.class, () -> p.source().read(new byte[4], 2, 3)), "a read range past the end is refused");
        check(throwsType(IndexOutOfBoundsException.class, () -> p.source().read(new byte[4], -1, 1)), "a negative offset is refused");
        check(throwsType(IndexOutOfBoundsException.class, () -> p.write(new byte[4], 1, 4)), "a write range past the end is refused");
        check(throwsType(NullPointerException.class, () -> p.write(null, 0, 0)), "a null write buffer is refused");
        check(throwsType(NullPointerException.class, () -> p.fail(null)), "a null error is refused");
        check(p.source().read(new byte[4], 0, 0) == 0, "a read of 0 bytes gives 0 and does not wait");
        p.write(new byte[0], 0, 0);
        p.finish();
        check(throwsType(IllegalStateException.class, () -> p.write(new byte[1], 0, 1)), "write after finish is refused");
        check(r.outcomes.isEmpty(), "argument errors give no outcome");
    }

    private static void pipeInterrupt() throws Exception {
        final Recorder r = new Recorder();
        final BridgePipe p = new BridgePipe(4, LONG_STALL_MS, r);
        AtomicReference<Throwable> err = new AtomicReference<Throwable>();
        final AtomicReference<Boolean> flag = new AtomicReference<Boolean>();
        Thread reader = start(new Body() {
            @Override
            public void run() throws Exception {
                try {
                    p.source().read(new byte[4], 0, 4);
                } finally {
                    flag.set(Thread.currentThread().isInterrupted());
                }
            }
        }, err);
        waitBlocked(reader);
        reader.interrupt();
        reader.join(TimeUnit.SECONDS.toMillis(WAIT_S));
        check(err.get() instanceof InterruptedIOException, "an interrupted read throws InterruptedIOException, got " + err.get());
        check(Boolean.TRUE.equals(flag.get()), "the interrupt flag stays set");
        check(r.outcomes.isEmpty(), "an interrupt is not an outcome");
        p.write(new byte[] {4}, 0, 1);
        p.finish();
        check(drain(p.source(), 4).length == 1, "the pipe still works after the interrupt");
        check(r.only() != null, "END after the interrupt");
    }

    private static void pipeCloseRace() throws Exception {
        for (int i = 0; i < 300; i++) {
            final Recorder r = new Recorder();
            final BridgePipe p = new BridgePipe(8, LONG_STALL_MS, r);
            final byte[] d = data(8, i);
            p.write(d, 0, 8);
            p.finish();
            final CountDownLatch go = new CountDownLatch(1);
            AtomicReference<Throwable> err = new AtomicReference<Throwable>();
            Thread reader = start(new Body() {
                @Override
                public void run() throws Exception {
                    go.await(WAIT_S, TimeUnit.SECONDS);
                    byte[] buf = new byte[8];
                    while (p.source().read(buf, 0, 3) != -1) {
                        // drain
                    }
                }
            }, err);
            Thread closer = start(new Body() {
                @Override
                public void run() throws Exception {
                    go.await(WAIT_S, TimeUnit.SECONDS);
                    p.source().close();
                }
            }, new AtomicReference<Throwable>());
            go.countDown();
            reader.join(TimeUnit.SECONDS.toMillis(WAIT_S));
            closer.join(TimeUnit.SECONDS.toMillis(WAIT_S));
            if (r.outcomes.size() != 1) {
                check(false, "iteration " + i + ": " + r.outcomes.size() + " outcomes");
                return;
            }
            BridgePipe.Outcome o = r.outcomes.get(0);
            boolean consistent = o.kind == BridgePipe.Kind.END ? o.bytes == 8 : o.kind == BridgePipe.Kind.CANCEL;
            if (!consistent || !Arrays.equals(o.sha256, sha(d, (int) o.bytes)) || r.readBytes.get() != o.bytes) {
                check(false, "iteration " + i + ": outcome " + o.kind + " with " + o.bytes + " bytes, reads " + r.readBytes.get());
                return;
            }
        }
        check(true, "300 close and read races give one consistent outcome each");
    }

    /** Waits until the thread blocks in a wait (bounded). */
    private static void waitBlocked(Thread t) throws InterruptedException {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(WAIT_S);
        while (System.nanoTime() < deadline) {
            Thread.State s = t.getState();
            if (s == Thread.State.WAITING || s == Thread.State.TIMED_WAITING) {
                return;
            }
            Thread.sleep(1);
        }
        check(false, "thread did not block");
    }

    interface Thrower {
        void run() throws Exception;
    }

    private static boolean throwsType(Class<? extends Throwable> type, Thrower t) {
        try {
            t.run();
        } catch (Throwable e) {
            return type.isInstance(e);
        }
        return false;
    }

    // ---- InterceptRule ----

    private static final String URL = "https://localhost:18443/s/stream?run=CAP-A-01-android-37-1&chunks=4";
    private static final Set<String> ALLOWED = new HashSet<String>(Arrays.asList("https://localhost:18443", "https://10.0.2.2:18443"));

    private static void ruleMatch() {
        InterceptRule rule = new InterceptRule(URL, "https://localhost:18443", ALLOWED);
        InterceptRule.Verdict v = rule.decide("GET", URL, true, false);
        check(v.decision == InterceptRule.Decision.STREAM && "match".equals(v.reason), "exact match streams");
        InterceptRule.Verdict again = rule.decide("GET", URL, true, false);
        check(again.decision == InterceptRule.Decision.PASS && "claimed".equals(again.reason), "the second match passes (once only)");
    }

    private static void rulePass() {
        String[][] cases = {
            {"GET", URL, "false", "false", "not-main-frame"},
            {"POST", URL, "true", "false", "method"},
            {"get", URL, "true", "false", "method"},
            {"GET", URL, "true", "true", "redirect"},
            {"GET", URL + "&x=1", "true", "false", "url"},
            {"GET", URL.replace("localhost", "LOCALHOST"), "true", "false", "url"},
            {"GET", "https://10.0.2.2:18443/s/stream?run=CAP-A-01-android-37-1&chunks=4", "true", "false", "url"},
        };
        for (String[] c : cases) {
            InterceptRule rule = new InterceptRule(URL, "https://localhost:18443", ALLOWED);
            InterceptRule.Verdict v = rule.decide(c[0], c[1], Boolean.parseBoolean(c[2]), Boolean.parseBoolean(c[3]));
            check(v.decision == InterceptRule.Decision.PASS && c[4].equals(v.reason),
                    "pass for " + c[4] + " (" + c[0] + " " + c[1] + "), got " + v.decision + " " + v.reason);
            check(rule.decide("GET", URL, true, false).decision == InterceptRule.Decision.STREAM,
                    "a pass does not claim the early fetch (" + c[4] + ")");
        }
        InterceptRule foreign = new InterceptRule(URL, "https://localhost:18444", ALLOWED);
        InterceptRule.Verdict v = foreign.decide("GET", URL, true, false);
        check(v.decision == InterceptRule.Decision.PASS && "origin".equals(v.reason), "an origin outside the allow list passes");
        check(throwsType(NullPointerException.class, () -> new InterceptRule(null, "o", ALLOWED)), "null URL refused");
    }

    private static void ruleOrigin() {
        check("https://localhost:18443".equals(InterceptRule.origin("HTTPS", "LocalHost", 18443)), "lower case, explicit port");
        check("https://example.test".equals(InterceptRule.origin("https", "example.test", 443)), "default https port is left out");
        check("http://example.test".equals(InterceptRule.origin("http", "example.test", 80)), "default http port is left out");
        check("http://example.test:443".equals(InterceptRule.origin("http", "example.test", 443)), "443 is not the http default");
        check("https://[::1]:18443".equals(InterceptRule.origin("https", "::1", 18443)), "IPv6 host gets brackets");
        check("https://[::1]:18443".equals(InterceptRule.origin("https", "[::1]", 18443)), "bracketed IPv6 host stays");
    }

    private static List<String[]> pairs(String... nv) {
        List<String[]> out = new ArrayList<String[]>();
        for (int i = 0; i < nv.length; i += 2) {
            out.add(new String[] {nv[i], nv[i + 1]});
        }
        return out;
    }

    private static void headStatus() {
        List<String[]> html = pairs("Content-Type", "text/html; charset=utf-8");
        InterceptRule.Head h = InterceptRule.head(200, "OK", html);
        check(h != null && "text/html".equals(h.mimeType) && "utf-8".equals(h.encoding) && h.status == 200 && "OK".equals(h.reason),
                "200 OK text/html utf-8");
        int[] ok = {100, 200, 299, 400, 404, 599};
        for (int s : ok) {
            check(InterceptRule.head(s, "X", html) != null, "status " + s + " is accepted");
        }
        int[] bad = {99, 300, 301, 302, 307, 308, 399, 600, 0, -1};
        for (int s : bad) {
            check(InterceptRule.head(s, "X", html) == null, "status " + s + " is refused");
        }
        check(InterceptRule.head(200, "", html) == null, "empty reason is refused");
        check(InterceptRule.head(200, "  ", html) == null, "blank reason is refused");
        check(InterceptRule.head(200, null, html) == null, "null reason is refused");
        check(InterceptRule.head(200, "OK", pairs("Cache-Control", "no-store")) == null, "no Content-Type is refused");
        check(InterceptRule.head(200, "OK", pairs("Content-Type", "html")) == null, "a Content-Type without a subtype is refused");
    }

    private static void headFields() {
        InterceptRule.Head h = InterceptRule.head(200, "OK", pairs(
                "Content-Type", "text/html; charset=utf-8",
                "Connection", "keep-alive, X-Hop",
                "Keep-Alive", "timeout=5",
                "Transfer-Encoding", "chunked",
                "Content-Length", "12",
                "X-Hop", "1",
                "Content-Security-Policy", "script-src 'nonce-a'",
                "Vary", "Accept",
                "vary", "Cookie",
                "Set-Cookie", "a=1",
                "Set-Cookie", "b=2",
                "Date", "Sun, 08 Nov 2026 10:00:00 GMT"));
        check(h != null, "head accepted");
        if (h == null) {
            return;
        }
        Map<String, String> f = h.fields;
        check(!f.containsKey("Connection") && !f.containsKey("Keep-Alive") && !f.containsKey("Transfer-Encoding"),
                "hop-by-hop fields are removed");
        check(!f.containsKey("Content-Length"), "Content-Length is removed (the body is re-framed)");
        check(!f.containsKey("X-Hop"), "a field named in Connection is removed");
        check("script-src 'nonce-a'".equals(f.get("Content-Security-Policy")), "CSP passes unchanged");
        check("Accept, Cookie".equals(f.get("Vary")), "repeated fields are joined with a comma, first name kept");
        check("a=1".equals(f.get("Set-Cookie")) && h.dropped.equals(Collections.singletonList("Set-Cookie")),
                "a second Set-Cookie is dropped and listed");
        check(new ArrayList<String>(f.keySet()).equals(Arrays.asList(
                "Content-Type", "Content-Security-Policy", "Vary", "Set-Cookie", "Date")), "received order kept: " + f.keySet());
        check(throwsType(UnsupportedOperationException.class, () -> h.fields.put("x", "y")), "fields are read-only");
    }

    private static void mediaType() {
        check(Arrays.equals(new String[] {"text/html", "utf-8"}, InterceptRule.mediaType("Text/HTML; Charset=\"UTF-8\"")), "quoted charset");
        check(Arrays.equals(new String[] {"text/html", null}, InterceptRule.mediaType("text/html")), "no charset");
        check(Arrays.equals(new String[] {"text/html", null}, InterceptRule.mediaType("text/html; q=1")), "other parameter");
        check(Arrays.equals(new String[] {"text/html", null}, InterceptRule.mediaType("text/html; charset=")), "empty charset");
        check(Arrays.equals(new String[] {"text/html", null}, InterceptRule.mediaType("text/html; charset=\"a b\"")), "bad charset");
        check(InterceptRule.mediaType(null) == null, "null");
        check(InterceptRule.mediaType("") == null, "empty");
        check(InterceptRule.mediaType("/html") == null, "no type");
        check(InterceptRule.mediaType("text/") == null, "no subtype");
        check(InterceptRule.mediaType("te xt/html") == null, "space in the type");
    }

    private static void loggable() {
        check("a-1=b-2; Path=/; HttpOnly".equals(InterceptRule.loggable("Set-Cookie", "a-1=b-2; Path=/; HttpOnly")), "synthetic Set-Cookie kept");
        check("[redacted]".equals(InterceptRule.loggable("set-cookie", "SID=AbC123; Path=/")), "other Set-Cookie redacted");
        check("a=b; c=d".equals(InterceptRule.loggable("Cookie", "a=b; c=d")), "synthetic Cookie kept");
        check("[redacted]".equals(InterceptRule.loggable("Cookie", "a=b; SID=X")), "Cookie with one real pair redacted");
        check("[redacted]".equals(InterceptRule.loggable("Authorization", "Bearer x")), "Authorization redacted");
        check("text/html".equals(InterceptRule.loggable("Content-Type", "text/html")), "other fields unchanged");
    }
}
