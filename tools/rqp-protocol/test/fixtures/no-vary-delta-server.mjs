// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// A server under test that is the reference server, except that its delta has
// no Vary: Rq-Nonce. That is a SHOULD of DL-063 (L1): check.mjs must count the
// fixture as SHOULD failed, not as failed.

import { handle as reference } from '../../src/server.mjs';

export function handle(request, app, page, config) {
  const r = reference(request, app, page, config);
  if (r.kind !== 'delta') return r;
  return { ...r, headers: r.headers.filter(([n, v]) => !(n === 'vary' && v === 'Rq-Nonce')) };
}
