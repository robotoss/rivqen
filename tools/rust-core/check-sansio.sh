#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# ADR-001 (DL-180): the core crates are sans-I/O. The src/ of a crate of the
# core workspace must not use files, sockets, threads, processes, the
# environment or the system time, except the crates of the list below:
#   rivqen-testkit  reads fixture files in tests
#   rivqen-store    the reference storage driver (adapter code in Rust)
#   rivqen-ffi      the FFI layer (WP-09)
# Time comes in through rivqen_model::time (Clock, UnixTime).
#
# Usage: sh tools/rust-core/check-sansio.sh   (from any directory)

set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
PATTERN='std::(fs|net|thread|process|env)([^A-Za-z0-9_]|$)|std::time::(SystemTime|Instant)|std::os::|SystemTime::|Instant::now'
status=0
count=0
for dir in "$ROOT"/crates/*/; do
  name=$(basename "$dir")
  case $name in
    rivqen-testkit | rivqen-store | rivqen-ffi) continue ;;
  esac
  count=$((count + 1))
  if found=$(grep -rnE "$PATTERN" "$dir/src" 2>/dev/null); then
    echo "check-sansio: $name uses I/O or the system time in src/:" >&2
    echo "$found" | sed 's/^/  /' >&2
    status=1
  fi
done
if [ "$count" -eq 0 ]; then
  echo "check-sansio: no crates found under crates/" >&2
  exit 1
fi
[ "$status" -eq 0 ] && echo "check-sansio: $count crates have no I/O in src/"
exit $status
