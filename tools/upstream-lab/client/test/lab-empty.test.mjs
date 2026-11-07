// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { postCase } from '../src/lab.mjs';

test('lab_posts_the_empty_page_as_an_empty_base64_text', async () => {
  let sent = null;
  await postCase({
    origin: 'lab-node',
    caseId: 'CS-30-quick-r1',
    scenarioCase: { page: 'empty' },
    fetchImpl: async (url, init) => {
      sent = JSON.parse(init.body);
      return new Response(null, { status: 204 });
    },
  });
  assert.equal(sent.page_base64, '');
  assert.deepEqual(sent.app, { status: 200, headers: [['content-type', 'text/html; charset=utf-8']] });
});
