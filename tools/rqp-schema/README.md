# rqp-schema — tests of the rqp/1 JSON Schemas

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

This package checks the JSON Schemas of the Rivqen protocol in `spec/rqp/schemas/` and the example instances in `spec/rqp/examples/`. Normative text: `docs/engineering/protocol/manifest-patch.md`.

## What the tests check

| File | Checks |
|---|---|
| `test/schemas.test.mjs` | Each schema is valid against the Draft 2020-12 meta-schema, compiles in Ajv strict mode, and has its URN `$id`. The definitions that both schemas carry (`block_id`, `format`, `sha256`, `template_revision`, `page_revision`) are identical. |
| `test/examples.test.mjs` | Every file in `examples/valid/<kind>/` passes. Every file in `examples/invalid/<kind>/` fails with the expected keyword at the expected instance path, and has no second defect. Boundary values (256/257 blocks and operations, content length, canonical base64url last character) are generated in the test. |
| `test/kat.test.mjs` | Known-answer test: `examples/kat/page.html`, the manifest and the patches agree with the revision algorithm (section 1.3 of the spec page), with the client verification of `next_revision` (section 2.4), and with the Node.js markup candidate `tools/rqp-markup/node`. |

## Requirements

- Node.js 22 or later.
- One dependency: `ajv` 8.20.0 (MIT), pinned in `package-lock.json`. No `format` keyword is used, so `ajv-formats` is not needed.

## Procedure

1. Go to `tools/rqp-schema`.
2. Install the locked dependencies without install scripts:

   ```sh
   npm ci --ignore-scripts
   ```

3. Run the tests:

   ```sh
   npm test
   ```

## Add an example

1. Put a valid instance in `spec/rqp/examples/valid/manifest/` or `spec/rqp/examples/valid/patch/`.
2. Put an invalid instance in `spec/rqp/examples/invalid/<kind>/<name>.json` with this form:

   ```json
   {
     "description": "One sentence: the defect",
     "expect": { "keyword": "pattern", "instancePath": "/blocks/0/id" },
     "instance": { }
   }
   ```

3. Give each invalid instance exactly one defect.
4. Run `npm test`.
