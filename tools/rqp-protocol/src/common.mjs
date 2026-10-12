// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Constants, hashes and revisions shared by the reference server step and the
// reference client step (docs/engineering/protocol/manifest-patch.md section 1.3,
// docs/engineering/core/resource-limits.md).

import { createHash } from 'node:crypto';

/** Limits of rqp/1. The fixtures use the default values. */
export const LIMITS = Object.freeze({
  maxPatchBytes: 262144, // negotiation section 4 (server) and manifest-patch section 1.5 (client)
  maxRqHaveBytes: 4096, // negotiation section 3.4
  maxRqCapabilitiesBytes: 1024, // negotiation section 3.3
  maxManifestBytes: 65536, // manifest-patch section 1.8 (H-50)
  maxDocumentBytes: 5 * 1024 * 1024,
  maxBlockBytes: 1024 * 1024,
  maxBlocks: 256,
  maxJsonDepth: 16, // parser limit of manifest-patch section 3.2, rule 5
});

export const DELTA_TYPE = 'application/vnd.rivqen.delta+json';
export const MANIFEST_TYPE = 'application/rivqen-manifest+json';
export const MANIFEST_START = `<script type="${MANIFEST_TYPE}">`;
export const MANIFEST_END = '</script>';

/** Client error codes of manifest-patch section 5 (H-50). */
export const Code = Object.freeze({
  MANIFEST_INVALID: 'MANIFEST_INVALID',
  PATCH_TOO_LARGE: 'PATCH_TOO_LARGE',
  PATCH_INVALID: 'PATCH_INVALID',
  TEMPLATE_MISMATCH: 'TEMPLATE_MISMATCH',
  PATCH_BASE_MISMATCH: 'PATCH_BASE_MISMATCH',
  INTEGRITY_ERROR: 'INTEGRITY_ERROR',
  PATCH_CONTENT_REJECTED: 'PATCH_CONTENT_REJECTED',
});

/**
 * An input that the reference functions do not model. The generator stops on
 * it, so a fixture can never get an expected result from an unmodelled path.
 */
export class UnsupportedInput extends Error {
  constructor(message) {
    super(message);
    this.name = 'UnsupportedInput';
    this.code = 'RQP_REFERENCE_UNSUPPORTED';
  }
}

/** base64url without padding of SHA-256 over bytes or a UTF-8 string. */
export function sha256b64u(data) {
  return createHash('sha256').update(data).digest('base64url');
}

function decimal(n) {
  if (!Number.isSafeInteger(n) || n < 0) throw new TypeError('not a non-negative integer');
  return String(n);
}

/**
 * Text S of the policy line (manifest-patch section 1.3, step 4; H-55): the
 * members of `policy` and `limits` that the manifest has, in the fixed order of
 * the table. '' when the manifest has neither member (no policy line).
 * `members` holds the optional `policy` and `limits` of a manifest.
 */
export function policyLimitsText(members) {
  const items = [];
  if (Object.hasOwn(members, 'policy')) {
    const p = members.policy;
    items.push('policy');
    if (Object.hasOwn(p, 'mode')) {
      if (p.mode !== 'private' && p.mode !== 'public') throw new TypeError('mode is not private or public');
      items.push(`mode=${p.mode}`);
    }
    if (Object.hasOwn(p, 'ttl_seconds')) items.push(`ttl_seconds=${decimal(p.ttl_seconds)}`);
    if (Object.hasOwn(p, 'offline')) {
      if (typeof p.offline !== 'boolean') throw new TypeError('offline is not a boolean');
      items.push(`offline=${p.offline}`);
    }
  }
  if (Object.hasOwn(members, 'limits')) {
    items.push('limits');
    if (Object.hasOwn(members.limits, 'max_patch_bytes')) {
      items.push(`max_patch_bytes=${decimal(members.limits.max_patch_bytes)}`);
    }
  }
  return items.join(';');
}

/**
 * Page revision of manifest-patch section 1.3, steps 4 and 5.
 * blocks = [{ id, format, sha256 }]; members = the optional policy and limits (H-55).
 */
export function pageRevision(templateRevision, blocks, members = {}) {
  let p = `rqp-r1\n${templateRevision}\n`;
  const s = policyLimitsText(members);
  if (s !== '') p += `PL\t${s}\n`;
  for (const b of blocks) p += `${b.id}\t${b.format}\t${b.sha256}\n`;
  return `r1.${sha256b64u(p)}`;
}

/** Only the members `policy` and `limits` of an object (a manifest or a server configuration). */
export function membersOf(obj) {
  const out = {};
  if (obj !== null && obj !== undefined) {
    if (Object.hasOwn(obj, 'policy')) out.policy = obj.policy;
    if (Object.hasOwn(obj, 'limits')) out.limits = obj.limits;
  }
  return out;
}

/** The first 8 bytes of the raw SHA-256 digest of a block (H-44), from its base64url hash. */
export function digestPrefix(sha256) {
  return Buffer.from(sha256, 'base64url').subarray(0, 8);
}

/** ASCII lower case of the bytes A-Z only (no Unicode case folding, H-28). */
export function asciiLower(s) {
  return s.replace(/[A-Z]+/g, (m) => m.toLowerCase());
}
