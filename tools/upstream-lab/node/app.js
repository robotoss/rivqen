// Test harness: serves fixture pages through the UNMODIFIED upstream compress
// middleware (which calls the published sonic_differ package).
//
// Two routes:
//  - /p/<page>: a stored fixture page (traces S01-S21).
//  - every other path (not a control path): the case that the runner stored through the control
//    endpoint (server/conformance/CONTRACT.md section 2). It is off unless RIVQEN_CONFORMANCE=1.
const http = require('http');
const Koa = require('koa');
const through2 = require('through2');
const fs = require('fs');
const compress = require('/upstream/sonic-nodejs/middleware/compress.js');
const control = require('./control.cjs');

const PROFILE = 'legacy-node';
const CONFORMANCE = process.env.RIVQEN_CONFORMANCE === '1';
const HOST_PORT = process.env.LAB_HOST_PORT || ''; // the published port; empty => every control request is refused (403)
let stored = null; // one case at a time

const app = new Koa();
app.use(compress());
app.use(async (ctx) => {
  if (ctx.path.startsWith('/p/')) {
    const name = ctx.path.replace(/^\/p\//, '').replace(/[^a-z]/g, '');
    const f = `/pages/${name}.html`;
    if (!fs.existsSync(f)) { ctx.status = 404; return; }
    ctx.body = through2();
    ctx.type = 'html';
    ctx.body.end(fs.readFileSync(f));
    return;
  }
  const c = stored;
  if (c === null) { ctx.status = 503; ctx.type = 'json'; ctx.body = '{"error":"NO_CASE"}'; return; }
  // What an application does: status, field lines, then the body bytes. The upstream code acts on it.
  for (const [name, value] of control.groupHeaders(c.headers)) ctx.set(name, value);
  ctx.status = c.status;
  ctx.body = through2();
  ctx.body.end(c.page);
});
app.on('error', (err) => {
  process.stderr.write(`lab error: ${err && err.code ? err.code : 'Error'}\n`);
});

function sendJson(res, status, value) {
  const body = Buffer.from(JSON.stringify(value), 'utf8');
  res.writeHead(status, { 'Content-Type': 'application/json', 'Content-Length': String(body.length) });
  res.end(body);
}

// Reads the whole body but keeps at most `limit` bytes; an over-limit body is read to its end and dropped.
function readBody(req, limit) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on('data', (chunk) => {
      size += chunk.length;
      if (size <= limit) chunks.push(chunk); else chunks.length = 0;
    });
    req.on('end', () => (size > limit ? resolve({ tooLarge: true }) : resolve({ text: Buffer.concat(chunks).toString('utf8') })));
    req.on('error', reject);
  });
}

async function handleControl(req, res, path) {
  if (!CONFORMANCE) return sendJson(res, 404, { error: 'NOT_FOUND' });
  // Lab exception (DL-081): the peer is the bridge address, so check Host and Content-Type only.
  if (!control.isControlHost(req.headers.host, HOST_PORT)) return sendJson(res, 403, { error: 'FORBIDDEN' });
  if (path === control.INFO_PATH) {
    if (req.method !== 'GET') return sendJson(res, 405, { error: 'METHOD_NOT_ALLOWED' });
    return sendJson(res, 200, { contract: 1, suites: ['legacy'], profiles: [PROFILE] });
  }
  if (path === control.CASE_PATH) {
    if (req.method !== 'POST') return sendJson(res, 405, { error: 'METHOD_NOT_ALLOWED' });
    if (!control.isJsonContentType(req.headers['content-type'])) return sendJson(res, 415, { error: 'UNSUPPORTED_MEDIA_TYPE' });
    const body = await readBody(req, control.MAX_CASE_BYTES);
    if (body.tooLarge) return sendJson(res, 413, { error: 'TOO_LARGE' });
    const parsed = control.parseCase(body.text, PROFILE);
    if (parsed.error === 'BAD_CASE') return sendJson(res, 400, { error: 'BAD_CASE' });
    if (parsed.error === 'UNSUPPORTED') return sendJson(res, 422, { error: 'UNSUPPORTED' });
    stored = parsed.case;
    res.writeHead(204);
    res.end();
    return;
  }
  return sendJson(res, 404, { error: 'NOT_FOUND' });
}

const callback = app.callback();
http.createServer((req, res) => {
  const path = (req.url || '').split('?')[0];
  if (!path.startsWith(control.CONTROL_PREFIX)) { callback(req, res); return; }
  handleControl(req, res, path).catch((err) => {
    process.stderr.write(`control error: ${err && err.code ? err.code : 'Error'}\n`);
    if (!res.headersSent) sendJson(res, 500, { error: 'HARNESS_FAILED' }); else res.destroy();
  });
}).listen(8080);
