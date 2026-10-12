# rqp/1 schema examples

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

| Directory | Content |
|---|---|
| `valid/manifest/`, `valid/patch/` | Instances that pass the schema. Only the instances listed in `kat/kat.json` are also consistent with a document; the others have revisions that do not belong to a real page. |
| `invalid/manifest/`, `invalid/patch/` | `{ "description", "expect": { "keyword", "instancePath" }, "instance" }`. Each instance has exactly one defect. `keyword` and `instancePath` are the JSON Schema keyword and the JSON Pointer of the error. |
| `kat/` | Known-answer set: `page.html` (exact bytes; the example of the markup page), the manifest `valid/manifest/shop.json`, and the patches `valid/patch/shop-price.json` and `valid/patch/shop-all.json`. |

These files are not the protocol fixtures (`fixtures/rqp/protocol`, request → response). Do not edit `kat/page.html` with an editor that changes line ends or adds a final line feed.
