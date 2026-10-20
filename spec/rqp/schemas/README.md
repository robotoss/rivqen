# rqp/1 JSON Schemas

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

| File | `$id` | Document |
|---|---|---|
| `manifest.schema.json` | `urn:rivqen:rqp:1:manifest` | Page manifest (inline `<script type="application/rivqen-manifest+json">`) |
| `patch.schema.json` | `urn:rivqen:rqp:1:patch` | Patch envelope (`application/vnd.rivqen.delta+json`) |

Normative text: `docs/engineering/protocol/manifest-patch.md` (published at `/engineering/protocol/manifest-patch`). When the page and a schema disagree, the page is right and the schema has a defect.

- JSON Schema Draft 2020-12. Both schemas are self-contained (no external `$ref`), so a validator needs no schema registry.
- No `format` keyword: every string rule is a `pattern` (ECMA-262 and Rust `regex` compatible: no look-around, no back-references).
- A schema check is necessary, not sufficient. The spec page lists the rules that a schema cannot express (the page revision check of the manifest, known and unique block ids of a patch, duplicate member names, byte sizes, revision recomputation, content rules).
- Since H-103 the manifest has no `template_revision` and no `blocks`; since H-104 the envelope is `next_revision` and the operations (`block_id`, `content`). Both schemas reject every other member, also the members of the forms before these decisions.

Examples: `spec/rqp/examples/` (valid, invalid with the expected error, and a known-answer set in `kat/`). Tests: `tools/rqp-schema/`.
