# RQP golden fixtures

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Exact-byte fixtures for `rqp/1` (`.gitattributes` turns off line-end conversion). Do not edit them by hand: regenerate them with the generators below. The catalog is [`docs/engineering/protocol/fixtures.md`](../../docs/engineering/protocol/fixtures.md).

| Path | Content | Generator and check |
|---|---|---|
| [`markup/`](markup/README.md) | 362 markup fixtures (148 valid, 214 invalid) | `python3 tools/rqp-markup/gen.py`; `node tools/rqp-markup/diff.mjs` |
| [`protocol/`](protocol/README.md) | 265 protocol fixtures (112 server `FX-RQ-P*`, 153 client `FX-RQ-C*`) | `node tools/rqp-protocol/gen.mjs --check`; `node tools/rqp-protocol/check.mjs` |
