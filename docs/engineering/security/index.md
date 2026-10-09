# Security overview

Rivqen runs inside other companies' apps and handles their users' pages. Security is a product requirement, not a later phase. This section gives the threat model, the controls, and the evidence that proves each control.

**Status:** <Badge type="info" text="DESIGN" /> All controls are **requirements**. None is implemented yet.

## Principles

1. **Speed never beats safety.** No optimization can disable TLS, CSP, origin checks, cookie rules or account isolation (`INV-03`).
2. **Network data is untrusted** on every transport (`INV-16`).
3. **Evidence, not claims.** "Improved security" needs a test and a source. Rust is not automatically safer than the code it replaces.
4. **Fix, do not hide.** A vulnerability is closed by a patch or a documented mitigation (`INV-19`).
5. **Transparent behavior.** Rivqen behaves the same in normal use, in security testing and in app-store review.

## Pages

| Page | Content |
|---|---|
| [Threat model](/engineering/security/threat-model) | Assets, attackers, trust boundaries, threats |
| [Security controls](/engineering/security/controls) | `SEC-01` … `SEC-26` with checks |
| [Platform hardening](/engineering/security/platform-hardening) | Android, iOS, Web, server measures |
| [Release profiles](/engineering/security/release-profiles) | `minimal-exposure` and `hardened-release` build profiles |
| [Cryptography](/engineering/security/crypto) | Cache encryption design and key handling |
| [Vulnerability management](/engineering/security/vulnerability-management) | Reporting, triage, advisories, patch policy |

## Evidence artifacts

<Badge type="danger" text="GATE" /> Gate `SECURITY-001` before public beta: no known unresolved critical or high issues in mandatory scenarios; every SDK and dependency version is traceable in SBOM and provenance; every exception is documented and approved by the security owner.

| Artifact | Accepted when |
|---|---|
| Threat model | Trust boundaries and attack classes are listed; each mitigation has an owner |
| Legacy gap analysis | Every "improved over upstream" claim links to upstream code and a test result |
| Control register | `SEC-01..26` each have status `planned` / `implemented` / `tested`, a date and test evidence |
| Platform matrix | Android/iOS capabilities listed; forbidden bypasses confirmed absent; safe fallback tested |
| Crypto design | ADR-011 accepted; key rotation, logout and recovery tested |
| Release audit | SBOM, signatures, permission list, SAST/DAST/fuzz/pentest results |
