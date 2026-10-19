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
| `caniuse-lite` 1.0.30001815 (`tools/rqp-markup/node`), 1.0.30001816 (`tools/rqp-protocol`) (npm, via `@stryker-mutator/core`) | CC-BY-4.0 | Browser data used by the mutation test tool of `tools/rqp-markup/node` and `tools/rqp-protocol` | Dev only; never in a distributed artifact. Approved by the human on 2026-10-11 ([WP-17 S1](/engineering/plan/sprints/WP-17-S1) H-29) |
| `minimatch` 10.2.6 (npm, via `@stryker-mutator/core`) | BlueOak-1.0.0 | Glob matching in the mutation test tool of `tools/rqp-markup/node` and `tools/rqp-protocol` | Dev only; never in a distributed artifact. Approved by the human on 2026-10-11 (H-29) |
| `robust-predicates` 3.0.3 (npm, via `mermaid`) | Unlicense | Geometry predicates in the diagram renderer of the documentation site | Documentation site only; never in an SDK artifact. Approved by the human on 2026-10-11 ([WP-17 S1](/engineering/plan/sprints/WP-17-S1) H-33) |
| PCOV (PHP extension `pcov` 1.0.12) | PHP-3.01 | Code coverage driver for PHPUnit 12 and Infection; PHPUnit 12 (`php-code-coverage`) supports only PCOV and Xdebug | Developer and CI machines only (installed with `pecl`); never a Composer dependency and never in a distributed artifact. Approved by the human on 2026-10-11 ([WP-17 S1](/engineering/plan/sprints/WP-17-S1) H-24) |

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

### WebView lab (`tools/webview-lab/app`, WP-17 S5)

<Badge type="tip" text="FACT" /> The lab is a test tool; nothing in it ships (human decision H-93). Its Kotlin Multiplatform project comes from the IDE template and uses only Apache-2.0 components (checked 2026-10-19 in `gradle/libs.versions.toml`): Kotlin 2.4.20, Compose Multiplatform 1.12.1 (with Material 3 1.12.0-alpha03), Android Gradle Plugin 9.1.1, AndroidX `activity` 1.13.0, `core` 1.19.1, `appcompat` 1.8.0, `lifecycle` 2.11.0, and the Gradle wrapper. The template's JUnit 4 (EPL-1.0, not in the allowed classes) and the Android host and device test set-up were removed; the iOS app uses system frameworks only. S5 T-04 added to the Android app `androidx.webkit:webkit` 1.17.1 (with `androidx.annotation:annotation-experimental`, `androidx.core`, `org.jspecify:jspecify` 1.0.0) and `org.jetbrains.kotlinx:kotlinx-coroutines-android` 1.11.0, all Apache-2.0; the Android app no longer uses `:shared` or Compose. S5 T-03 made the iOS app pure Swift with a local Swift package (`LabCore`, no external dependency); it no longer links the Kotlin framework.

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
