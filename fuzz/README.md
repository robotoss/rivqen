<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

# Rivqen fuzz targets

Fuzz targets for the parsers of `rivqen-proto` and the template engine `rivqen-template` (DL-148, DL-154, DL-165). This directory is its own Cargo workspace. It is not part of the root workspace or of a release build.

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
| `split_v1` | `legacy::split_v1` with default limits, then `render_v1` on an applied split | `{title}` is the first entry; the rebuild equals the input when each key stands once in the template (P-TPL-1, DL-166, DL-167); outside that domain the rebuild may differ or give `Mismatch`; the page is at most `max_document_bytes` |
| `render_v1` | `legacy::render_v1` on a template and data built from the bytes (split at U+0001 and U+0002), default and small limits | An `Ok` page is at most `max_document_bytes` |
| `split_rqp` | `rqp::split_rqp` with default limits | On `Ok`: blocks in order, not overlapping, in bounds, at most 256; the template and the contents give the input back |

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

The script copies files from `fixtures/malformed/seeds/fields`, `spec/rqp/examples` and the `response.body` files of `fixtures/rqp/protocol` into `corpus/<target>/`. For the template targets it copies the pages of `fixtures/legacy/pages`, `fixtures/malformed/legacy/pages`, `fixtures/malformed/seeds/pages` and `fixtures/rqp/protocol/pages`, and `fixtures/rqp/markup/*/input.html`. It only reads the fixtures. You can run it again.

## Run

Run the seed corpus once:

```sh
cd fuzz
for t in sf_fields json_reader manifest envelope split_v1 render_v1 split_rqp; do cargo fuzz run $t -- -runs=0; done
```

Fuzz one target for 60 seconds. Run one target at a time:

```sh
cd fuzz
cargo fuzz run envelope -- -max_total_time=60
```

If a target crashes, libFuzzer writes the input to `artifacts/<target>/`. Do not fix the input. Keep the file, open a finding, and add a regression test to the parser crate.
