// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The client model of the legacy fixtures (fixtures/legacy/README.md "Client fixtures"):
// run(profile, mode, case, env) gives the `client` member of a client expected.json (the
// projection of DL-110 and DL-118) for the profiles legacy-canonical (src/client-canonical.mjs)
// and legacy-android (src/client-android.mjs).
//
// The model is pure: no I/O (pages and server fixtures come from `env`), no real clock, no state
// between calls. It never throws for a case of any shape: a defect of the case is
// { ok: false, error: { code, message } }. An unknown profile or mode is a configuration error
// of the caller and throws ModelError.

import { ANDROID_PROFILE } from './client-android.mjs';
import { CANONICAL_PROFILE } from './client-canonical.mjs';
import { ClientCaseError, MODES, validateBehavior } from './client-case.mjs';
import { Sim } from './client-engine.mjs';
import { ModelError } from './model.mjs';

const PROFILES = Object.freeze({ 'legacy-canonical': CANONICAL_PROFILE, 'legacy-android': ANDROID_PROFILE });
export const CLIENT_MODEL_PROFILES = Object.freeze(Object.keys(PROFILES));

/**
 * Run a client case.
 *
 * @param {string} profile legacy-canonical or legacy-android
 * @param {string} mode quick or standard; must equal case.mode
 * @param {unknown} c the case (the members of a canonical client case.json)
 * @param {{ page?: (id: string) => Uint8Array | null, fixture?: (id: string) => { page: string, app: object } | null }} env
 * @returns {{ ok: true, client: object } | { ok: false, error: { code: string, message: string } }}
 */
export function run(profile, mode, c, env = {}) {
  const p = PROFILES[profile];
  if (p === undefined) throw new ModelError('LEGACY_UNKNOWN_PROFILE', `unknown legacy client profile: ${String(profile)}`);
  if (!MODES.includes(mode)) throw new ModelError('LEGACY_UNKNOWN_MODE', `unknown presentation mode: ${String(mode)}`);
  try {
    const behavior = validateBehavior(c);
    if (behavior.mode !== mode) return { ok: false, error: { code: 'CASE_MODE', message: `the case mode is ${behavior.mode}, not ${mode}` } };
    return { ok: true, client: new Sim(p, behavior, env).run() };
  } catch (error) {
    if (error instanceof ClientCaseError) return { ok: false, error: { code: error.code, message: error.message } };
    throw error;
  }
}
