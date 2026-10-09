# Legal overview

This section defines how Rivqen handles licenses, third-party code, upstream provenance and its name. It is an engineering policy, not legal advice.

::: warning Not legal advice
These pages record engineering decisions and public facts. A qualified IP / open-source lawyer reviews them before the first public release (gate `LEGAL-001`).
:::

## Decisions

| Topic | Decision | Page |
|---|---|---|
| Project license | **Apache License 2.0** for all Rivqen code and documentation | [Licensing](/legal/licensing) |
| Third-party code | Allowlist of permissive licenses; copyleft only by exception | [Third-party dependencies](/legal/third-party) |
| Upstream VasSonic | **Clean-room**: behavior studied, no code copied | [Clean-room provenance](/legal/provenance) |
| Attribution | VasSonic named only for compatibility and attribution | [Upstream attribution](/legal/upstream-attribution) |
| Name | "Rivqen" is the working public name; clearance before packages are published | [Name and brand](/legal/brand) |
| Contributions | Developer Certificate of Origin 1.1 (`git commit -s`) | [CONTRIBUTING.md](https://github.com/robotoss/rivqen/blob/main/CONTRIBUTING.md) |

## Gate `LEGAL-001`

Before the first public alpha, and before each release with new dependencies:

1. Upstream commit pinned; inventory of studied files complete. ✅ (pinned at `59936bef`)
2. SBOM and full direct/transitive dependency map for Rust, Kotlin, Swift, JS, Java, PHP.
3. All required third-party notices included in each distribution.
4. Human and similarity review of AI-generated code.
5. License of own modules, contributor policy and dependency policy agreed. ✅ (Apache-2.0, DCO)
6. Name and visual identity cleared; result documented (`BRAND-001`).
7. No claims of "official", "successor", "by Tencent"; no foreign logos.
8. "Sonic" used only to identify the compatible protocol.
9. Patent review in commercial markets, if needed.
10. Results signed off by the release owner; disputed points reviewed by a lawyer.
