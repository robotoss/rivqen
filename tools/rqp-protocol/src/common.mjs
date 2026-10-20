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
  maxRqNonceBytes: 128, // negotiation section 3.9 (DL-063): a longer Rq-Nonce field line is no nonce
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

/**
 * Client error codes of manifest-patch section 5 (H-50). Since H-104 an rqp/1
 * delta never gives TEMPLATE_MISMATCH or PATCH_BASE_MISMATCH: a delta for
 * another template or another base fails the binding check of step 7
 * (INTEGRITY_ERROR, H-107), so the reference has no step that produces them.
 */
export const Code = Object.freeze({
  MANIFEST_INVALID: 'MANIFEST_INVALID',
  PATCH_TOO_LARGE: 'PATCH_TOO_LARGE',
  PATCH_INVALID: 'PATCH_INVALID',
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

/**
 * A call that breaks the contract of a reference function (a wrong argument
 * type or a value that the caller must check first). It is a defect of the
 * caller, never a result for protocol input.
 */
export class ContractError extends TypeError {
  constructor(message) {
    super(message);
    this.name = 'ContractError';
    this.code = 'RQP_REFERENCE_CONTRACT';
  }
}

/** base64url without padding of SHA-256 over bytes or a UTF-8 string. */
export function sha256b64u(data) {
  return createHash('sha256').update(data).digest('base64url');
}

function decimal(n) {
  if (!Number.isSafeInteger(n) || n < 0) throw new ContractError('not a non-negative integer');
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
      if (p.mode !== 'private' && p.mode !== 'public') throw new ContractError('mode is not private or public');
      items.push(`mode=${p.mode}`);
    }
    if (Object.hasOwn(p, 'ttl_seconds')) items.push(`ttl_seconds=${decimal(p.ttl_seconds)}`);
    if (Object.hasOwn(p, 'offline')) {
      if (typeof p.offline !== 'boolean') throw new ContractError('offline is not a boolean');
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

const REVISION = /^r1\.[A-Za-z0-9_-]{43}$/;

/**
 * The URL U of the delta binding (manifest-patch section 2.2, H-107; H-58):
 * the WHATWG URL serialization of `url` without its fragment. The server
 * passes the URL of the request that it answers, the client the final URL of
 * the response (after redirects). The steps of cache identity section 2 that
 * the WHATWG serializer does not do (percent-encoding normalization, ignored
 * query parameters) are not applied: the server cannot know the client
 * configuration of ignored parameters. The serialization never has a line
 * feed: the parser removes tab and newline and percent-encodes the other C0
 * controls. Throws a TypeError for a string that is not a URL.
 */
export function canonicalUrl(url) {
  const u = new URL(url);
  u.hash = '';
  return u.href;
}

/** The length of the client nonce of a delta request in bytes (H-108). */
export const NONCE_BYTES = 16;

/** The text N of a nonce: canonical base64url without padding of 16 bytes (22 characters, H-108). */
const NONCE_TEXT = /^[A-Za-z0-9_-]{21}[AQgw]$/;

/**
 * True when `text` is the text N of a nonce (H-108): base64url without
 * padding of exactly 16 bytes, in the canonical form (the last character has
 * no set pad bits), so one nonce has one text. Never throws.
 */
export function isNonceText(text) {
  return typeof text === 'string' && NONCE_TEXT.test(text);
}

/**
 * The text N of nonce bytes (H-108): base64url without padding. The server
 * decodes the Byte Sequence of the field Rq-Nonce (H-109) and writes it in this
 * form, so a padded and an unpadded Byte Sequence give the same N. Throws a
 * TypeError for a value that is not a Buffer of 16 bytes.
 */
export function nonceText(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length !== NONCE_BYTES) throw new ContractError('the nonce is not 16 bytes');
  return bytes.toString('base64url');
}

/**
 * The preimage of the delta binding (manifest-patch section 2.2, H-107,
 * H-108): "rqp-d1" LF U LF B LF N LF R' LF. `url` is U (canonicalUrl), `base`
 * the base page revision B, `nonce` the text N of the client nonce of the
 * request (isNonceText), `next` the next page revision R'. Throws a TypeError
 * when U has a line feed or B, N or R' does not have its form: one preimage
 * has one parse.
 */
export function deltaBindingPreimage(url, base, nonce, next) {
  if (typeof url !== 'string' || url.includes('\n')) throw new ContractError('the binding URL is not a canonical URL');
  if (typeof base !== 'string' || !REVISION.test(base)) throw new ContractError('the binding base is not a page revision');
  if (!isNonceText(nonce)) throw new ContractError('the binding nonce is not the text of 16 bytes');
  if (typeof next !== 'string' || !REVISION.test(next)) throw new ContractError('the binding next revision is not a page revision');
  return `rqp-d1\n${url}\n${base}\n${nonce}\n${next}\n`;
}

/** The delta binding value (H-107, H-108): "d1." + B64U(SHA-256(UTF-8 of the preimage)). */
export function deltaBinding(url, base, nonce, next) {
  return `d1.${sha256b64u(deltaBindingPreimage(url, base, nonce, next))}`;
}

/**
 * The default values of the members of `policy` (manifest-patch section 1.4).
 * `limits.max_patch_bytes` has no default value: an absent member means "no
 * hint" (section 1.5).
 */
export const POLICY_DEFAULTS = Object.freeze({ mode: 'private', ttl_seconds: 0, offline: false });

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
