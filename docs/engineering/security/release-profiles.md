# Release profiles

A release profile is a documented set of build and runtime settings. Rivqen defines two profiles that reduce unnecessary metadata and harden release builds with **standard, documented toolchain features**. The host app owner chooses the profile.

**Status:** <Badge type="info" text="DESIGN" /> Requirements `VISIBILITY-001`, `RESILIENCE-001`, `BUILD-ID-001`, `PATCH-001`. Decision record: [ADR-012](/engineering/architecture/adr/#adr-012).

[[toc]]

## 1. Priorities

1. **No exploitable vulnerability, and fast delivery of fixes.**
2. Protection of secrets, user data and integrity.
3. Less unnecessary information in release artifacts.

A lower priority never overrides a higher one. Hiding a version string never replaces a patch.

## 2. What the profiles do and do not do

| Goal | Allowed approach | Not a goal |
|---|---|---|
| Neutral integration | No SDK banner in UI or logs; no SDK-identifying HTTP header by default | Making the SDK undetectable |
| Less telemetry | Telemetry off by default; local diagnostics only | Hiding SDK behavior from the user, the app owner or auditors |
| Smaller binaries | R8 in the app release build; linker dead-code stripping; Rust LTO/strip after measurement | Padding, junk data, or size changes to avoid recognition |
| Fewer internals exposed | Minimal exported FFI symbols; no debug endpoints in release; private symbol files | Obscuring code to avoid security review |
| Private data protection | AEAD for private cache; platform key stores | Encrypting everything to mask behavior |
| Release integrity | SHA-256 digests, signatures, SBOM, SLSA provenance, reproducible builds where possible | Changing hashes or signatures to avoid recognition |
| Owner transparency | Local `diagnostics()` with exact versions; versions in SBOM and lockfiles | Hiding the version from the people responsible for updates |

::: danger Out of scope for Rivqen
Rivqen does not implement anti-analysis, environment-dependent behavior, evasion of store review or security scanners, polymorphic binaries, hidden code loading, or concealed data collection. These practices break trust in an open-source SDK, make vulnerable versions hard to find, and can violate [Google Play](https://support.google.com/googleplay/android-developer/answer/13323374) and [Apple App Review](https://developer.apple.com/app-store/review/guidelines/) policies.
:::

## 3. Profile `minimal-exposure`

```yaml
# Conceptual policy — each tool gets its own native configuration
integration_profile: minimal-exposure
telemetry: disabled_by_default
network_sdk_identity_header: disabled_by_default
user_facing_sdk_branding: none
release_minification: enabled_by_host_application
release_debug_symbols: stored_separately_for_crash_analysis
sensitive_cache_encryption: enabled
security_inventory: required
sdk_diagnostics: available_to_host_application
app_review_disclosure: required_where_applicable
```

**Invariants.** The profile does not change the trust model, features, dependencies, permissions, signatures, or behavior based on the environment. Versions and provenance stay available to the app owner and auditors.

## 4. Profile `hardened-release`

`hardened-release` inherits `minimal-exposure` and adds:

| Area | Setting | Check |
|---|---|---|
| Android | Host-controlled R8; narrow, tested SDK keep rules; native symbols stored separately | Golden E2E tests pass after R8; crash reports symbolicate |
| iOS | Release optimizations and dead-code stripping; dSYM in a secure symbol store | Archive + symbolication test |
| Rust core | Minimal, versioned exported ABI; LTO/strip only after size/speed/crash-report tests; `unsafe` review | ABI manifest test; size and latency report |
| Web | Production minification; deliberate source-map policy (private or public) | Package scan for source maps and secrets |
| Supply chain | SBOM, signed provenance, genuine digests, third-party notices | Inventory from source to APK/IPA |
| Security reviews | Static and dynamic analysis; mobile pentest against OWASP MASVS (incl. MASVS-RESILIENCE and MASVS-CODE) | Report with findings and fixes |

The YAML is a concept, not a Gradle, Cargo or Xcode format. CI checks real outputs, not flags.

## 5. Version and build identity (`BUILD-ID-001`)

| Item | Who can learn it | Rivqen rule |
|---|---|---|
| SHA-256 of an APK/IPA | Anyone with the file | Genuine digest and signature. Never hidden or faked. |
| SDK version string in the app | Only if included | Not added as an unnecessary public marker; always available to the owner via `diagnostics()` and SBOM |
| Protocol version and capabilities | Visible on the network | Always accurate. Required for safe compatibility. |
| Likely origin of code from binary features | Often possible for open source | Not promised to be hidden |

The maintainers keep a secure record: `host app release → SDK modules → git revision → dependency versions → artifact digests → security status`. With it, an app owner can find vulnerable builds **without** access to user devices.

## 6. Acceptance checklist

- [ ] Android release build: R8 enabled, not disabled by broad SDK keep rules; golden E2E tests pass.
- [ ] iOS release build: symbols and exports checked; dSYM symbolicates a crash.
- [ ] Rust: no test functions or debug endpoints exported.
- [ ] No embedded secrets in Rust, Android, iOS or Web artifacts.
- [ ] No automatic network transmission of SDK name or exact version; protocol capabilities stay accurate.
- [ ] legacy and RQP conformance passes after hardening.
- [ ] Each build is traceable to commit, dependencies, digest and signature.
- [ ] Hardening cost measured: size, startup, memory, p95 render, crash rate.
- [ ] Legitimate users and auditors are not blocked by any setting.

## Related

- [Vulnerability management](/engineering/security/vulnerability-management)
- [Release and packaging](/engineering/delivery/release)
- [OWASP MASVS-RESILIENCE](https://mas.owasp.org/MASVS/11-MASVS-RESILIENCE/)
