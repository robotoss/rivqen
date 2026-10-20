# rqp-schema — tests of the rqp/1 JSON Schemas

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

This package checks the JSON Schemas of the Rivqen protocol in `spec/rqp/schemas/` and the example instances in `spec/rqp/examples/`. Normative text: `docs/engineering/protocol/manifest-patch.md`.

## What the tests check

| File | Checks |
|---|---|
| `test/schemas.test.mjs` | Each schema is valid against the Draft 2020-12 meta-schema, compiles in Ajv strict mode, and has its URN `$id`. The page revision of the manifest and the binding of the envelope (H-107) have the same canonical base64url tail. The members of the manifest (H-103) and of the envelope and its operations (H-104, H-107) are exactly the ones of the spec page. |
| `test/examples.test.mjs` | Every file in `examples/valid/<kind>/` passes. Every file in `examples/invalid/<kind>/` fails with the expected keyword at the expected instance path, and has no second defect. Boundary values (256/257 operations, content length, canonical base64url last character of a revision and of a binding, the `d1.` prefix of a binding) and the members of the documents before H-103, H-104 and H-107 (each an unknown member, also `next_revision`) are generated in the test. |
| `test/revision.test.mjs` | The policy line of the page-revision preimage (section 1.3, H-55) on 112 combinations of present and missing `policy` and `limits` members: every combination is schema-valid and has its own preimage; the line is absent only without both members; member order in the JSON text does not change it. |
| `test/kat.test.mjs` | Known-answer test, two sets (without and with `policy` and `limits`): `examples/kat/page.html`, the computed values of `kat.json`, the manifests and the patches agree with the revision algorithm (section 1.3 of the spec page), with the client verification of the binding (section 2.4, step 7; H-104, H-107, H-108: a delta for another template, another base, its own target, another URL or another nonce, or with content that the binding does not bind, fails it), and with the Node.js markup candidate `tools/rqp-markup/node`. The `kat.json` bindings: the exact preimage written out by hand (157 bytes for the price patch, with the nonce N of H-108), each entry against its patch file, the binding with a query (`binding_with_query`), and the KAT nonce (`nonce`: the bytes 00 to 0F, its text N and its Byte Sequence). The client checks of H-48 and H-103: the page revision that the markup analysis of the original document gives (section 1.8) and the analysis of the patched document (section 2.4, step 8), with patches that have a correct binding but bad content. A manifest that adds or removes `policy` or `limits` and keeps the old page revision fails the client check (H-55), and a patch fails the binding check against a snapshot with other `policy` or `limits`. The full documents `kat/full.html` and `kat/full-policy.html` (manifest position, no `<` in the manifest text, M-13) and the `Rq-Have` example value (with the member `n`, H-108). |

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
     "expect": { "keyword": "pattern", "instancePath": "/operations/0/block_id" },
     "instance": { }
   }
   ```

3. Give each invalid instance exactly one defect.
4. Run `npm test`.
