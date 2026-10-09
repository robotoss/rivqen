# Concepts

This section explains the ideas behind Rivqen. Read it before you integrate the SDK.

| Concept | One-line summary |
|---|---|
| [Template and data](/guide/concepts/template-and-data) | A page is a stable template plus small named data blocks. |
| [Cache and revalidation](/guide/concepts/cache-and-revalidation) | Show the cached page at once; ask the server what changed. |
| [Offline mode](/guide/concepts/offline) | Show an allowed copy without network, only when the server permits it. |
| [Protocol modes](/guide/concepts/protocol-modes) | Legacy protocol for compatibility; Rivqen protocol for new features. |
| [Transports](/guide/concepts/transports) | HTTPS always; push channels are optional. |
| [Security guarantees](/guide/concepts/security) | What Rivqen promises, and what it never does. |

## Key terms

| Term | Meaning |
|---|---|
| **Session** | One page load managed by Rivqen, from `open` to close. |
| **Template** | The part of a page that rarely changes. |
| **Data block** | A named part of a page that changes often. |
| **Revision** | A version label of a page or a template. |
| **Partition** | The account scope of cached data. |
| **Fallback** | A normal page load without Rivqen optimizations. |

See the full [Glossary](/guide/glossary).
