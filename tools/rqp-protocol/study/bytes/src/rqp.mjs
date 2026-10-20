// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The rqp/1 baseline of the study: the full document F, the 304 and the delta
// D of a page change A -> B, made by the reference server step
// (tools/rqp-protocol/src/server.mjs, negotiation section 4). The study uses
// these bytes as they are; it never builds its own baseline.

import { havePrefixes } from '../baseline/client.mjs';
import { DELTA_TYPE, MANIFEST_END, MANIFEST_START, membersOf } from '../baseline/common.mjs';
import { analyze, documentHeadStart } from '../../../src/markup.mjs';
import { changeSet, deltaEnvelope, handle, manifestOf } from '../baseline/server.mjs';
import { StudyError } from './errors.mjs';

export { DELTA_TYPE, MANIFEST_END, MANIFEST_START };

/** The application response of every study page. */
const APP = Object.freeze({ status: 200, headers: [['content-type', 'text/html; charset=utf-8']] });

/**
 * RQP request fields (negotiation section 6.2). `snapshot` = { template_revision,
 * page_revision, blocks } or null for a first visit.
 */
export function requestFields(snapshot) {
  const base = [
    ['rq-version', '1'],
    ['rq-capabilities', '(block-patch)'],
  ];
  if (snapshot === null) return [...base, ['accept', 'text/html']];
  return [
    ...base,
    ['accept', `text/html, ${DELTA_TYPE}`],
    ['if-none-match', `"${snapshot.page_revision}"`],
    ['rq-have', `t=${snapshot.template_revision}, b=:${havePrefixes(snapshot.blocks)}:`],
  ];
}

function run(target, fields, page, config) {
  return handle({ method: 'GET', target, headers: fields }, APP, page, config);
}

/**
 * The exchanges of a page change A -> B: cold visit (F of A), revisit without
 * change (304), revisit after the change (D, or F of B when the server picks F).
 * config = the manifest members policy and limits of the server.
 */
export function exchange(pageA, pageB, target, config = {}) {
  const members = membersOf(config);
  const analysisA = analyze(pageA);
  const analysisB = analyze(pageB);
  if (!analysisA.valid || !analysisB.valid) throw new StudyError('STUDY_PAGE', 'a study page is invalid for RQP');
  const manifestA = manifestOf(analysisA, target, members);
  const manifestB = manifestOf(analysisB, target, members);
  const cold = run(target, requestFields(null), pageA, config);
  if (cold.kind !== 'full') throw new StudyError('STUDY_BASELINE', `cold visit gave ${cold.reason}`);
  const snapshot = { template_revision: manifestA.template_revision, page_revision: manifestA.page_revision, blocks: manifestA.blocks };
  const unchanged = run(target, requestFields(snapshot), pageA, config);
  if (unchanged.status !== 304) throw new StudyError('STUDY_BASELINE', `unchanged revisit gave ${unchanged.reason}`);
  const changed = run(target, requestFields(snapshot), pageB, config);
  const fullB = run(target, requestFields(null), pageB, config);
  return {
    target,
    members,
    pageA,
    pageB,
    analysisA,
    analysisB,
    manifestA,
    manifestB,
    insertAtA: documentHeadStart(pageA),
    insertAtB: documentHeadStart(pageB),
    requestFirst: requestFields(null),
    requestRevisit: requestFields(snapshot),
    cold,
    unchanged,
    changed,
    fullB,
    delta: changed.kind === 'delta' ? JSON.parse(changed.body.toString('utf8')) : null,
    // The envelope that step 7 builds before its size rule (also when the server then sends F).
    candidate: candidateEnvelope(pageB, analysisB, manifestB, manifestA),
  };
}

/** F of `page` with the manifest text `text` and the given wrapper at the insertion point. */
export function withManifest(page, insertAt, text, start = MANIFEST_START, end = MANIFEST_END) {
  return Buffer.concat([page.subarray(0, insertAt), Buffer.from(start + text + end, 'utf8'), page.subarray(insertAt)]);
}

/** Step 7 envelope for the change A -> B, with the change set of the Rq-Have prefixes of A (null when no block changed). */
function candidateEnvelope(pageB, analysisB, manifestB, manifestA) {
  const prefixes = Buffer.from(havePrefixes(manifestA.blocks), 'base64');
  const indices = changeSet(analysisB.blocks, prefixes);
  return indices.length === 0 ? null : deltaEnvelope(pageB, analysisB, manifestB, manifestA.page_revision, indices);
}
