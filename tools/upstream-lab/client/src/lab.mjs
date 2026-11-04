// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The control endpoint of the lab servers (server/conformance/CONTRACT.md section 2).
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { LabError } from './errors.mjs';
import { ORIGINS, PROFILES } from './steps.mjs';

export const PAGES_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..', 'pages');
export const DEFAULT_APP_HEADERS = [['content-type', 'text/html; charset=utf-8']];
const PAGE = /^[a-z0-9_-]+$/;

/** The JSON text of a case request. `scenarioCase` is { page, status?, headers? } (CONTRACT section 2). */
export function buildCaseBody({ caseId, origin, scenarioCase, pageBytes }) {
  if (!Object.hasOwn(PROFILES, origin)) throw new LabError('BAD_ORIGIN', `unknown origin ${origin}`);
  return JSON.stringify({
    suite: 'legacy',
    case_id: caseId,
    profile: PROFILES[origin],
    page_base64: pageBytes.toString('base64'),
    app: { status: scenarioCase.status ?? 200, headers: scenarioCase.headers ?? DEFAULT_APP_HEADERS },
    server_config: {},
  });
}

/** Post the application response of a scenario to the lab server of `origin`. */
export async function postCase({ origin, caseId, scenarioCase, pagesDir = PAGES_DIR, fetchImpl = fetch, ports = ORIGINS }) {
  if (!PAGE.test(scenarioCase.page)) throw new LabError('BAD_PAGE', `bad page name ${scenarioCase.page}`);
  const port = ports[origin];
  if (port === undefined) throw new LabError('BAD_ORIGIN', `unknown origin ${origin}`);
  let pageBytes;
  try {
    pageBytes = await readFile(path.join(pagesDir, `${scenarioCase.page}.html`));
  } catch (cause) {
    throw new LabError('NO_PAGE', `cannot read page ${scenarioCase.page}`, { cause });
  }
  const body = buildCaseBody({ caseId, origin, scenarioCase, pageBytes });
  let res;
  try {
    res = await fetchImpl(`http://127.0.0.1:${port}/__rivqen/conformance/case`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body,
      signal: AbortSignal.timeout(10_000),
    });
  } catch (cause) {
    throw new LabError('LAB_DOWN', `lab server ${origin} on 127.0.0.1:${port} is not reachable`, { cause });
  }
  await res.arrayBuffer();
  if (res.status !== 204) throw new LabError('CASE_REJECTED', `lab server ${origin} answered ${res.status} to the case`);
}
