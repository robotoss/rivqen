#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Run the conformance runner against the three upstream lab servers (started by start.sh) and write
# one combined evidence file: evidence/conformance/upstream-lab-<date>.json
#
# Usage: sh tools/upstream-lab/conformance.sh
# Env:   DATE=<yyyy-mm-dd>   date in the file name (default: today, UTC)
#        OUT_DIR=<dir>       output directory (default: evidence/conformance)
#
# The runner exits 1 when a MUST case fails; that is a finding, not an error of this script, so the
# script goes on and writes all three reports. Its own exit code is the largest runner exit code
# (0 all passed, 1 some failed, 2 usage error or server not reachable).
set -eu
LAB="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$LAB/../.." && pwd)"
DATE="${DATE:-$(date -u +%Y-%m-%d)}"
OUT_DIR="${OUT_DIR:-$ROOT/evidence/conformance}"
RUNNER="$ROOT/server/conformance/bin/rivqen-conformance.mjs"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$OUT_DIR"

worst=0
for pair in legacy-node:18081 legacy-php:18082 legacy-java:18083; do
  profile="${pair%%:*}"
  port="${pair##*:}"
  echo "== $profile on 127.0.0.1:$port"
  code=0
  node "$RUNNER" --target "http://127.0.0.1:$port" --suite legacy --profile "$profile" \
    --json "$TMP/$profile.json" || code=$?
  echo "$code" > "$TMP/$profile.code"
  if [ "$code" -gt "$worst" ]; then worst="$code"; fi
done

OUT="$OUT_DIR/upstream-lab-$DATE.json"
# Combine: the runner's --json report per profile (null when the runner wrote none) and its exit code.
node --input-type=module -e '
import { readFileSync, writeFileSync, existsSync } from "node:fs";
const [tmp, out, date] = process.argv.slice(1);
const profiles = {};
for (const p of ["legacy-node", "legacy-php", "legacy-java"]) {
  const file = `${tmp}/${p}.json`;
  profiles[p] = {
    exit_code: Number(readFileSync(`${tmp}/${p}.code`, "utf8").trim()),
    report: existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : null,
  };
}
const doc = {
  date,
  upstream: { repo: "Tencent/VasSonic", sha: "59936beff656d4b5718ff6444d6c5e001a2c5231" },
  runner: "server/conformance/bin/rivqen-conformance.mjs",
  profiles,
};
writeFileSync(out, JSON.stringify(doc, null, 2) + "\n");
' "$TMP" "$OUT" "$DATE"
echo "wrote $OUT"
exit "$worst"
