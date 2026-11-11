<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

# Rivqen fuzz targets

Fuzz targets for the parsers of `rivqen-proto` (DL-148, DL-154). This directory is its own Cargo workspace. It is not part of the root workspace or of a release build.

## Requirements

- The nightly toolchain in `rust-toolchain.toml` (`nightly-2026-11-01`). Install it with `rustup toolchain install nightly-2026-11-01 --profile minimal`.
- `cargo-fuzz` 0.13.2: `cargo install --locked cargo-fuzz --version 0.13.2`.

## Targets

| Target | Function under test | Invariant beyond "no panic" |
|---|---|---|
| `sf_fields` | `sf::parse_item`, `parse_list`, `parse_dictionary` on the same UTF-8 text | A parsed value serializes, and the text parses again to the same value |
| `json_reader` | `json::Reader::next_event` to the end | After an error the reader returns the same error; events are not more than input bytes |
| `manifest` | `manifest::parse_manifest` | none |
| `envelope` | `envelope::parse_envelope`, limit 65 536, manifest limits `None`, empty and 1 024 | `TooLarge` if and only if the body is over `envelope_limit` |

## Build

```sh
cd fuzz
cargo fuzz build
```

## Seed

The seed corpora are not committed (`.gitignore`). Make them with:

```sh
cd fuzz
sh seed.sh
```

The script copies files from `fixtures/malformed/seeds/fields`, `spec/rqp/examples` and the `response.body` files of `fixtures/rqp/protocol` into `corpus/<target>/`. It only reads the fixtures. You can run it again.

## Run

Run the seed corpus once:

```sh
cd fuzz
for t in sf_fields json_reader manifest envelope; do cargo fuzz run $t -- -runs=0; done
```

Fuzz one target for 60 seconds. Run one target at a time:

```sh
cd fuzz
cargo fuzz run envelope -- -max_total_time=60
```

If a target crashes, libFuzzer writes the input to `artifacts/<target>/`. Do not fix the input. Keep the file, open a finding, and add a regression test to the parser crate.
