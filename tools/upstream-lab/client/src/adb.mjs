// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// adb commands of the runner. They are built here as argument lists and run by the caller.
// `adb shell` joins its arguments into one shell line, so every value that goes into it is checked.
import { LabError } from './errors.mjs';

export const DEFAULT_PACKAGE = 'org.rivqen.lab.harness';
// The harness is an instrumentation (architect, WP-01 S1): its result line is `rivqen.ok=true|false`.
export const DEFAULT_START = 'shell am instrument -w -e run {run} -e control {control} {package}/{component}';
export const DEFAULT_COMPONENT = 'org.rivqen.lab.harness.LabInstrumentation';
export const DEFAULT_SERIAL = 'emulator-5600';
export const CONTROL_URL = 'http://10.0.2.2:18091';

const SAFE = /^[A-Za-z0-9._/-]+$/;

function checked(name, value) {
  if (typeof value !== 'string' || !SAFE.test(value)) throw new LabError('BAD_ARG', `unsafe value for ${name}`);
  return value;
}

/** @param {{ serial?: string }} cfg  the device; default the lab emulator */
const prefix = (cfg) => ['-s', checked('serial', cfg.serial ?? DEFAULT_SERIAL)];

/** `pm clear` of the harness package (state reset before each run). */
export function clearArgs(cfg) {
  return [...prefix(cfg), 'shell', 'pm', 'clear', checked('package', cfg.package ?? DEFAULT_PACKAGE)];
}

/** `am force-stop` of the harness package (after each run). */
export function stopArgs(cfg) {
  return [...prefix(cfg), 'shell', 'am', 'force-stop', checked('package', cfg.package ?? DEFAULT_PACKAGE)];
}

/**
 * The command that starts the harness for one run. `cfg.startCommand` is a template of
 * whitespace-separated words with {package}, {component}, {run} and {control}.
 */
export function startArgs(cfg, runId) {
  const values = {
    package: checked('package', cfg.package ?? DEFAULT_PACKAGE),
    component: checked('component', cfg.component ?? DEFAULT_COMPONENT),
    run: checked('run', runId),
    control: CONTROL_URL,
  };
  const words = (cfg.startCommand ?? DEFAULT_START).split(/\s+/).filter(Boolean);
  return [...prefix(cfg), ...words.map((w) => w.replace(/\{(package|component|run|control)\}/g, (_, k) => values[k]))];
}

/**
 * The verdict in the output of `am instrument`: true or false from the line `rivqen.ok=true|false`
 * (the last such line), or null when there is none.
 */
export function parseInstrumentResult(output) {
  const found = [...output.matchAll(/rivqen\.ok=(true|false)\b/g)];
  return found.length === 0 ? null : found.at(-1)[1] === 'true';
}
