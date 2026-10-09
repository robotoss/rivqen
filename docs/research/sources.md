# Sources and confidence

This page lists the main sources used in milestone M1 and how much confidence each one deserves.

## Confidence levels

| Level | Meaning |
|---|---|
| **A** | Primary source, checked directly (code at a pinned commit, RFC text, registry data, official docs) |
| **B** | Official source summarized by a reliable secondary source; needs confirmation before a decision |
| **C** | Secondary or community source only; use as a hint |

## Upstream

| Source | Confidence | Used for |
|---|---|---|
| Tencent/VasSonic at `59936bef` (code) | A | Protocol, client, server behavior |
| Tencent/VasSonic wiki at `a81e90dc` | A (as documentation of intent) | Marker conventions, FAQ |
| `assets/sonic发展历程.md`, `Sonic2.0.md`, `VasSonic3.0_preload.md` | A (as design notes) | History, local server mode, preload |
| Upstream performance claims | C | Not used |

## Standards and platforms

| Source | Confidence | Used for |
|---|---|---|
| RFC Editor (RFC 9110–9114, 9111, 5861, 9651, 9842, 3229, 6455, 9220, 9297, 8785) | A | Protocol and cache design |
| W3C WebTransport (CR Snapshot July 2026) | B | Transport status |
| Android developers: Play target API, API 37 diff | A / B | Android baselines |
| Apple developer documentation (WebKit) | A | iOS constraints |
| iOS 27 release reports | B | iOS baselines |
| crates.io, npm registry (queried 9 Oct 2026) | A | Versions and licenses |
| GitHub action tags (queried 9 Oct 2026) | A | Workflow versions |
| Node.js release schedule (via secondary reports) | B | Node.js baselines |
| php.net supported versions | A / B | PHP baselines |
| Spring Boot system requirements | A | Java baselines |
| ASD-STE100 STEMG site and announcements | A / B | Style guide |
| OWASP MAS (MASVS, MASWE, MASTG) | A | Security requirements |

## Literature

All papers in the [Literature review](/research/literature) are peer-reviewed or standards documents (confidence A for their own claims). Their application to Rivqen is design interpretation.

## Name check (not legal clearance)

| Registry | Result on 9 Oct 2026 |
|---|---|
| npm | `rivqen` not found |
| crates.io | No crate matching `rivqen` |
| Packagist | No `rivqen` package (only unrelated similar names) |
| GitHub | Repository `robotoss/rivqen` (this project) |

A trademark search is still required (`BRAND-001`).
