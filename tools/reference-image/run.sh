#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Build the container image of the reference server, run it with the port published on
# 127.0.0.1 only, and run the conformance runner against it by URL: the RQP suite, the four legacy
# profiles and the malformed legacy suite. The container is removed on any exit.
#
# Usage: sh tools/reference-image/run.sh
# Env:   IMAGE=<ref>         test this image and do not build (default: build rivqen-reference:local)
#        PORT=<n>            host and container port (default 18080; the same number on both sides)
#        START_TIMEOUT=<s>   seconds to wait for the server (default 60)
# Exit:  the largest runner exit code (0 all passed, 1 some failed, 2 usage error or no server).
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
RUNNER="$ROOT/server/conformance/bin/rivqen-conformance.mjs"
PORT="${PORT:-18080}"
START_TIMEOUT="${START_TIMEOUT:-60}"
NAME="rivqen-reference-test-$$"

if [ -z "${IMAGE:-}" ]; then
  IMAGE="rivqen-reference:local"
  docker build -f "$ROOT/server/rust-reference/Dockerfile" -t "$IMAGE" "$ROOT"
fi

# A new control token for each run (H-117): a file that only this run reads.
TOKDIR="$(mktemp -d)"
cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  rm -rf "$TOKDIR"
}
trap cleanup EXIT
trap 'exit 130' INT TERM
node -e 'process.stdout.write(require("node:crypto").randomBytes(32).toString("base64url"))' > "$TOKDIR/token"
# The server runs as user 65532 in the container, so the file must be readable for it.
chmod 0644 "$TOKDIR/token"

# No writable root file system, no capabilities, no privilege change: the server
# needs none of them.
docker run -d --name "$NAME" -p "127.0.0.1:$PORT:$PORT" \
  --read-only --cap-drop ALL --security-opt no-new-privileges \
  -v "$TOKDIR/token:/run/rivqen/token:ro" "$IMAGE" \
  --conformance --listen 0.0.0.0 --port "$PORT" --lab-peer --control-token-file /run/rivqen/token >/dev/null

# Wait until the control endpoint answers (no fixed sleep between runs, only between polls).
i=0
until node -e '
const [port, file] = process.argv.slice(1);
const token = require("node:fs").readFileSync(file, "utf8");
fetch("http://127.0.0.1:" + port + "/__rivqen/conformance/info", { headers: { Host: "127.0.0.1:" + port, "Rivqen-Conformance-Token": token }, signal: AbortSignal.timeout(2000) })
  .then((r) => process.exit(r.ok ? 0 : 1), () => process.exit(1));
' "$PORT" "$TOKDIR/token"; do
  i=$((i + 1))
  if [ "$i" -ge "$START_TIMEOUT" ]; then
    echo "server did not become ready in ${START_TIMEOUT}s" >&2
    docker logs "$NAME" >&2 || true
    exit 2
  fi
  if [ "$(docker inspect -f '{{.State.Running}}' "$NAME" 2>/dev/null)" != "true" ]; then
    echo "container stopped" >&2
    docker logs "$NAME" >&2 || true
    exit 2
  fi
  sleep 1
done

# Without the token the control endpoint must refuse (H-117).
if ! node -e '
const port = process.argv[1];
fetch("http://127.0.0.1:" + port + "/__rivqen/conformance/info", { headers: { Host: "127.0.0.1:" + port }, signal: AbortSignal.timeout(2000) })
  .then((r) => process.exit(r.status === 403 ? 0 : 1), () => process.exit(1));
' "$PORT"; then
  echo "the control endpoint answered without the token" >&2
  exit 1
fi
echo "== control endpoint without the token: 403"

TARGET="http://127.0.0.1:$PORT"
worst=0
run() {
  echo "== $*"
  code=0
  (cd "$ROOT" && node "$RUNNER" --target "$TARGET" --control-token-file "$TOKDIR/token" "$@") || code=$?
  if [ "$code" -gt "$worst" ]; then worst="$code"; fi
}
run --suite rqp
for p in legacy-canonical legacy-java legacy-node legacy-php; do
  run --suite legacy --profile "$p"
done
run --suite legacy --fixtures fixtures/malformed/legacy

echo "== container log (first and last lines)"
docker logs "$NAME" 2>&1 | sed -n '1,3p' || true
docker logs --tail 3 "$NAME" 2>&1 || true
exit "$worst"
