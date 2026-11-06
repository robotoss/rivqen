// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Writes the meta file of an Android function run (CONTRACT sections 7 and 8): the platform build,
// the upstream SHA, the case count and the package-access members (non_public_calls). Called by
// functions.sh with the values in the environment. No device identifier goes into the file.
'use strict';
const fs = require('fs');

const e = process.env;
for (const k of ['API', 'RELEASE', 'ABI', 'IMAGE', 'CASES', 'ACCESS', 'OUT']) {
  if (!e[k]) {
    process.stderr.write(`functions-meta: ${k} is not set\n`);
    process.exit(2);
  }
}
const access = JSON.parse(fs.readFileSync(e.ACCESS, 'utf8'));
const cases = JSON.parse(fs.readFileSync(e.CASES, 'utf8'));
const meta = {
  platform: 'android',
  runner: 'harness instrumentation, mode functions',
  api_level: e.API,
  android_release: e.RELEASE,
  abi: e.ABI,
  emulator_image: e.IMAGE,
  webview: e.WEBVIEW || 'unknown',
  upstream_sha: access.upstream,
  cases: cases.length,
  non_public_calls: access.members.map((m) => `${m.member} (${m.kind}; ${m.use})`),
  limit: `Results are evidence for this Android build only (API ${e.API} emulator image). org.json, android.net.Uri and the default charset of other builds can differ.`,
};
fs.writeFileSync(e.OUT, `${JSON.stringify(meta)}\n`);
