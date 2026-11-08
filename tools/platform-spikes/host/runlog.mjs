// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Host part of the run log (CONTRACT.md sections 4 and 5): events with src=host, seq and t.

import { HostError, isRunId, stableStringify } from "./util.mjs";

const SAFE = /^[A-Za-z0-9.-]{1,64}$/;

export class RunLogs {
  /** @param {{ now?: () => number, maxRuns?: number, maxEvents?: number }} [opts] */
  constructor({ now = () => performance.now(), maxRuns = 16, maxEvents = 100_000 } = {}) {
    this.now = now;
    this.maxRuns = maxRuns;
    this.maxEvents = maxEvents;
    /** @type {Map<string, { t0: number, seq: number, lines: string[] }>} */
    this.runs = new Map();
  }

  /**
   * Start a run from the body of POST /run/start.
   * @param {unknown} meta
   * @returns {string} the run id
   */
  start(meta) {
    if (meta === null || typeof meta !== "object" || Array.isArray(meta)) {
      throw new HostError(400, "BAD_RUN_START", "body must be a JSON object");
    }
    const m = /** @type {Record<string, unknown>} */ (meta);
    if (!isRunId(m.run)) throw new HostError(400, "BAD_RUN_START", "run id must match [A-Za-z0-9.-]{1,128}");
    for (const key of ["capability", "platform", "os"]) {
      const v = m[key];
      if (typeof v !== "string" || !SAFE.test(v)) throw new HostError(400, "BAD_RUN_START", `${key} must match [A-Za-z0-9.-]{1,64}`);
    }
    const k = m.k;
    const kOk = Number.isInteger(k) ? /** @type {number} */ (k) >= 0 && /** @type {number} */ (k) < 1_000_000 : typeof k === "string" && SAFE.test(k);
    if (!kOk) throw new HostError(400, "BAD_RUN_START", "k must be a small integer or a safe token");
    const run = m.run;
    if (this.runs.has(run)) throw new HostError(409, "RUN_EXISTS", "run already started");
    if (this.runs.size >= this.maxRuns) throw new HostError(429, "TOO_MANY_RUNS", "too many open runs");
    this.runs.set(run, { t0: this.now(), seq: 0, lines: [] });
    return run;
  }

  /** @param {string | null} run */
  has(run) {
    return run !== null && this.runs.has(run);
  }

  /**
   * Add a host event. Events of an unknown run are dropped.
   * @param {string | null} run @param {string} ev @param {Record<string, unknown>} [members]
   */
  emit(run, ev, members = {}) {
    const r = run === null ? undefined : this.runs.get(run);
    if (r === undefined || r.lines.length >= this.maxEvents) return;
    const line = stableStringify({ ...members, ev, src: "host", seq: r.seq, t: Math.round(this.now() - r.t0) });
    r.seq += 1;
    r.lines.push(line);
  }

  /** @param {string} run @returns {string | null} JSONL of the run so far, or null when unknown */
  text(run) {
    const r = this.runs.get(run);
    return r === undefined ? null : r.lines.map((l) => `${l}\n`).join("");
  }

  /** Close a run. @param {string} run @returns {string | null} */
  end(run) {
    const t = this.text(run);
    this.runs.delete(run);
    return t;
  }
}

/** Waiters for /s/beacon (the gate of /s/stream). */
export class BeaconGate {
  constructor() {
    /** @type {Map<string, Set<() => void>>} */
    this.waiters = new Map();
  }

  static key(run, k) {
    return `${run ?? "-"}:${k}`;
  }

  /**
   * Register interest in the beacon of chunk `k` before the chunk is written.
   * @param {string | null} run @param {number} k
   * @returns {{ arrived: Promise<void>, cancel: () => void }}
   */
  expect(run, k) {
    const key = BeaconGate.key(run, k);
    /** @type {() => void} */
    let release = () => {};
    const arrived = new Promise((resolve) => {
      release = () => resolve();
    });
    const set = this.waiters.get(key) ?? new Set();
    set.add(release);
    this.waiters.set(key, set);
    const cancel = () => {
      set.delete(release);
      if (set.size === 0 && this.waiters.get(key) === set) this.waiters.delete(key);
    };
    return { arrived, cancel };
  }

  /** @param {string | null} run @param {number} k */
  arrive(run, k) {
    const key = BeaconGate.key(run, k);
    const set = this.waiters.get(key);
    this.waiters.delete(key);
    for (const release of set ?? []) release();
  }
}
