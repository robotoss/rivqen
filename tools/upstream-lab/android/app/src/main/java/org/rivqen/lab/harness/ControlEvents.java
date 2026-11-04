// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import org.json.JSONObject;

import java.io.IOException;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.TimeUnit;

/**
 * Sends events to the conductor in the order of {@link #emit}: one bounded FIFO queue and one sender
 * thread, so the conductor receives the events of all harness threads in one order.
 */
final class ControlEvents extends Events {

    private static final int RETRIES = 3;

    private final ControlChannel channel;
    private final BlockingQueue<JSONObject> queue = new ArrayBlockingQueue<>(Lab.MAX_EVENT_QUEUE);
    private final Object idle = new Object();
    private final Thread sender;
    private int pending;
    private long nextHseq;
    private volatile String failure;
    private volatile boolean closed;

    ControlEvents(ControlChannel channel) {
        this.channel = channel;
        this.sender = new Thread(this::loop, "rq-events");
        this.sender.setDaemon(true);
        this.sender.start();
    }

    @Override
    protected void send(JSONObject event) {
        if (closed) {
            return;
        }
        synchronized (idle) {
            // hseq: the harness order of the event. It is set under the same lock as the queue
            // insert, so it equals the send order. The conductor order (seq) of harness events
            // relative to proxy events depends on delivery time; hseq does not.
            try {
                event.put("hseq", nextHseq + 1);
            } catch (org.json.JSONException e) {
                fail("hseq not encoded: " + e.getMessage());
                return;
            }
            if (!queue.offer(event)) {
                fail("event queue over " + Lab.MAX_EVENT_QUEUE);
                return;
            }
            nextHseq++;
            pending++;
        }
    }

    /** First send failure, or {@code null}. A failed run is stopped by the step loop. */
    String failure() {
        return failure;
    }

    /** Waits until every queued event is delivered, at most {@code timeoutMs}. */
    boolean flush(long timeoutMs) throws InterruptedException {
        long end = System.currentTimeMillis() + timeoutMs;
        synchronized (idle) {
            while (pending > 0 && failure == null) {
                long left = end - System.currentTimeMillis();
                if (left <= 0) {
                    return false;
                }
                idle.wait(left);
            }
        }
        return failure == null;
    }

    /** Stops the sender after the queued events. */
    void close(long timeoutMs) throws InterruptedException {
        flush(timeoutMs);
        closed = true;
        sender.interrupt();
    }

    private void loop() {
        while (!closed) {
            JSONObject event;
            try {
                event = queue.poll(1, TimeUnit.SECONDS);
            } catch (InterruptedException e) {
                return;
            }
            if (event == null) {
                continue;
            }
            deliver(event);
            synchronized (idle) {
                pending--;
                idle.notifyAll();
            }
        }
    }

    private void deliver(JSONObject event) {
        if (failure != null) {
            return;
        }
        IOException last = null;
        for (int attempt = 0; attempt < RETRIES; attempt++) {
            try {
                channel.event(event);
                return;
            } catch (IOException e) {
                last = e;
                try {
                    Thread.sleep(200L * (attempt + 1));
                } catch (InterruptedException ie) {
                    Thread.currentThread().interrupt();
                    break;
                }
            }
        }
        fail("event not delivered: " + (last == null ? "interrupted" : Lab.describe(last)));
    }

    private void fail(String reason) {
        if (failure == null) {
            failure = reason;
            Lab.log(reason);
        }
        synchronized (idle) {
            idle.notifyAll();
        }
    }
}
