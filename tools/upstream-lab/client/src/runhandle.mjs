// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import { LabError } from './errors.mjs';

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

/**
 * What the control channel needs from a run: the scenario body, the long poll, and the intake of
 * harness events. The harness may not send conductor or proxy events, and every harness event
 * needs a `thread` (CONTRACT section 5).
 * @param {{ id: string, scenario: () => object, log: import('./eventlog.mjs').EventLog, channel: import('./channel.mjs').HarnessChannel }} input
 */
export function makeRunHandle({ id, scenario, log, channel }) {
  return {
    id,
    scenario,
    poll: (ms) => channel.poll(ms),
    accept: (event) => {
      if (!isObject(event) || typeof event.ev !== 'string' || event.ev === '') throw new LabError('BAD_EVENT', 'event needs a name "ev"');
      const aborts = event.ev === 'run.end' && event.ok === false; // the only conductor event that the harness may send
      if (!aborts && (/^(proxy|run)\./.test(event.ev) || event.ev === 'step.start')) throw new LabError('BAD_EVENT', `the harness may not send ${event.ev}`);
      if (event.ev === 'run.end' && !aborts) throw new LabError('BAD_EVENT', 'the harness sends run.end only with ok false');
      if (typeof event.thread !== 'string' || event.thread === '') throw new LabError('BAD_EVENT', 'event needs a "thread"');
      if (!Number.isInteger(event.hseq) || event.hseq < 1) throw new LabError('BAD_EVENT', 'event needs a positive integer "hseq"');
      channel.accept(log.emit('harness', event.ev, event));
      if (aborts) channel.fail(new LabError('HARNESS_ABORT', `the harness aborted the run: ${String(event.error)}`));
    },
  };
}
