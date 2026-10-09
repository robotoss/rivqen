# Third-party dependencies

This page defines which third-party licenses Rivqen accepts, how they are checked, and how notices are delivered.

**Status:** <Badge type="info" text="DESIGN" /> policy; enforcement starts with WP-20.

## 1. License classes

| Class | Licenses | Rule |
|---|---|---|
| **Allowed** | MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, 0BSD, CC0-1.0, Unicode-3.0, NCSA, PSF-2.0 (tools only) | Use freely; keep notices |
| **Allowed by exception** | MPL-2.0 (file-level copyleft), EPL-2.0 (build tools only) | Maintainer approval; list in this page |
| **Not allowed** | GPL-2.0/3.0, LGPL (in distributed artifacts), AGPL, SSPL, BUSL, Commons Clause, "non-commercial" | Never in distributed artifacts |

## 2. Approved exceptions

| Component | License | Why | Conditions |
|---|---|---|---|
| UniFFI (`uniffi*` crates) | MPL-2.0 | Standard Rust → Kotlin/Swift bindings tool | No modification of UniFFI files without publishing them; check generated-code status in WP-09 |
| JUnit Jupiter (`org.junit:junit-bom` 5.14.4) | EPL-2.0 | Standard Java test framework; test scope only, never in a distributed artifact | Test dependency only. Approved by the human on 2026-10-09 ([WP-17 S1](/engineering/plan/sprints/WP-17-S1) H-19) |

## 3. Per-ecosystem checks

| Ecosystem | Tool |
|---|---|
| Rust | `cargo-deny` (licenses, bans, advisories, sources) |
| Android/Kotlin | Gradle license report plugin + dependency verification |
| iOS/Swift | Swift Package dependencies reviewed manually (SDK uses system frameworks only) |
| npm | `license-checker`-style CI step on the production dependency tree |
| Java | Maven/Gradle license plugin |
| PHP | `composer licenses` |
| All | SBOM (CycloneDX or SPDX) per artifact; REUSE lint for own files |

## 4. Notices in artifacts

1. Each artifact includes `LICENSE`, `NOTICE` and a generated third-party notice file for the dependencies it contains.
2. Android AAR: notices in the AAR; apps can merge them with the OSS licenses plugin.
3. iOS: notices in the XCFramework bundle.
4. npm / Maven / Composer: notices in the package root.

## 5. Documentation site dependencies

| Component | License |
|---|---|
| VitePress 1.6.4 | MIT |
| Mermaid 11.17.2 | MIT |
| vitepress-plugin-mermaid 2.0.17 | MIT |
