// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The S2 routes of CONTRACT.md section 3: sw.js, sw-page, sw-probe, csp, hist, cookie/js.
// Page scripts report through `window.rqLab.probe(name, value)` (section 6).

import { cookieFromQuery } from "./scenarios.mjs";
import { nonceOf } from "./stream.mjs";
import { enumParam, intParam } from "./util.mjs";

const HTML = "text/html; charset=utf-8";
const HIST_LAST = 9;
const SW_READY_MS = 10_000;

/** The Service Worker: answers /s/sw-probe itself; every other request goes to the network. */
export const SW_SOURCE = `self.addEventListener("install", function () { self.skipWaiting(); });
self.addEventListener("activate", function (e) { e.waitUntil(self.clients.claim()); });
self.addEventListener("fetch", function (e) {
  if (new URL(e.request.url).pathname === "/s/sw-probe") {
    e.respondWith(new Response('{"sw":"worker"}', { status: 200, headers: { "Content-Type": "application/json" } }));
  }
});
`;

const PROBE_FN = 'var p=function(n,v){window.rqLab&&window.rqLab.probe(n,v)};';
const runQuery = (run) => (run === null ? "" : `run=${run}`);

/** @param {import("./context.mjs").RequestContext} rc */
export function swScript(rc) {
  rc.send(200, [["Content-Type", "text/javascript; charset=utf-8"], ["Service-Worker-Allowed", "/s/"]], SW_SOURCE);
}

/** @param {import("./context.mjs").RequestContext} rc */
export function swProbe(rc) {
  rc.send(200, [["Content-Type", "application/json"]], '{"sw":"network"}');
}

/** The script of /s/sw-page for a run. @param {string | null} run */
export function swPageScript(run) {
  const probeUrl = `/s/sw-probe?${runQuery(run)}`;
  const again = `${run === null ? "" : `run=${run}&`}again=1`;
  return `(function(){${PROBE_FN}` +
    'if(!navigator.serviceWorker){p("sw","unsupported");return}' +
    `var limit=new Promise(function(_,no){setTimeout(function(){no({name:"Timeout"})},${SW_READY_MS})});` +
    'navigator.serviceWorker.register("/s/sw.js",{scope:"/s/"})' +
    ".then(function(){return Promise.race([navigator.serviceWorker.ready,limit])})" +
    ".then(function(){" +
    `if(!navigator.serviceWorker.controller&&location.search.indexOf("again=1")<0){location.replace(location.pathname+"?"+${JSON.stringify(again)});return}` +
    `return fetch(${JSON.stringify(probeUrl)}).then(function(r){return r.json()}).then(function(j){p("sw",j.sw)})})` +
    '.catch(function(e){p("sw",e&&e.name?e.name:"error")})})();';
}

/** @param {import("./context.mjs").RequestContext} rc */
export function swPage(rc) {
  const body = `<!doctype html><meta charset="utf-8"><title>sw</title><p>sw</p><script>${swPageScript(rc.run)}</script>`;
  rc.send(200, [["Content-Type", HTML]], body);
}

/** @param {import("./context.mjs").RequestContext} rc */
export function csp(rc) {
  const mode = enumParam(rc.q, "mode", ["header", "meta", "both"], "header");
  const nonce = nonceOf(rc.run);
  const policy = `script-src 'nonce-${nonce}'`;
  const meta = mode === "header" ? "" : `<meta http-equiv="Content-Security-Policy" content="${policy}">`;
  const body = `<!doctype html><html><head>${meta}<meta charset="utf-8"><title>csp</title></head><body><p>csp</p>` +
    `<script nonce="${nonce}">${PROBE_FN}p("csp.nonce",true);` +
    'try{eval("1");p("csp.eval","allowed")}catch(e){p("csp.eval","blocked")}</script>' +
    `<script>${PROBE_FN}p("csp.inline",true)</script></body></html>`;
  /** @type {[string, string][]} */
  const fields = [["Content-Type", HTML]];
  if (mode !== "meta") fields.push(["Content-Security-Policy", policy]);
  rc.send(200, fields, body);
}

/** @param {import("./context.mjs").RequestContext} rc */
export function hist(rc) {
  const step = intParam(rc.q, "step", { def: 0, min: 0, max: HIST_LAST });
  const next = `/s/hist?${rc.run === null ? "" : `run=${rc.run}&`}step=${step + 1}`;
  const body = `<!doctype html><meta charset="utf-8"><title>hist ${step}</title><p>step ${step}</p>` +
    `<a id="next" href="${next}">next</a>` +
    `<script>${PROBE_FN}p("history",history.length);p("location",document.location.href)</script>`;
  rc.send(200, [["Content-Type", HTML]], body);
}

/** @param {import("./context.mjs").RequestContext} rc */
export function cookieJs(rc) {
  const cookie = cookieFromQuery(rc.q, { httpOnly: false });
  const body = '<!doctype html><meta charset="utf-8"><title>cookie js</title><p>cookie js</p>' +
    `<script>document.cookie=${JSON.stringify(cookie)};${PROBE_FN}p("cookie",document.cookie)</script>`;
  rc.send(200, [["Content-Type", HTML]], body);
}
