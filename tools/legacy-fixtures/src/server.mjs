// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The handle() module of the legacy model for the conformance harness
// (server/conformance/CONTRACT.md section 5).

import { respond } from './model.mjs';

export const DEFAULT_PROFILE = 'legacy-canonical';

/**
 * @param {{ method: string, target?: string, headers: Array<[string, string]> }} request
 * @param {{ status: number, headers: Array<[string, string]> }} app
 * @param {Uint8Array} pageBytes
 * @param {{ profile?: string }} [config]
 * @returns {{ status: number, headers: Array<[string, string]>, body: Uint8Array | null }}
 */
export function handle(request, app, pageBytes, config = {}) {
  const profile = config !== null && typeof config === 'object' && typeof config.profile === 'string' ? config.profile : DEFAULT_PROFILE;
  return respond(profile, request, app, pageBytes);
}
