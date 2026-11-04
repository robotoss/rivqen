// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.os.Handler;
import android.os.Looper;

import java.util.concurrent.Callable;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicReference;

/** Runs a harness call on the main thread or on a named worker thread, with a time limit. */
final class Threads {

    private static final Handler MAIN = new Handler(Looper.getMainLooper());

    private Threads() {
    }

    /** Runs {@code call} on the main thread and waits at most {@code timeoutMs} for it. */
    static <T> T onMain(Callable<T> call, long timeoutMs) throws Exception {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return call.call();
        }
        Waiter<T> w = new Waiter<>(call);
        MAIN.post(w);
        return w.await(timeoutMs, "main thread");
    }

    /**
     * Starts {@code call} on a new thread named {@code name} and waits at most {@code waitMs}.
     * @return the result, or {@code null} when the call still runs (it goes on; {@code late} gets it).
     */
    static <T> Waiter<T> onWorker(String name, Callable<T> call) {
        Waiter<T> w = new Waiter<>(call);
        Thread t = new Thread(w, name);
        t.setDaemon(true);
        t.start();
        return w;
    }

    /** One call and its outcome. */
    static final class Waiter<T> implements Runnable {
        private final Callable<T> call;
        private final CountDownLatch done = new CountDownLatch(1);
        private final AtomicReference<T> result = new AtomicReference<>();
        private final AtomicReference<Throwable> error = new AtomicReference<>();

        Waiter(Callable<T> call) {
            this.call = call;
        }

        @Override
        public void run() {
            try {
                result.set(call.call());
            } catch (Throwable e) {
                error.set(e);
            } finally {
                done.countDown();
            }
        }

        /** True when the call returned or failed within {@code timeoutMs}. */
        boolean finished(long timeoutMs) throws InterruptedException {
            return done.await(timeoutMs, TimeUnit.MILLISECONDS);
        }

        T await(long timeoutMs, String where) throws Exception {
            if (!finished(timeoutMs)) {
                throw new TimeoutException(where + ": no return in " + timeoutMs + " ms");
            }
            Throwable e = error.get();
            if (e instanceof Exception) {
                throw (Exception) e;
            }
            if (e != null) {
                throw new IllegalStateException(e);
            }
            return result.get();
        }
    }
}
