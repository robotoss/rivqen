# Rivqen protocol: manifest and patch

This page defines the two RQP documents: the **page manifest** (what a page version contains) and the **patch envelope** (how to go from one version to the next).

**Status:** <Badge type="info" text="DESIGN" /> Draft. JSON Schemas are produced in WP-17.

[[toc]]

## 1. Page manifest

The server sends the manifest with a full document, inline in `<head>`: `<script type="application/rivqen-manifest+json">`. There is no other delivery form ([ADR-006](/engineering/architecture/adr/#adr-006)).

```json
{
  "protocol": "rqp/1",
  "origin": "https://shop.example.com",
  "path": "/catalog",
  "template_revision": "tpl-008",
  "page_revision": "rev-042",
  "blocks": [
    { "id": "price", "format": "html", "sha256": "<hex SHA-256 of canonical bytes>" },
    { "id": "stock", "format": "json", "sha256": "<hex SHA-256 of canonical bytes>" }
  ],
  "policy": { "mode": "private", "ttl_seconds": 60, "offline": false },
  "limits": { "max_patch_bytes": 262144 }
}
```

| Field | Rule |
|---|---|
| `protocol` | MUST be `rqp/1` |
| `origin`, `path` | MUST match the navigation. The client compares them with the real URL, not with the manifest alone. |
| `template_revision`, `page_revision` | Opaque strings |
| `blocks[].id` | `[a-z0-9][a-z0-9_-]{0,63}`, unique in the page |
| `blocks[].format` | `html` or `json` |
| `blocks[].sha256` | Hex SHA-256 over the canonical block bytes (section 3) |
| `policy` | Cache and offline policy. Cannot be weaker than HTTP headers. |
| `limits` | Server hint. The client uses the lower of server and client limits. |

The manifest MUST NOT contain cookies, tokens, personal data or executable code.

## 2. Patch envelope

```json
{
  "type": "rivqen.patch",
  "protocol": 1,
  "page_id": "catalog-main",
  "base_revision": "rev-041",
  "next_revision": "rev-042",
  "template_revision": "tpl-008",
  "sequence": 17,
  "operations": [
    { "op": "replace_block", "block_id": "price", "format": "html",
      "content": "<p class=\"price\">130.00</p>", "sha256": "<hex>" }
  ]
}
```

| Operation | Fields | Effect |
|---|---|---|
| `replace_block` | `block_id`, `format`, `content`, `sha256` | Replace the whole block |
| `remove_block` | `block_id` | Remove the block content (the placeholder stays) |
| `insert_block` | `block_id`, `after`, `format`, `content`, `sha256` | Only for blocks declared optional in the template |

Validation follows the order in [Diff engine §3](/engineering/core/diff-engine#_3-rqp-patch-validation).

## 3. Canonical bytes

Hashes are computed over **canonical bytes**, never over JSON text:

| Format | Canonical bytes |
|---|---|
| `html` | UTF-8 bytes of the block content exactly as it appears between the markers; no normalization of whitespace; LF and CRLF are different bytes |
| `json` | RFC 8785 JSON Canonicalization Scheme (JCS) output, UTF-8 |

<Badge type="warning" text="RESEARCH" /> Confirm that JCS libraries exist and agree for Rust, Kotlin/Java, Swift, TypeScript and PHP (WP-17).

## 4. Content rules

1. `html` content MUST NOT contain `<script>`, `<iframe>`, `<object>`, `<embed>`, event-handler attributes (`on*`), or `javascript:` URLs. The client rejects the patch if it does.
2. `json` content is data. The page renders it with its own code.
3. The client inserts `html` content only into SDK-owned regions. In React apps, prefer `json` blocks and state updates.

## Related

- [RQP negotiation](/engineering/protocol/negotiation)
- [Diff engine](/engineering/core/diff-engine)
- [Web and React SDK](/engineering/platforms/web-react)
