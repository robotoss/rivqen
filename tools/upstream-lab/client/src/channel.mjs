// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import { LabError } from './errors.mjs';

/**
 * The harness side of one run: one step at a time. A step is dispatched, the harness takes it with
 * `poll`, and the harness acknowledges it with a `step.done` event whose `step.seq` is the same.
 * After the last step, `finish` makes the next poll answer `{ do: 'end' }`.
 */
export class HarnessChannel {
  #queued = null;
  #delivered = false;
  #current = null; // { seq, resolve, reject, timer }
  #ending = false;
  #endDelivered = false;
  #endWaiters = [];
  #wakers = new Set();
  #failure = null;

  /**
   * Send one step to the harness. Resolves with the `step.done` event; rejects with STEP_FAILED
   * (the harness reported an error), STEP_TIMEOUT or HARNESS_FAILED.
   */
  dispatch(step, timeoutMs) {
    if (this.#failure) return Promise.reject(this.#failure);
    if (this.#current) return Promise.reject(new LabError('BUSY', 'a step is still running'));
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#clear();
        reject(new LabError('STEP_TIMEOUT', `harness did not finish step ${step.seq} (${step.do}) within ${timeoutMs} ms`));
      }, timeoutMs);
      this.#current = { seq: step.seq, resolve, reject, timer };
      this.#queued = step;
      this.#delivered = false;
      this.#wake();
    });
  }

  /** Long poll of the harness (GET /run/next). Resolves with a step, `{ do: 'end' }` or null on timeout. */
  async poll(timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      if (this.#queued && !this.#delivered) {
        this.#delivered = true;
        return this.#queued;
      }
      if (this.#ending && !this.#queued && !this.#endDelivered) {
        this.#endDelivered = true;
        for (const done of this.#endWaiters) done();
        this.#endWaiters = [];
        return { do: 'end' };
      }
      const left = deadline - Date.now();
      if (left <= 0) return null;
      await this.#sleepUntilWake(left);
    }
  }

  /** Handle a harness event. Returns true when it acknowledged the current step. */
  accept(event) {
    const cur = this.#current;
    if (!cur || event.ev !== 'step.done' || event.step?.seq !== cur.seq) return false;
    this.#clear();
    if (typeof event.error === 'string') cur.reject(new LabError('STEP_FAILED', `harness step ${cur.seq} failed: ${event.error}`));
    else cur.resolve(event);
    return true;
  }

  /** The harness process failed (for example it could not start). Pending and later steps fail. */
  fail(cause) {
    this.#failure = new LabError('HARNESS_FAILED', `harness failed: ${cause?.message ?? 'unknown'}`, { cause });
    const cur = this.#current;
    this.#clear();
    cur?.reject(this.#failure);
    this.#wake();
  }

  /** Make the harness stop. Resolves when it has fetched `end`, or after `timeoutMs` (false). */
  finish(timeoutMs) {
    this.#ending = true;
    this.#wake();
    if (this.#endDelivered) return Promise.resolve(true);
    return new Promise((resolve) => {
      const timer = setTimeout(() => resolve(false), timeoutMs);
      this.#endWaiters.push(() => {
        clearTimeout(timer);
        resolve(true);
      });
    });
  }

  #clear() {
    if (this.#current) clearTimeout(this.#current.timer);
    this.#current = null;
    this.#queued = null;
  }

  #wake() {
    for (const wake of [...this.#wakers]) wake();
  }

  #sleepUntilWake(ms) {
    return new Promise((resolve) => {
      const wake = () => {
        clearTimeout(timer);
        this.#wakers.delete(wake);
        resolve();
      };
      const timer = setTimeout(wake, ms);
      this.#wakers.add(wake);
    });
  }
}
