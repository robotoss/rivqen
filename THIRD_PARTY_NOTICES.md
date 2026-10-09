# Third-party notices

This file lists third-party material that is **distributed** with Rivqen, and the upstream project that Rivqen studies for protocol compatibility.

## 1. Distributed third-party material

The repository currently contains documentation, documentation tooling, an example server and a test lab.

| Component | Use | License | Distributed in |
|---|---|---|---|
| [VitePress](https://github.com/vuejs/vitepress) | Static site generator (build-time) | MIT | Generated documentation site (runtime JS) |
| [Mermaid](https://github.com/mermaid-js/mermaid) | Diagram rendering (runtime in the site) | MIT | Generated documentation site |
| [vitepress-plugin-mermaid](https://github.com/emersonbottero/vitepress-plugin-mermaid) | Mermaid integration (build-time) | MIT | Generated documentation site |
| [parse5](https://github.com/inikulin/parse5) 8.0.1 | HTML tokenizer in `examples/server-demo` (installed by npm, not committed) | MIT | Example server only |
| [entities](https://github.com/fb55/entities) | Transitive dependency of parse5 | BSD-2-Clause | Example server only |
| [Contributor Covenant 2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/) | Text of `CODE_OF_CONDUCT.md` | CC BY 4.0 | Repository |
| [Developer Certificate of Origin 1.1](https://developercertificate.org/) | Text quoted in `CONTRIBUTING.md` | Verbatim copying permitted | Repository |

Transitive npm dependencies are listed in `package-lock.json` and `examples/server-demo/package-lock.json`. The test lab in `tools/upstream-lab` runs a local VasSonic checkout that you supply, and downloads its npm and Maven dependencies into an ignored work folder at run time. None of the upstream code is committed. A full SBOM will be generated in CI (see `docs/engineering/delivery/ci-cd.md`).

## 2. Upstream project studied for compatibility (not distributed)

| Project | Pinned commit | License | Relation to Rivqen |
|---|---|---|---|
| [Tencent/VasSonic](https://github.com/Tencent/VasSonic) | `59936beff656d4b5718ff6444d6c5e001a2c5231` (2019-04-15) | BSD 3-Clause; includes Android Source Code 4.4_r1 under Apache-2.0 | Behavior studied for the legacy protocol compatibility mode under a clean-room policy. **No code is copied.** |

Original upstream notice (for attribution):

> Tencent is pleased to support the open source community by making VasSonic available.
> Copyright (C) 2017 THL A29 Limited, a Tencent company. All rights reserved.
> VasSonic is licensed under the BSD 3-Clause License, except for the third-party components listed in its LICENSE file.

Rivqen does not use the VasSonic name, logo, or visual identity as its own brand.
