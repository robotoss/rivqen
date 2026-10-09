# Standards

This page lists the standards that Rivqen relies on, with their status on 9 October 2026.

[[toc]]

## HTTP and transport

| Standard | Title | Status | Use in Rivqen |
|---|---|---|---|
| [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110) | HTTP Semantics | Internet Standard | Conditional requests, `ETag`, `304` |
| [RFC 9111](https://www.rfc-editor.org/rfc/rfc9111) | HTTP Caching | Internet Standard | Cache policy |
| [RFC 9112](https://www.rfc-editor.org/rfc/rfc9112) / [9113](https://www.rfc-editor.org/rfc/rfc9113) / [9114](https://www.rfc-editor.org/rfc/rfc9114) | HTTP/1.1, HTTP/2, HTTP/3 | Standards | Transport through platform stacks |
| [RFC 9000](https://www.rfc-editor.org/rfc/rfc9000) | QUIC | Proposed Standard | Under HTTP/3 |
| [RFC 5861](https://www.rfc-editor.org/rfc/rfc5861) | `stale-while-revalidate`, `stale-if-error` | Informational | Stale policy |
| [RFC 8297](https://www.rfc-editor.org/rfc/rfc8297) | 103 Early Hints | Experimental | Subresource hints |
| [RFC 9218](https://www.rfc-editor.org/rfc/rfc9218) | Extensible Prioritization Scheme for HTTP | Proposed Standard | Prefetch priority |
| [RFC 9651](https://www.rfc-editor.org/rfc/rfc9651) | Structured Field Values for HTTP | Proposed Standard | RQP header syntax |
| [RFC 9842](https://www.rfc-editor.org/rfc/rfc9842) | Compression Dictionary Transport (Sept 2025) | Proposed Standard | RQP evaluation (ADR-013) |
| [RFC 3229](https://www.rfc-editor.org/rfc/rfc3229) | Delta encoding in HTTP | Proposed Standard | Not used (reference) |
| [RFC 6455](https://www.rfc-editor.org/rfc/rfc6455) / [8441](https://www.rfc-editor.org/rfc/rfc8441) / [9220](https://www.rfc-editor.org/rfc/rfc9220) | WebSocket; over HTTP/2; over HTTP/3 | Standards | Realtime channel |
| [RFC 9297](https://www.rfc-editor.org/rfc/rfc9297) | HTTP Datagrams and the Capsule Protocol | Proposed Standard | Not used for mandatory data |
| [W3C WebTransport](https://www.w3.org/TR/webtransport/) | WebTransport API | Candidate Recommendation Snapshot (July 2026) | Experimental channel |

## Data formats

| Standard | Use |
|---|---|
| [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259) JSON | legacy data, RQP envelopes |
| [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) JSON Canonicalization Scheme | RQP hashes of `json` blocks |
| [JSON Schema 2020-12](https://json-schema.org/specification) | RQP message schemas |
| [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) / [8174](https://www.rfc-editor.org/rfc/rfc8174) | Requirement keywords |

## Security and supply chain

| Standard | Status | Use |
|---|---|---|
| [OWASP MASVS](https://mas.owasp.org/MASVS/) | v2.x | Mobile security requirements, incl. MASVS-PLATFORM, -CODE, -RESILIENCE |
| [OWASP MASWE](https://mas.owasp.org/MASWE/) | v1.0.0 (Aug 2026) | Weakness catalog for test traceability |
| [OWASP MASTG](https://mas.owasp.org/MASTG/) | v2 | Test procedures |
| [SLSA](https://slsa.dev/spec/v1.0/provenance) | v1.x | Build provenance |
| [CycloneDX](https://cyclonedx.org/) / [SPDX](https://spdx.dev/) | Current | SBOM formats |
| [REUSE](https://reuse.software/spec/) | Current | Per-file license information |
| [CVSS v4.0](https://www.first.org/cvss/v4-0/) | Current | Severity |

## ASD-STE100 {#asd-ste100}

| Item | Value |
|---|---|
| Name | ASD-STE100 Simplified Technical English — "Standard for Technical Documentation" |
| Current issue | **Issue 9**, 15 January 2025 |
| Owner | ASD Simplified Technical English Maintenance Group (STEMG) |
| Content | 53 writing rules and a controlled dictionary; Issue 9 revised the wording of 31 rules and 555 dictionary entries; it became an international standard |
| Access | Free copy on request from [asd-ste100.org](https://www.asd-ste100.org/) |

How Rivqen applies it: see the [Style guide](/engineering/style-guide). Rivqen uses the core writing rules (short sentences, one instruction per sentence, imperative mood, active voice, one meaning per word). It does not claim formal STE conformance, because technical names (APIs, headers) are outside the STE dictionary.
