<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

# Rivqen fuzz targets

Fuzz targets for the parsers of `rivqen-proto`, the template engine `rivqen-template`, the cache engine `rivqen-cache` and the storage driver `rivqen-store` (DL-148, DL-154, DL-165, DL-182, DL-204). This directory is its own Cargo workspace. It is not part of the root workspace or of a release build.

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
| `patch_content` | The input is the new content of the `price` block of `fixtures/rqp/protocol/pages/page-a.html` (embedded); `render_rqp`, `split_rqp`, then `check_patch_content` on the patched block | The rule gives the same result on a second call; an accepted content has no start tag with a `PATCH_FORBIDDEN` name in HTML context (by the scan) and no attribute name that starts with `on` |
| `cache_control` | `header::parse_cache_control`, `parse_age` and `parse_vary` on two field lines (the first LF of the input splits them), and `parse_cache_offline` on the whole input | Each number is at most 2^31; the order of the lines does not matter; two lines without `"` equal one line joined with a comma; a further `no-store` line sets only `no_store`, a further `max-age=x` line sets only `invalid_freshness` and removes `max_age`; the serialized result parses to the same result; `Age` and `Vary` give the same result for the lines and the joined line; `Vary` names are sorted, unique, lower-case tokens, and a `*` member gives `Any`; `cache-offline` ignores ASCII case and is `False` for any other value |
| `http_date` | `header::parse_http_date` | ASCII case and white space at both ends do not change the result; a date is at most 9999-12-31T23:59:59Z; the IMF-fixdate and the asctime text of a parsed date (from a calendar in the target) parse to the same second |
| `store_file_decode` | `crypto::decode_file` (the plain text of an entry file of `rivqen-store`) with the default limits and with small limits | A decoded value encodes to the same bytes; a value that decodes with the small limits decodes to the same value with the default limits; the content is not larger than the input |
| `store_row_decode` | `crypto::decode_row` (the plain text of the `sealed` column of a row) with the default limits and with small limits | A decoded value encodes to the same bytes; a value that decodes with the small limits decodes to the same value with the default limits; each page field line and each `Vary` name is at most `max_header_bytes`, all lines and all names at most `max_headers_total_bytes` |

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

The script copies files from `fixtures/malformed/seeds/fields`, `spec/rqp/examples` and the `response.body` files of `fixtures/rqp/protocol` into `corpus/<target>/`. For the template targets it copies the pages of `fixtures/legacy/pages`, `fixtures/malformed/legacy/pages`, `fixtures/malformed/seeds/pages` and `fixtures/rqp/protocol/pages`, and `fixtures/rqp/markup/*/input.html`. For `patch_content` it writes the content of every delta operation of the WP-06 fixtures of the ledger and the block contents of the protocol pages (one file for each content). For `cache_control` it copies the `cache-*` and `vary-*` files of `fixtures/malformed/seeds/fields` and writes the examples of RFC 5861 and RFC 9111 and some edge values. For `http_date` it writes the three examples of RFC 9110 section 5.6.7, a leap second, a leap day, the last second of 9999 and `0`. For `store_file_decode` and `store_row_decode` it writes plain texts from the layouts in `crates/rivqen-store/src/crypto/file.rs` and `row.rs`: a legacy entry, an RQP entry (for the row: with a snapshot of two blocks), an empty value and a value of empty items. It only reads the fixtures. You can run it again.

## Run

Run the seed corpus once:

```sh
cd fuzz
for t in sf_fields json_reader manifest envelope split_v1 render_v1 split_rqp patch_content cache_control http_date store_file_decode store_row_decode; do cargo fuzz run $t -- -runs=0; done
```

Fuzz one target for 60 seconds. Run one target at a time:

```sh
cd fuzz
cargo fuzz run envelope -- -max_total_time=60
```

If a target crashes, libFuzzer writes the input to `artifacts/<target>/`. Do not fix the input. Keep the file, open a finding, and add a regression test to the parser crate.
