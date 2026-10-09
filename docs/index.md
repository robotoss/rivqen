---
layout: home

hero:
  name: Rivqen
  text: Fast first render. Small updates. Safe offline.
  tagline: An independent, open-source engine that loads HTML pages in mobile and web apps faster, sends only the data that changed, and falls back to normal HTTPS when anything goes wrong.
  image:
    src: /logo.svg
    alt: Rivqen
  actions:
    - theme: brand
      text: Read the Guide
      link: /guide/
    - theme: alt
      text: Engineering handbook
      link: /engineering/
    - theme: alt
      text: Development plan
      link: /engineering/plan/work-packages

features:
  - icon: ⚡
    title: Parallel first load
    details: The native SDK starts the HTML request while the WebView starts. The page bytes are ready when the view is ready.
    link: /guide/how-it-works
  - icon: 🧩
    title: Template and data
    details: A page is split into a stable template and small data blocks. When only data changes, only data travels.
    link: /guide/concepts/template-and-data
  - icon: 🦀
    title: One Rust core
    details: One deterministic core runs the protocol, session state machine, cache policy and diff on Android and iOS.
    link: /engineering/core/
  - icon: 🔁
    title: Own protocol, easy migration
    details: A clean, versioned protocol with data-rq-block markup. A temporary legacy mode and a migration tool help you move from VasSonic.
    link: /guide/concepts/protocol-modes
  - icon: 🛡️
    title: Secure by default
    details: TLS is never bypassed. Caches are isolated per account. Patches never run code. Every failure falls back to HTTPS.
    link: /guide/concepts/security
  - icon: 🌐
    title: Full stack
    details: Kotlin, Swift, TypeScript/React clients. Node.js, Java and PHP server SDKs that pass one conformance suite.
    link: /guide/getting-started/
---

<div class="vp-doc" style="max-width: 960px; margin: 48px auto 0; padding: 0 24px;">

::: warning Project status — documentation phase
Rivqen is **pre-alpha**. This site is the **design and development documentation**. No SDK is published yet. Code samples show the **planned** API. See [Project status](/guide/status).
:::

## Two layers of documentation

| Layer | For whom | Start here |
|---|---|---|
| **Guide** | App developers, backend developers, architects who evaluate Rivqen | [What is Rivqen?](/guide/) |
| **Engineering** | Contributors who design, build, test and release Rivqen | [Engineering handbook](/engineering/) |
| **Research** | Everyone who wants the evidence behind the design | [Research overview](/research/) |
| **Legal** | Maintainers, legal reviewers, integrators | [Legal overview](/legal/) |

## How to read status labels

Every important statement has a label:

| Label | Meaning |
|---|---|
| <Badge type="tip" text="FACT" /> | Verified from a cited source (upstream code at a pinned commit, a standard, or official platform documentation). |
| <Badge type="info" text="DESIGN" /> | Proposed Rivqen design. Not built yet. Can change through an ADR. |
| <Badge type="warning" text="RESEARCH" /> | Not confirmed. A spike, test or audit must close it. |
| <Badge type="danger" text="GATE" /> | A blocking condition. Work cannot pass this point until the condition is true. |

<p style="font-size: 0.85em; opacity: 0.8;">Rivqen is an independent project. It is not affiliated with, sponsored by, or endorsed by Tencent. "VasSonic" and "Sonic" are used only to identify the compatible protocol and for attribution.</p>

</div>
