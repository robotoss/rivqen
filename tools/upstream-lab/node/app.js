// Test harness: serves fixture pages through the UNMODIFIED upstream compress
// middleware (which calls the published sonic_differ package).
const Koa = require('koa');
const through2 = require('through2');
const fs = require('fs');
const compress = require('/upstream/sonic-nodejs/middleware/compress.js');
const app = new Koa();
app.use(compress());
app.use(async (ctx) => {
  const name = ctx.path.replace(/^\/p\//, '').replace(/[^a-z]/g, '');
  const f = `/pages/${name}.html`;
  if (!fs.existsSync(f)) { ctx.status = 404; return; }
  ctx.body = through2();
  ctx.type = 'html';
  ctx.body.end(fs.readFileSync(f));
});
app.listen(8080);
