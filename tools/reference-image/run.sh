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

cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT TERM

docker run -d --name "$NAME" -p "127.0.0.1:$PORT:$PORT" "$IMAGE" \
  --conformance --listen 0.0.0.0 --port "$PORT" --lab-peer >/dev/null

# Wait until the control endpoint answers (no fixed sleep between runs, only between polls).
i=0
until node -e '
const port = process.argv[1];
fetch("http://127.0.0.1:" + port + "/__rivqen/conformance/info", { headers: { Host: "127.0.0.1:" + port }, signal: AbortSignal.timeout(2000) })
  .then((r) => process.exit(r.ok ? 0 : 1), () => process.exit(1));
' "$PORT"; do
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

TARGET="http://127.0.0.1:$PORT"
worst=0
run() {
  echo "== $*"
  code=0
  (cd "$ROOT" && node "$RUNNER" --target "$TARGET" "$@") || code=$?
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
