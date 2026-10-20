# RQP byte study (WP-17 S6 T-01)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

This study splits the bytes of `rqp/1` into components and measures candidate wire changes that send fewer bytes. Input: human decision H-101 ([sprint record WP-17 S6](../../../../docs/engineering/plan/sprints/WP-17-S6.md)). The report is `evidence/rqp-bytes/report.md`. Nothing here is normative: every option is a measurement model, and the human decides the wire format.

## Contents

| Path | Content |
|---|---|
| `run.mjs` | CLI: compute the results, run the legacy lab visits, check the results, print the report tables |
| `src/pages.mjs` | The demo catalog page of `examples/server-demo` at a data step; synthetic pages with 1, 4, 16 or 64 blocks of 20 or 2048 bytes; the conversion to legacy markers and to a page without block markup |
| `src/rqp.mjs` | The baseline: F, `304` and D of the reference server step `tools/rqp-protocol/src/server.mjs` |
| `src/json-segments.mjs` | A JSON serializer that writes the bytes of `JSON.stringify` and labels each byte (raw breakdown) |
| `src/breakdown.mjs` | Raw rows (exact, additive) and marginal gzip and Brotli costs (by ablation) of the manifest and of the delta envelope |
| `src/options.mjs` | Candidate wire changes (`M*` manifest, `D*` delta, `Q*` request fields) |
| `src/cbor.mjs` | A minimal CBOR encoder for the binary envelope options (measurement only) |
| `src/hpack.mjs` | HPACK header block sizes of Node.js HTTP/2 on 127.0.0.1 |
| `src/legacy.mjs` | Client for the upstream lab server (`lab-node`) and the breakdown of a legacy data response |
| `src/study.mjs`, `src/tables.mjs` | The results object and the Markdown tables of the report |
| `results/bytes.json` | Results (Node.js 26.8.2, zlib 1.2.12, Brotli 1.2.0, nghttp2 1.70.0; see `environment`) |
| `results/legacy-lab.json` | Observations of the upstream Node.js server (VasSonic `59936bef`, `sonic_differ` 1.0.7) on 2026-10-20 |
| `test/` | `node --test` tests |

## Method

- **Pinned reference.** The study is pinned to the pre-P1 reference (commit `276a1c3`, frozen copies in `baseline/`), not to the shipped reference (DL-047).
- **Sizes.** Raw bytes, and gzip and Brotli with the Node.js zlib defaults on the body. This is the method of `examples/server-demo/scripts/measure.js`, so the demo numbers are the numbers of `evidence/benchmarks/rqp-bytes-2026-10-19.json` (a test checks this).
- **Raw breakdown.** Each byte of the manifest element (or of D) is in exactly one row. The rows add up to the total.
- **Marginal cost.** For gzip and Brotli, the cost of a component is the size of the whole body minus the size of the body without the component. Marginal costs overlap, so they do not add up. Brotli marginal costs of components under about 50 raw bytes are not stable (they can be larger than the raw bytes or negative). Use the gzip column to rank small components.
- **Header fields.** Header fields are not content-coded. The study gives the HTTP/1.1 bytes (`name: value` and CRLF) and the HPACK header block bytes that Node.js (nghttp2) writes on a new HTTP/2 connection.
- **Legacy.** The same pages with legacy markers go through the unmodified upstream Node.js server in a container. The title element has no markers, because the legacy servers take the first title element as the data block `title` ([legacy markers](../../../../docs/engineering/protocol/legacy-markers.md) section 2).

## Requirements

- Node.js 22 or later. No dependency.
- For the legacy lab only: Docker, network access to GitHub and to the npm registry.

## Procedure: run the study

1. Go to the repository root.
2. Run the tests:

   ```sh
   node --test "tools/rqp-protocol/study/bytes/test/*.test.mjs"
   ```

3. Write the results:

   ```sh
   node tools/rqp-protocol/study/bytes/run.mjs --out tools/rqp-protocol/study/bytes/results/bytes.json
   ```

   The run reads `results/legacy-lab.json` when it exists.

4. Print the tables of the report:

   ```sh
   node tools/rqp-protocol/study/bytes/run.mjs --tables tools/rqp-protocol/study/bytes/results/bytes.json
   ```

To compare the committed raw numbers with a new run, use `--check tools/rqp-protocol/study/bytes/results/bytes.json`. The check ignores gzip, Brotli and HPACK, because they can change with the zlib and nghttp2 versions.

## Procedure: run the legacy lab visits

Do these steps outside the repository. Never copy upstream code into the repository ([provenance](../../../../docs/legal/provenance.md)).

1. Set `W` to an empty work directory outside the repository.
2. Clone the upstream at the pinned commit:

   ```sh
   git clone https://github.com/Tencent/VasSonic.git "$W/vassonic"
   git -C "$W/vassonic" checkout 59936beff656d4b5718ff6444d6c5e001a2c5231
   ```

3. Install the harness dependencies with install scripts disabled:

   ```sh
   mkdir -p "$W/node" "$W/pages"
   cp tools/upstream-lab/node/app.js "$W/node/"
   (cd "$W/node" && echo '{"private":true}' > package.json && \
     npm install --no-fund --no-audit --ignore-scripts koa@2 through2@2 koa-is-json@1 sonic_differ@1.0.7)
   ```

4. Start only `lab-node`, bound to 127.0.0.1:

   ```sh
   docker run -d --name lab-node -p 127.0.0.1:18081:8080 -v "$W/vassonic:/upstream:ro" \
     -v "$W/pages:/pages:ro" -v "$W/node:/harness:ro" -e NODE_PATH=/harness/node_modules \
     -w /harness node:22-alpine node app.js
   ```

5. Run the visits. The command writes each page into `$W/pages` and deletes it after its visits:

   ```sh
   node tools/rqp-protocol/study/bytes/run.mjs --lab http://127.0.0.1:18081 --pages "$W/pages" \
     --lab-out tools/rqp-protocol/study/bytes/results/legacy-lab.json
   ```

6. Stop the container and delete the work directory:

   ```sh
   docker rm -f lab-node
   rm -rf "$W"
   ```

7. Do the procedure "run the study" again, from step 3.
