// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// A server under test that never applies RQP: it sends the application response unchanged.
// check.mjs must fail it on MUST fixtures and on SHOULD fixtures, with different labels.

export function handle(_request, app, page) {
  return { status: app.status, headers: app.headers, body: page };
}
