// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Fault scenarios of CONTRACT.md section 3: redirect, cookie/set, echo, range, cut.

import { HostError, MAX_SIZE, enumParam, intParam } from "./util.mjs";

const HTML = "text/html; charset=utf-8";
const SYNTHETIC = /^[a-z0-9-]{1,64}$/;
const SAFE_TARGET = /^\/(?!\/)[\x21-\x7e]*$/;
const CODES = ["301", "302", "303", "307", "308"];
const MAX_HOPS = 20;

/** Bytes of the lab body: byte i is (i * 31 + 7) mod 251 (CONTRACT.md section 10). */
export function labBytes(from, to) {
  const out = Buffer.alloc(to - from);
  for (let i = from; i < to; i += 1) out[i - from] = (i * 31 + 7) % 251;
  return out;
}

/** @param {import("./context.mjs").RequestContext} rc */
export function redirect(rc) {
  const { q, run, site } = rc;
  const n = intParam(q, "n", { def: 1, min: 1, max: MAX_HOPS });
  const code = enumParam(q, "code", CODES, "302");
  const cross = enumParam(q, "cross", ["0", "1"], "0") === "1";
  const alt = enumParam(q, "alt", ["127.0.0.1", "10.0.2.2"], "127.0.0.1");
  const to = q.get("to") ?? "/s/echo";
  if (!SAFE_TARGET.test(to) || to.includes("\\")) throw new HostError(400, "BAD_PARAM", "parameter to must be a path");

  let next = to;
  if (n > 1) {
    const p = new URLSearchParams();
    if (run !== null) p.set("run", run);
    p.set("n", String(n - 1));
    p.set("code", code);
    p.set("to", to);
    if (cross) {
      p.set("cross", "1");
      p.set("alt", alt);
    }
    next = `/s/redirect?${p}`;
  }
  let location = next;
  if (cross) {
    const other = site.site === "A" ? alt : "localhost";
    location = `https://${other}${site.port === null ? "" : `:${site.port}`}${next}`;
  }
  rc.send(Number(code), [["Location", location], ["Content-Type", "text/plain; charset=utf-8"]], "redirect\n");
}

/** @param {import("./context.mjs").RequestContext} rc */
export function cookieSet(rc) {
  const cookie = cookieFromQuery(rc.q, { httpOnly: true });
  return cookieSetAnswer(rc, cookie);
}

/**
 * The cookie string of the name, value and attrs parameters (shared by cookie/set and cookie/js).
 * @param {URLSearchParams} q @param {{ httpOnly: boolean }} rules whether HttpOnly is allowed
 */
export function cookieFromQuery(q, { httpOnly }) {
  const name = q.get("name") ?? "";
  const value = q.get("value") ?? "";
  if (!SYNTHETIC.test(name) || !SYNTHETIC.test(value)) {
    throw new HostError(400, "BAD_PARAM", "name and value must match [a-z0-9-]{1,64}");
  }
  const attrs = [];
  const seen = new Set();
  const list = q.get("attrs") ?? "";
  for (const raw of list === "" ? [] : list.split(",")) {
    const m = /^(HttpOnly|Secure|SameSite=(?:Strict|Lax|None)|Max-Age=\d{1,10})$/.exec(raw);
    if (m === null) throw new HostError(400, "BAD_PARAM", "attrs has an unknown attribute");
    if (raw === "HttpOnly" && !httpOnly) throw new HostError(400, "BAD_PARAM", "attrs must not contain HttpOnly here");
    const key = raw.split("=")[0];
    if (seen.has(key)) throw new HostError(400, "BAD_PARAM", "attrs repeats an attribute");
    seen.add(key);
    attrs.push(raw);
  }
  return [`${name}=${value}`, "Path=/", ...attrs].join("; ");
}

/** @param {import("./context.mjs").RequestContext} rc @param {string} cookie */
function cookieSetAnswer(rc, cookie) {
  const body = '<!doctype html><meta charset="utf-8"><title>cookie</title><p>cookie set</p>' +
    "<script>if(window.rqLab){rqLab.probe(\"location\",document.location.href);" +
    'rqLab.probe("origin",window.origin);rqLab.probe("cookie",document.cookie)}</script>';
  rc.send(200, [["Content-Type", HTML], ["Set-Cookie", cookie]], body);
}

const escapeHtml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** @param {import("./context.mjs").RequestContext} rc */
export function echo(rc) {
  const { req, site, q } = rc;
  const json = JSON.stringify({
    cookie: req.headers.cookie ?? null,
    fields: Object.keys(req.headers).sort(),
    method: req.method,
    site: site.site,
    target: req.url,
  });
  if (q.get("html") === "1") {
    rc.send(200, [["Content-Type", HTML]], `<!doctype html><meta charset="utf-8"><pre id="echo">${escapeHtml(json)}</pre>`);
  } else {
    rc.send(200, [["Content-Type", "application/json"]], json);
  }
}

/** @param {string | undefined} header @param {number} size @returns {{ from: number, to: number } | null | "bad"} */
export function parseRange(header, size) {
  if (header === undefined) return null;
  const suffix = /^bytes=-(\d{1,15})$/.exec(header);
  if (suffix !== null) {
    const n = Number(suffix[1]);
    return n === 0 ? "bad" : { from: Math.max(0, size - n), to: size - 1 };
  }
  const m = /^bytes=(\d{1,15})-(\d{0,15})$/.exec(header);
  if (m === null) return "bad";
  const from = Number(m[1]);
  const to = m[2] === "" ? size - 1 : Math.min(Number(m[2]), size - 1);
  if (from >= size || from > to) return "bad";
  return { from, to };
}

/** @param {import("./context.mjs").RequestContext} rc */
export function range(rc) {
  const size = intParam(rc.q, "size", { def: 1024, min: 1, max: MAX_SIZE });
  const r = parseRange(rc.req.headers.range, size);
  const base = [["Accept-Ranges", "bytes"], ["Content-Type", "application/octet-stream"]];
  if (r === "bad") {
    rc.send(416, [...base, ["Content-Range", `bytes */${size}`]], "");
  } else if (r === null) {
    rc.send(200, base, labBytes(0, size));
  } else {
    rc.send(206, [...base, ["Content-Range", `bytes ${r.from}-${r.to}/${size}`]], labBytes(r.from, r.to + 1));
  }
}

const CUT_HEAD = "<!doctype html><meta charset=\"utf-8\"><title>cut</title><p>";

/** @param {import("./context.mjs").RequestContext} rc */
export function cut(rc) {
  const size = intParam(rc.q, "size", { def: 4096, min: CUT_HEAD.length + 1, max: MAX_SIZE });
  const after = intParam(rc.q, "after", { min: 0, max: size - 1 });
  const body = Buffer.from(CUT_HEAD + "x".repeat(size - CUT_HEAD.length));
  rc.head(200, [["Content-Type", HTML], ["Content-Length", String(size)]]);
  rc.res.flushHeaders();
  const socket = rc.req.socket;
  rc.res.write(body.subarray(0, after), () => {
    rc.log("host.cut", { after });
    socket.destroy();
  });
}
