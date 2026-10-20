# rqp/1 schema examples

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

| Directory | Content |
|---|---|
| `valid/manifest/`, `valid/patch/` | Instances that pass the schema (the forms of H-103 and H-104). Only the instances listed in `kat/kat.json` are also consistent with a document; the others have revisions that do not belong to a real page. `valid/manifest/defaults.json` has `"policy": {}` and `"limits": {}`: the schema and a client accept them, but a server does not write them (H-103). |
| `invalid/manifest/`, `invalid/patch/` | `{ "description", "expect": { "keyword", "instancePath" }, "instance" }`. Each instance has exactly one defect. `keyword` and `instancePath` are the JSON Schema keyword and the JSON Pointer of the error. |
| `kat/` | Known-answer set 1: `page.html` (exact bytes; the example of the markup page), `full.html` (one valid full document: `page.html` with the compact manifest element immediately after the head start tag `<head>`, H-72; written by the reference server step `tools/rqp-protocol/src/server.mjs`, and `tools/rqp-protocol/test/kat.test.mjs` and the server demo test compare their output with it), the manifest `valid/manifest/shop.json` (no `policy`, no `limits`), the patches `valid/patch/shop-price.json` and `valid/patch/shop-all.json` (`next_revision` and the operations, H-104), in `kat.json` the values that the client computes with its markup analysis (`computed`: the template revision and the block list with id, format and hash; the manifest has neither, H-103) and the `Rq-Have` field value of the page (`rq_have`). Known-answer set 2 (`kat.json` member `with_policy_limits`, H-55): the same page with `policy` and `limits` in the manifest `valid/manifest/shop-policy.json` (only members that are not the default, H-103), the full document `full-policy.html`, the patch `valid/patch/shop-policy-price.json`, the policy line of the page-revision preimage (`policy_line`) and the page revision after the patch (`page_revision_b`). |

These files are not the protocol fixtures (`fixtures/rqp/protocol`, request → response). Do not edit `kat/page.html`, `kat/full.html` or `kat/full-policy.html` with an editor that changes line ends or adds a final line feed.
