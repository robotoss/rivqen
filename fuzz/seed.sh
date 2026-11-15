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
for t in split_v1 render_v1 split_rqp; do
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
