# RQP machine-readable specification

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The normative text of `rqp/1` is in `docs/engineering/protocol/` ([markup](../../docs/engineering/protocol/markup.md), [negotiation](../../docs/engineering/protocol/negotiation.md), [manifest and patch](../../docs/engineering/protocol/manifest-patch.md)). This directory holds the machine-readable parts.

| Path | Content |
|---|---|
| [`schemas/`](schemas/README.md) | JSON Schemas of the manifest and the delta envelope |
| [`examples/`](examples/README.md) | Valid and invalid examples and the known-answer tests (`kat/`) |

Tests: `cd tools/rqp-schema && npm ci --ignore-scripts && npm test` ([`tools/rqp-schema/README.md`](../../tools/rqp-schema/README.md)).
