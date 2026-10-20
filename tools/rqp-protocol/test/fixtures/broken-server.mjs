// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// A server under test with one defect: it never sends Rq-Version. check.mjs must fail it.

import { handle as reference } from '../../src/server.mjs';

export function handle(request, app, page, config) {
  const r = reference(request, app, page, config);
  return { ...r, headers: r.headers.filter(([n]) => n !== 'rq-version') };
}
