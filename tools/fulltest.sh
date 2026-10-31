#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Full repository run: every test and check of the code packages.
# CI (.github/workflows/checks.yml) runs the same groups.
#
# Usage: sh tools/fulltest.sh [group ...]
#   Groups: node python rust java php diff docs. No group: all groups.
#
# Requirements per group (install the dependencies first, see each package README):
#   node    Node.js 22 or later; `npm ci --ignore-scripts` in tools/rqp-protocol,
#           tools/rqp-schema, tools/rqp-markup/node, tools/rqp-markup/crosscheck,
#           tools/webview-lab and examples/server-demo
#   python  Python 3
#   rust    cargo with rustfmt and clippy
#   java    JDK 21 and `gradle` on PATH (optional: ORG_GRADLE_PROJECT_rqpMavenMirror)
#   php     PHP 8.3 or later; `composer install` in tools/rqp-markup/php
#   diff    all four markup candidates (rust, java, php, node groups)
#   docs    `npm ci` in the repository root
#
# Exit code: 0 when every step passes, 1 otherwise. The log of a failed step is printed.

set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
LOGS=$(mktemp -d "${TMPDIR:-/tmp}/rivqen-fulltest.XXXXXX")
trap 'rm -rf "$LOGS"' EXIT
FAILED=0

# step <name> <command>: run the command (in a subshell, from ROOT) and report the result.
step() {
  name=$1
  shift
  if (cd "$ROOT" && eval "$*") >"$LOGS/$name.log" 2>&1; then
    echo "ok    $name"
  else
    echo "FAIL  $name"
    tail -n 40 "$LOGS/$name.log" | sed 's/^/      /'
    FAILED=1
  fi
}

group_node() {
  step rqp-protocol 'cd tools/rqp-protocol && npm test'
  step rqp-protocol-gen 'node tools/rqp-protocol/gen.mjs --check'
  step rqp-protocol-check 'node tools/rqp-protocol/check.mjs'
  step server-demo 'cd examples/server-demo && npm test'
  step server-demo-fixtures 'node tools/rqp-protocol/check.mjs --server examples/server-demo/src/rqp.js'
  step rqp-schema 'cd tools/rqp-schema && npm test'
  step markup-node 'cd tools/rqp-markup/node && npm test'
  step crosscheck 'cd tools/rqp-markup/crosscheck && npm test'
  step corpus 'node --test "tools/rqp-markup/corpus/test/*.test.mjs"'
  step study 'node --test "tools/rqp-markup/study/relax/test/*.test.mjs" "tools/rqp-markup/study/noscript/test/*.test.mjs" "tools/rqp-markup/study/s4/test/*.test.mjs" "tools/rqp-markup/study/s5/test/*.test.mjs"'
  step bytes-study 'node --test "tools/rqp-protocol/study/bytes/test/*.test.mjs"'
  step webview-lab 'cd tools/webview-lab && npm test'
  step webview-lab-cases 'node tools/webview-lab/host/build-cases.mjs --check'
  step mutation-helper 'node --test "tools/mutation/test/*.test.mjs"'
  step legacy-fixtures 'cd tools/legacy-fixtures && npm test'
  step legacy-gen 'node tools/legacy-fixtures/gen.mjs --check'
  step conformance 'cd server/conformance && npm test'
  step conformance-rqp-http 'node server/conformance/e2e/run.mjs --module tools/rqp-protocol/src/server.mjs --suite rqp'
  step conformance-demo-http 'node server/conformance/e2e/run.mjs --module examples/server-demo/src/rqp.js --suite rqp'
  step conformance-legacy-http 'for p in legacy-canonical legacy-java legacy-node legacy-php; do node server/conformance/e2e/run.mjs --module tools/legacy-fixtures/src/server.mjs --suite legacy --profile "$p" || exit 1; done'
  step conformance-fails-http 'for m in broken app-only; do node server/conformance/e2e/run.mjs --module "tools/rqp-protocol/test/fixtures/$m-server.mjs" --suite rqp --expect fail || exit 1; done'
  step malformed-gen 'node tools/legacy-fixtures/gen.mjs --check --dir fixtures/malformed/legacy'
  step malformed-http 'node server/conformance/e2e/run.mjs --module tools/legacy-fixtures/src/server.mjs --suite legacy -- --fixtures fixtures/malformed/legacy'
  step upstream-lab-unit 'node --check tools/upstream-lab/node/app.js && node --test tools/upstream-lab/node/control.test.cjs'
}

group_python() {
  step markup-gen 'python3 tools/rqp-markup/gen.py --check'
  step markup-gen-unit 'python3 -m unittest tools/rqp-markup/test_gen.py'
}

group_rust() {
  step rust 'cd tools/rqp-markup/rust && cargo fmt --check && cargo clippy --all-targets -q -- -D warnings && RUST_TEST_THREADS=4 cargo test -q'
}

group_java() {
  step java 'cd tools/rqp-markup/java && gradle --quiet spotlessCheck build installDist'
}

group_php() {
  step php 'cd tools/rqp-markup/php && vendor/bin/phpunit && vendor/bin/phpstan analyse --no-progress'
}

group_diff() {
  step markup-runner-unit 'node --test "tools/rqp-markup/test/*.test.mjs"'
  step markup-diff 'node tools/rqp-markup/diff.mjs'
}

group_docs() {
  step docs 'npm run docs:build'
}

[ $# -eq 0 ] && set -- node python rust java php diff docs
for g in "$@"; do
  case $g in
    node | python | rust | java | php | diff | docs) "group_$g" ;;
    *)
      echo "fulltest: unknown group '$g'" >&2
      exit 2
      ;;
  esac
done
exit $FAILED
