// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import { canonicalLine } from './canon.mjs';
import { LabError } from './errors.mjs';

export const MAX_EVENTS = 100_000;

/**
 * The event log of one run (CONTRACT section 5). The conductor gives every event its
 * sequence number `seq` (from 1, no gaps) and its time `t` (ms since the log was created).
 */
export class EventLog {
  #events = [];
  #waiters = new Set();
  #clock;
  #t0;
  #max;

  /** @param {{ clock?: () => number, maxEvents?: number }} [options] */
  constructor({ clock = Date.now, maxEvents = MAX_EVENTS } = {}) {
    this.#clock = clock;
    this.#t0 = clock();
    this.#max = maxEvents;
  }

  get events() {
    return this.#events;
  }

  /**
   * Append one event. The members `ev`, `seq`, `src` and `t` always come from the conductor.
   * @param {'harness'|'proxy'|'conductor'} src
   * @param {string} ev
   * @param {Record<string, unknown>} [members]
   */
  emit(src, ev, members = {}) {
    if (this.#events.length >= this.#max) {
      throw new LabError('EVENT_LIMIT', `event log is full (${this.#max} events)`);
    }
    const event = { ...members, ev, seq: this.#events.length + 1, src, t: Math.max(0, this.#clock() - this.#t0) };
    this.#events.push(event);
    for (const waiter of [...this.#waiters]) {
      if (waiter.ev === event.ev && event.seq > waiter.afterSeq) {
        this.#waiters.delete(waiter);
        clearTimeout(waiter.timer);
        waiter.resolve(event);
      }
    }
    return event;
  }

  /**
   * Resolve with the first event named `ev` whose `seq` is greater than `afterSeq`
   * (already logged or still to come). Rejects with WAIT_TIMEOUT.
   * @param {string} ev
   * @param {number} afterSeq
   * @param {number} timeoutMs
   */
  waitFor(ev, afterSeq, timeoutMs) {
    const seen = this.#events.find((e) => e.ev === ev && e.seq > afterSeq);
    if (seen) return Promise.resolve(seen);
    return new Promise((resolve, reject) => {
      const waiter = { ev, afterSeq, resolve, timer: null };
      waiter.timer = setTimeout(() => {
        this.#waiters.delete(waiter);
        reject(new LabError('WAIT_TIMEOUT', `no event "${ev}" within ${timeoutMs} ms`));
      }, timeoutMs);
      this.#waiters.add(waiter);
    });
  }

  /** The log as JSON Lines text: keys sorted, LF after every line. */
  toJsonl() {
    return this.#events.map((e) => `${canonicalLine(e)}\n`).join('');
  }
}
