# Release and packaging

This page defines what Rivqen ships, where, and in which release stages.

**Status:** <Badge type="info" text="DESIGN" />

## 1. Artifacts

| Component | Format | Registry |
|---|---|---|
| Android SDK | AAR with native libraries (`arm64-v8a`, `armeabi-v7a`, `x86_64`) | Maven Central |
| iOS SDK | Swift package + XCFramework (device + simulator) | GitHub releases + Swift Package Index |
| Web core, React, SSR, Next adapter | ES modules + type definitions | npm |
| Node.js server SDK | ES modules + type definitions | npm |
| Java server SDK | JAR + BOM + Spring Boot starter | Maven Central |
| PHP server SDK | Composer package | Packagist |
| Rust crates | Source crates | crates.io — only when the API is intentionally stable |
| Reference server | Container image | GitHub Container Registry |

Every artifact has: a version, a SHA-256 digest, a signature, an SBOM, provenance, and license notices.

## 2. Versioning

1. SemVer for all public packages.
2. Mobile SDKs and web/server SDKs share the **minor** version when they share protocol capabilities.
3. Protocol versions are separate from package versions: Legacy protocol (frozen), Rivqen protocol.x.
4. The changelog follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## 3. Release stages

| Stage | Content | For whom |
|---|---|---|
| **Alpha: Compatibility** | Rust core with legacy, Android and iOS demos, reference server | Research, closed apps |
| **Beta: Ecosystem** | Node/Java/PHP servers, React SDK, cache hardening, security suite | Controlled pilots |
| **Stable 1.0** | Confirmed parity + safe platform fallbacks | Production on own sites |
| **Experimental RQP** | HTTP/3 tuning, WebSocket, WebTransport PoC | Opt-in; never changes the stable contract |

## 4. Release checklist

1. All gates for the stage are closed (see [Gates](/engineering/plan/gates)).
2. `LEGAL-001` is complete for the first public release.
3. SBOM, provenance and signatures are generated and verified.
4. Release notes list: changes, protocol capability changes, security fixes, migration steps.
5. Documentation is updated for every changed surface.
6. The release owner signs off.

## Related

- [Release profiles](/engineering/security/release-profiles)
- [Licensing](/legal/licensing)
