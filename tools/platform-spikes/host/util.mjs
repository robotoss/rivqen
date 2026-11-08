// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Pure helpers of the spike host: errors, limits, site detection, query parsing, log-safe fields.

export const MAX_SIZE = 8 * 1024 * 1024; // bytes of one chunk or one body
export const MAX_CHUNKS = 1024;
export const MAX_TOTAL = 64 * 1024 * 1024; // chunks * size
export const MAX_MS = 60_000;

/** An error that maps to an HTTP answer. `code` is a fixed token, `message` has no request data. */
export class HostError extends Error {
  /** @param {number} status @param {string} code @param {string} message @param {ErrorOptions} [options] */
  constructor(status, code, message, options) {
    super(message, options);
    this.name = "HostError";
    this.status = status;
    this.code = code;
  }
}

const HOST_HEADER = /^([A-Za-z0-9.-]{1,253})(?::(\d{1,5}))?$/;
const SITE_B_HOSTS = new Set(["127.0.0.1", "10.0.2.2"]);

/**
 * Site of a request from its Host field. A is `localhost`, B is the IP form.
 * @param {string | undefined} hostHeader
 * @returns {{ site: "A" | "B", host: string, port: string | null } | null}
 */
export function siteOf(hostHeader) {
  const m = HOST_HEADER.exec(hostHeader ?? "");
  if (m === null) return null;
  const host = m[1].toLowerCase();
  const port = m[2] ?? null;
  if (host === "localhost") return { site: "A", host, port };
  if (SITE_B_HOSTS.has(host)) return { site: "B", host, port };
  return null;
}

const RUN_ID = /^[A-Za-z0-9.-]{1,128}$/;
export const isRunId = (v) => typeof v === "string" && RUN_ID.test(v);

/**
 * The run id of a request: the `Rq-Lab-Run` field wins over the `run` query parameter.
 * @param {string | undefined} header @param {string | null} param
 * @returns {string | null}
 */
export function runOf(header, param) {
  const v = header ?? param;
  if (v === undefined || v === null) return null;
  if (!isRunId(v)) throw new HostError(400, "BAD_RUN", "run id must match [A-Za-z0-9.-]{1,128}");
  return v;
}

const ASCII = /^[\x20-\x7e]*$/;

/**
 * Parse a raw query string. Every name and value must be printable ASCII after decoding.
 * @param {string} raw
 * @returns {URLSearchParams}
 */
export function parseQuery(raw) {
  const q = new URLSearchParams(raw);
  for (const [k, v] of q) {
    if (!ASCII.test(k) || !ASCII.test(v)) throw new HostError(400, "BAD_QUERY", "query must be ASCII");
  }
  return q;
}

/**
 * @param {URLSearchParams} q @param {string} name
 * @param {{ def?: number, min: number, max: number }} spec
 * @returns {number}
 */
export function intParam(q, name, { def, min, max }) {
  const v = q.get(name);
  if (v === null) {
    if (def === undefined) throw new HostError(400, "BAD_PARAM", `parameter ${name} is required`);
    return def;
  }
  if (!/^\d{1,9}$/.test(v)) throw new HostError(400, "BAD_PARAM", `parameter ${name} must be an integer`);
  const n = Number(v);
  if (n < min || n > max) throw new HostError(400, "BAD_PARAM", `parameter ${name} must be ${min}..${max}`);
  return n;
}

/**
 * @template {string} T
 * @param {URLSearchParams} q @param {string} name @param {readonly T[]} allowed @param {T} def
 * @returns {T}
 */
export function enumParam(q, name, allowed, def) {
  const v = q.get(name);
  if (v === null) return def;
  const hit = allowed.find((a) => a === v);
  if (hit === undefined) throw new HostError(400, "BAD_PARAM", `parameter ${name} must be one of ${allowed.join("|")}`);
  return hit;
}

const COOKIE_VALUE = /^[a-z0-9-]*$/;

function redactCookie(name, value) {
  // Set-Cookie: only the first segment is a name=value pair. Cookie: every segment is.
  const segments = name === "set-cookie" ? [value.split(";")[0]] : value.split(";");
  const ok = segments.every((s) => {
    const eq = s.indexOf("=");
    return eq >= 0 && COOKIE_VALUE.test(s.slice(eq + 1).trim());
  });
  return ok ? value : "[redacted]";
}

/**
 * A field value that is safe to put in the log: a cookie value outside [a-z0-9-] is replaced.
 * @param {string} name lower case @param {string} value
 */
export function loggableValue(name, value) {
  return name === "cookie" || name === "set-cookie" ? redactCookie(name, value) : value;
}

/** JSON with sorted keys at every level (the run log format). */
export function stableStringify(v) {
  if (Array.isArray(v)) return `[${v.map((x) => stableStringify(x === undefined ? null : x)).join(",")}]`;
  if (v !== null && typeof v === "object") {
    const parts = [];
    for (const k of Object.keys(v).sort()) {
      if (v[k] !== undefined) parts.push(`${JSON.stringify(k)}:${stableStringify(v[k])}`);
    }
    return `{${parts.join(",")}}`;
  }
  return JSON.stringify(v);
}

/**
 * Sleep that ends early on abort.
 * @param {number} ms @param {AbortSignal} signal
 * @returns {Promise<boolean>} true when the time passed, false when aborted
 */
export function sleep(ms, signal) {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve(false);
    const onAbort = () => {
      clearTimeout(t);
      resolve(false);
    };
    const t = setTimeout(() => {
      signal.removeEventListener("abort", onAbort);
      resolve(true);
    }, ms);
    signal.addEventListener("abort", onAbort, { once: true });
  });
}
