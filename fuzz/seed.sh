#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Copies the seed inputs from the fixtures into fuzz/corpus/<target>/.
# It only reads the fixtures. Run it again at any time.
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

for f in "$root"/fixtures/malformed/seeds/fields/*.txt; do
    copy sf_fields "$f"
done
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
