# RQP: negotiation and headers

The Rivqen protocol (RQP) has explicit versions, capabilities and revisions. This page defines how a client and a server agree on RQP and which headers they use.

**Status:** <Badge type="info" text="DESIGN" /> Draft. Decided: the `Rq-` header prefix and Structured Field syntax ([ADR-006](/engineering/architecture/adr/#adr-006)). Block markup: [RQP markup](/engineering/protocol/markup).

[[toc]]

## 1. Goals

1. Explicit negotiation of version and capabilities.
2. Same result over HTTP/1.1, HTTP/2 and HTTP/3. Extra channels never affect correctness.
3. Atomic, ordered delivery of patches; rejection of stale patches.
4. A typed page manifest bound to origin, partition and revision.
5. Parsing and hashing rules that every language can reproduce.
6. Partial updates without page re-initialization, with explicit React integration.
7. Optional subresource hints within browser policies.

## 2. Header syntax

RQP headers use **Structured Field Values** ([RFC 9651](https://www.rfc-editor.org/rfc/rfc9651)). This gives one parser for all languages and avoids ad-hoc formats.

| Header | Direction | Type | Example |
|---|---|---|---|
| `Rq-Version` | both | Integer | `Rq-Version: 1` |
| `Rq-Capabilities` | request | Inner list of tokens | `Rq-Capabilities: (block-patch manifest realtime-ws)` |
| `Rq-Base-Revision` | request | String | `Rq-Base-Revision: "rev-041"` |
| `Rq-Revision` | response | String | `Rq-Revision: "rev-042"` |
| `Rq-Template-Revision` | response | String | `Rq-Template-Revision: "tpl-008"` |

<Badge type="warning" text="RESEARCH" /> The intermediary test (CDNs and proxies must pass `Rq-*` headers unchanged) runs in WP-13 and WP-17.

## 3. Request example

```http
GET /catalog HTTP/2
Host: shop.example.com
Accept: text/html, application/vnd.rivqen.delta+json
Rq-Version: 1
Rq-Capabilities: (block-patch manifest)
Rq-Base-Revision: "rev-041"
```

## 4. Response examples

Full document:

```http
HTTP/2 200 OK
Content-Type: text/html; charset=utf-8
Rq-Version: 1
Rq-Revision: "rev-042"
Rq-Template-Revision: "tpl-008"
Cache-Control: private, max-age=0, must-revalidate
Vary: Rq-Version, Rq-Capabilities, Rq-Base-Revision
```

Delta (patch):

```http
HTTP/2 200 OK
Content-Type: application/vnd.rivqen.delta+json
Rq-Version: 1
Rq-Revision: "rev-042"
Rq-Base-Revision: "rev-041"
Cache-Control: private, no-store
Vary: Rq-Version, Rq-Capabilities, Rq-Base-Revision
```

Not modified: `304 Not Modified` with no body, as in HTTP.

## 5. Rules

| ID | Rule |
|---|---|
| RQP-01 | A delta response MUST use the `application/vnd.rivqen.delta+json` media type and MUST include `Vary` for every negotiation header that changes the response. |
| RQP-02 | A server MUST NOT use status `226 IM Used` for RQP deltas. (RFC 3229 delta encoding is a different mechanism.) |
| RQP-03 | `304` MUST NOT carry a body. |
| RQP-04 | A server that does not support RQP ignores the headers and sends a normal response. The client then treats the session as non-RQP. |
| RQP-05 | A client MUST NOT send `Rq-*` headers to origins outside its allowlist. |
| RQP-06 | Unknown capabilities MUST be ignored. A required capability that the server lacks leads to legacy (if allowed) or normal HTTPS. |
| RQP-07 | Revisions are opaque strings of at most 128 bytes, compared byte for byte. |

## 6. Capabilities (initial set)

| Token | Meaning |
|---|---|
| `manifest` | Server sends a page manifest |
| `block-patch` | Server can answer with a patch envelope |
| `json-blocks` | Blocks may have `json` format (for state-based React updates) |
| `realtime-ws` | Server offers a WebSocket channel |
| `realtime-wt` | Server offers a WebTransport channel (experimental) |
| `dict-compression` | Server supports Compression Dictionary Transport for documents (ADR-013) |

## Related

- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Versioning and downgrade](/engineering/protocol/versioning)
