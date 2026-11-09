#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Acceptance of WP-05 (DL-147): every crate of the core workspace forbids
# unsafe code, except rivqen-ffi (R-CORE-10). A crate passes when its lib.rs
# (or main.rs) has `#![forbid(unsafe_code)]` and its Cargo.toml uses the
# workspace lints (`[lints]` with `workspace = true`, where unsafe_code is
# forbid).
#
# Usage: sh tools/rust-core/check-unsafe.sh   (from any directory)

set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
status=0
count=0
for dir in "$ROOT"/crates/*/; do
  name=$(basename "$dir")
  [ "$name" = "rivqen-ffi" ] && continue
  count=$((count + 1))
  root_file="$dir/src/lib.rs"
  [ -f "$root_file" ] || root_file="$dir/src/main.rs"
  if ! grep -q '^#!\[forbid(unsafe_code)\]' "$root_file" 2>/dev/null; then
    echo "check-unsafe: $name: no #![forbid(unsafe_code)] in src/lib.rs or src/main.rs" >&2
    status=1
  fi
  if ! awk '/^\[lints\]/{l=1; next} /^\[/{l=0} l && /^workspace *= *true/{f=1} END{exit !f}' "$dir/Cargo.toml"; then
    echo "check-unsafe: $name: Cargo.toml does not use the workspace lints" >&2
    status=1
  fi
done
if [ "$count" -eq 0 ]; then
  echo "check-unsafe: no crates found under crates/" >&2
  exit 1
fi
[ "$status" -eq 0 ] && echo "check-unsafe: $count crates forbid unsafe code"
exit $status
