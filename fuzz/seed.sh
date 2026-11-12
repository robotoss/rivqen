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
