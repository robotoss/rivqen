# Golden fixtures

A golden fixture is a set of files that defines one exact behavior: the input, the state, and the expected output. Every Rivqen implementation (Rust core, servers in Node.js, Java, PHP, the reference server) runs the same fixtures.

**Status:** <Badge type="info" text="DESIGN" /> Fixtures are created in WP-02. The catalog below is the minimum set.

[[toc]]

## 1. Fixture layout

```text
fixtures/legacy/FX-LEG-304/
├── fixture.json          # metadata: id, profile, upstream SHA, sources, description
├── cache-before/         # client cache state (template, data, meta) or empty
├── request.http          # raw HTTP request the client sends
├── origin.html           # what the origin renders for this request
├── response.http         # expected raw HTTP response from the server SDK
├── expected.template     # expected template after split (if any)
├── expected.data.json    # expected data blocks (if any)
├── expected.render.html  # expected final document shown to the user
└── expected.actions.json # expected core actions, in order
```

`fixture.json` example:

```json
{
  "id": "FX-LEG-304",
  "profile": "legacy-canonical",
  "upstream": { "repo": "Tencent/VasSonic", "sha": "59936beff656d4b5718ff6444d6c5e001a2c5231" },
  "sources": ["upstream:<path>:<line>"],
  "description": "Cached page, server confirms no change"
}
```

## 2. Rules

1. Files use UTF-8 and LF line ends, except where the fixture tests CRLF or encodings.
2. A fixture changes only through a `spec` commit with protocol-owner review.
3. Never change a fixture to make code pass. Fix the code, or open an ADR if the fixture is wrong.
4. Each fixture names its profile. Divergent upstream behaviors get separate fixtures per profile.
5. Fixtures never contain real user data, real tokens or real domains (use `example.com`).

## 3. legacy protocol catalog (minimum)

| ID | Scenario |
|---|---|
| `FX-LEG-FIRST-LOAD` | No cache; full HTML; template and data stored |
| `FX-LEG-304` | Cache present; `etag` matches; `304` |
| `FX-LEG-DATA-CHANGE` | Same template, data changed; data-only response |
| `FX-LEG-TEMPLATE-CHANGE` | Template changed; full HTML; `template-change: true` |
| `FX-LEG-OFFLINE-TRUE` | `cache-offline: true` |
| `FX-LEG-OFFLINE-FALSE` | `cache-offline: false` |
| `FX-LEG-OFFLINE-STORE` | `cache-offline: store` |
| `FX-LEG-OFFLINE-HTTP` | `cache-offline: http` (disable acceleration for a period) |
| `FX-LEG-MALFORMED-MARKERS` | Unclosed, mismatched or broken markers |
| `FX-LEG-DUPLICATE-BLOCKS` | Same block name twice |
| `FX-LEG-TITLE` | Special handling of the `<title>` element (see [markers grammar](/engineering/protocol/legacy-markers)) |
| `FX-LEG-UNICODE` | Non-ASCII content and block names |
| `FX-LEG-COMPRESSED` | gzip / br response |
| `FX-LEG-INTERRUPTED` | Stream cut in the middle |
| `FX-LEG-CONCURRENT` | Two sessions for the same URL |
| `FX-LEG-AUTH-CHANGE` | Partition change during a session |
| `FX-LEG-REDIRECT` | 301/302 on the main document |
| `FX-LEG-COOKIE-VARIANCE` | Different cookies, same URL |
| `FX-LEG-NON-SONIC-CLIENT` | Browser without legacy headers gets unchanged HTML |

The exact expected outputs are derived from the upstream audit and confirmed with captured traces (WP-01).

## 4. Rivqen protocol catalog (initial)

| ID | Scenario |
|---|---|
| `FX-RQ-NEGOTIATE` | Client and server agree on RQP |
| `FX-RQ-UNSUPPORTED` | Server ignores RQP headers |
| `FX-RQ-PATCH-OK` | Valid patch applied |
| `FX-RQ-PATCH-BASE-MISMATCH` | Stale base revision |
| `FX-RQ-PATCH-TEMPLATE-MISMATCH` | Template revision changed |
| `FX-RQ-PATCH-SEQUENCE-GAP` | Missing sequence number |
| `FX-RQ-PATCH-SCRIPT` | Patch with a `<script>` tag is rejected |
| `FX-RQ-PATCH-HASH` | Wrong block hash |
| `FX-RQ-DOWNGRADE-DENIED` | Downgrade not allowed by policy |

## 5. Conformance runner

The runner (`server/conformance`) is language-neutral. It starts the server under test, sends `request.http`, and compares the response with `response.http` using these rules:

| Part | Comparison |
|---|---|
| Status | Exact |
| Protocol headers | Exact names (case-insensitive) and values |
| Other headers | Presence of required headers (`Content-Type`, `Cache-Control`, `Vary`) |
| HTML body | Byte-exact (after decoding transfer and content encodings) |
| JSON body | Semantic equality (key order ignored), unless the fixture marks it byte-exact |

## Related

- [Test strategy](/engineering/quality/testing)
- [Server conformance](/engineering/server/conformance)
