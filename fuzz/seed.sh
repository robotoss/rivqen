#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Copies the seed inputs from the fixtures into fuzz/corpus/<target>/, and
# writes the RFC examples of the cache field targets. It only reads the
# fixtures. Run it again at any time.
set -eu
cd "$(dirname "$0")"
root=..

copy() { # target source-file
    dest="corpus/$1"
    mkdir -p "$dest"
    # A unique, stable file name from the source path.
    name=$(printf '%s' "$2" | sed 's#^\.\./##; s#[/ ]#__#g')
    cp "$2" "$dest/$name"
}

put() { # target name text: writes one seed that no fixture has
    mkdir -p "corpus/$1"
    printf '%s' "$3" > "corpus/$1/$2"
}

for f in "$root"/fixtures/malformed/seeds/fields/*.txt; do
    copy sf_fields "$f"
done

# Cache field parsers (WP-07): the field seeds of the cache fields, and the
# examples of RFC 9111, RFC 5861 and RFC 9110 section 5.6.7. In the
# cache_control target the first LF splits two field lines.
for f in "$root"/fixtures/malformed/seeds/fields/cache-*.txt \
    "$root"/fixtures/malformed/seeds/fields/vary-*.txt; do
    copy cache_control "$f"
done
put cache_control rfc5861-swr 'max-age=600, stale-while-revalidate=30'
put cache_control rfc5861-sie 'max-age=600, stale-if-error=1200'
put cache_control rfc9111-community 'private, community="UCI"'
put cache_control quoted-no-cache 'no-cache="set-cookie, x-a", max-age="60"'
put cache_control two-lines 'public, max-age=60
no-store=1, private="a, no-store"'
put cache_control dl088-split 'max-age=5; no-store, x"y, must-revalidate"'
put cache_control space-separated-no-cache 'no-cache no-store'
put cache_control space-separated-private 'private no-store'
put cache_control age-list '5, 10'
put cache_control vary-list 'Accept-Encoding, accept-language, *'
put http_date imf-fixdate 'Sun, 06 Nov 1994 08:49:37 GMT'
put http_date rfc850 'Sunday, 06-Nov-94 08:49:37 GMT'
put http_date asctime 'Sun Nov  6 08:49:37 1994'
put http_date leap-second 'Sat, 31 Dec 2016 23:59:60 GMT'
put http_date leap-day 'Thu, 29 Feb 2024 00:00:00 GMT'
put http_date last 'Fri, 31 Dec 9999 23:59:59 GMT'
put http_date expires-zero '0'
for d in "$root"/spec/rqp/examples/valid/manifest "$root"/spec/rqp/examples/invalid/manifest; do
    for f in "$d"/*.json; do
        copy manifest "$f"
        copy json_reader "$f"
    done
done
for d in "$root"/spec/rqp/examples/valid/patch "$root"/spec/rqp/examples/invalid/patch; do
    for f in "$d"/*.json; do
        copy envelope "$f"
        copy json_reader "$f"
    done
done
for f in "$root"/fixtures/rqp/protocol/*/response.body; do
    copy envelope "$f"
    copy json_reader "$f"
done

# Template engine targets: HTML pages of the legacy and malformed fixtures,
# the RQP markup inputs and the protocol pages (any file may be a seed).
for t in split_v1 render_v1 diff_v1 split_rqp; do
    for d in "$root"/fixtures/legacy/pages "$root"/fixtures/malformed/legacy/pages \
        "$root"/fixtures/malformed/seeds/pages; do
        for f in "$d"/*; do
            [ -f "$f" ] && copy "$t" "$f"
        done
    done
    for f in "$root"/fixtures/rqp/markup/*/input.html; do
        [ -f "$f" ] && copy "$t" "$f"
    done
    for f in "$root"/fixtures/rqp/protocol/*/*.html; do
        [ -f "$f" ] && copy "$t" "$f"
    done
done

# patch_content: the content of every delta operation of the WP-06 fixtures of
# the ledger, and the block contents of the protocol pages. Each file is one
# content (the target puts it in the price block of page-a).
node -e '
const fs = require("fs"), path = require("path");
const root = process.argv[1], dest = process.argv[2];
fs.mkdirSync(dest, { recursive: true });
const put = (name, text) => fs.writeFileSync(path.join(dest, name), text);
const ledger = fs.readFileSync(path.join(root, "crates/rivqen-testkit/ledger/rqp-client.txt"), "utf8");
for (const line of ledger.split("\n")) {
  const [id, owner] = line.split(" ");
  if (owner !== "WP-06" || !id.startsWith("FX-")) continue;
  const file = path.join(root, "fixtures/rqp/protocol", id, "response.body");
  if (!fs.existsSync(file)) continue;
  let body;
  try { body = JSON.parse(fs.readFileSync(file, "utf8")); } catch { continue; }
  (body.ops || body.operations || []).forEach((op, i) => {
    if (typeof op.content === "string") put(`op__${id}__${i}`, op.content);
  });
}
const pages = path.join(root, "fixtures/rqp/protocol/pages");
for (const f of fs.readdirSync(pages)) {
  const html = fs.readFileSync(path.join(pages, f), "utf8");
  const re = /<([a-zA-Z0-9]+)\b[^>]*\bdata-rq-block="[^"]*"[^>]*>([\s\S]*?)<\/\1\s*>/g;
  let m, i = 0;
  while ((m = re.exec(html)) !== null) put(`block__${f}__${i++}`, m[2]);
}
' "$root" "corpus/patch_content"

# store_file_decode and store_row_decode (WP-07 S2): plain texts of the entry
# file and of the row of rivqen-store, written from the layouts in
# crates/rivqen-store/src/crypto/file.rs and row.rs. Lengths and counts are
# 8 bytes big-endian; an option is 00, or 01 and the value.
node -e '
const fs = require("fs"), path = require("path");
const [fileDir, rowDir] = process.argv.slice(1);
fs.mkdirSync(fileDir, { recursive: true });
fs.mkdirSync(rowDir, { recursive: true });
const u8 = (v) => Buffer.from([v]);
const u32 = (v) => { const b = Buffer.alloc(4); b.writeUInt32BE(v); return b; };
const u64 = (v) => { const b = Buffer.alloc(8); b.writeBigUInt64BE(BigInt(v)); return b; };
const lp = (s) => { const b = Buffer.from(s); return Buffer.concat([u64(b.length), b]); };
const some = (...parts) => Buffer.concat([u8(1), ...parts]);
const none = u8(0);
const cat = (...parts) => Buffer.concat(parts);
const file = (page, template, blocks) => cat(u8(1), lp(page),
  template === null ? none : some(lp(template)),
  u64(blocks.length), ...blocks.map(([k, v]) => cat(lp(k), lp(v))));
fs.writeFileSync(path.join(fileDir, "legacy"), file("P!", "T", [["k", "vv"], ["{b1}", "<b>x</b>"]]));
fs.writeFileSync(path.join(fileDir, "rqp"), file("<html><div data-rq-block=\"main\">x</div></html>", null, []));
fs.writeFileSync(path.join(fileDir, "empty"), file("", null, []));
fs.writeFileSync(path.join(fileDir, "empty-blocks"), file("", "", [["", ""], ["", ""]]));
// Six booleans: no_store, no_cache, private, valid_private, public,
// must_revalidate (valid_private only with private).
const policy = cat(u8(0), u8(1), u8(1), u8(1), u8(0), u8(0), some(u64(60)), none, some(u64(5)), u8(0),
  u8(2), u64(1000), some(u64(990)), none, u64(980), u64(985), some(u32(30)), u8(1));
const zeroPolicy = cat(Buffer.alloc(6), none, none, none, u8(0), u8(0), none, none, u64(0), u64(0), none, u8(0));
// Row format 2: the host and the store time follow the format byte.
const row = (protocol, vary, etag, rqp, pol, fields, host = "shop.example.com",
  storedAt = 1700000000) => cat(u8(2), lp(host), u64(storedAt), u8(protocol),
  u64(vary.length), ...vary.map(([n, d]) => cat(lp(n), d === null ? none : some(Buffer.alloc(32, d)))),
  etag === null ? none : some(lp(etag)), none, rqp === null ? none : some(rqp), pol,
  u64(fields.length), ...fields.map(([n, v]) => cat(lp(n), lp(v))));
const snapshot = cat(Buffer.alloc(32, 1), Buffer.alloc(32, 2), u64(2),
  lp("main"), u8(0), Buffer.alloc(32, 3), lp("price-1"), u8(1), Buffer.alloc(32, 4),
  some(some(u8(1)), some(u32(600)), some(u8(1))), some(some(u32(65536))));
fs.writeFileSync(path.join(rowDir, "legacy"),
  row(0, [["accept-language", 7], ["x", null]], "abc", null, policy, [["content-type", "text/html"]]));
fs.writeFileSync(path.join(rowDir, "rqp-snapshot"), row(1, [], null, snapshot, policy, []));
fs.writeFileSync(path.join(rowDir, "minimal"), row(1, [], null, null, zeroPolicy, []));
fs.writeFileSync(path.join(rowDir, "empty-items"),
  row(0, [["", null]], "", null, zeroPolicy, [["", ""], ["", ""]], "", 0));
fs.writeFileSync(path.join(rowDir, "host-at-limit"),
  row(1, [], null, null, zeroPolicy, [], "a".repeat(253), "18446744073709551615"));
fs.writeFileSync(path.join(rowDir, "ipv6-host"),
  row(1, [], null, null, zeroPolicy, [], "[2001:db8::1]"));
' "corpus/store_file_decode" "corpus/store_row_decode"

# Legacy response (WP-08 S1): a selector byte (`x`: 2048 bytes, depth 7), the
# head lines, an empty line, the body.
put legacy_response data-update 'xcontent-type: text/html; charset=utf-8
etag: e1
template-tag: t1
template-change: false
cache-offline: true

{"data":{"{title}":"<title>a</title>","{p}":"<!--sonicdiff-p-->1<!--sonicdiff-p-end-->"},"template-tag":"t1","html-sha1":"h","diff":""}'
put legacy_response custom-etag 'xContent-Type: TEXT/HTML
sonic-etag-key: My-ETag
my-etag: W/"c1"
template-tag: t1
template-change: true
cache-offline: store
connection: x-hop
x-hop: 1
set-cookie: a=1

<html></html>'
put legacy_response l1-etag-key 'xcontent-type: text/html
sonic-etag-key: Content-Security-Policy
content-security-policy: e1
template-tag: t1
template-change: false
cache-offline: true
'
put legacy_response l1-connection 'xcontent-type: text/html
connection: close, content-security-policy, x-frame-options
content-security-policy: default-src
x-frame-options: DENY
etag: e1
template-tag: t1
template-change: false
cache-offline: true
'

# Legacy data diff (WP-08 S1): a template, the stored data and the server data.
put diff_v1 data-update "$(printf '<title>T</title>{title} <!--sonicdiff-a-->1<!--sonicdiff-a-end-->\003{title}\002<title>T</title>\001{a}\002x\003{title}\002<title>U</title>\001{a}\002y')"

# URL parser (WP-08 S1): the inputs of the URL interop spike and the known
# answer of manifest and patch section 2.2.
mkdir -p corpus/url_canonical
node -e '
const fs = require("fs");
const [urls, dest] = process.argv.slice(1);
const list = JSON.parse(fs.readFileSync(urls, "utf8"));
const items = Array.isArray(list) ? list : (list.urls || list.cases || list.inputs || []);
items.forEach((item, i) => {
  const input = typeof item === "string" ? item : item.input;
  if (typeof input === "string") fs.writeFileSync(`${dest}/interop-${String(i).padStart(3, "0")}`, input);
});
' "$root/tools/platform-spikes/url-interop/urls.json" corpus/url_canonical
put url_canonical kat-binding-with-query 'https://SHOP.Example.com:443/catalog?sort=price&page=2#reviews'
