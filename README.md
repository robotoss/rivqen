<p align="center">
  <img src="docs/public/logo.svg" width="96" height="96" alt="Rivqen logo">
</p>

<h1 align="center">Rivqen</h1>

<p align="center">
  <b>Fast first render. Small updates. Safe offline.</b><br>
  An independent, open-source engine that loads HTML pages in mobile and web apps faster,
  sends only the data that changed, and falls back to normal HTTPS when anything goes wrong.
</p>

<p align="center">
  <a href="https://robotoss.github.io/rivqen/">Documentation</a> ·
  <a href="https://robotoss.github.io/rivqen/guide/">Guide</a> ·
  <a href="https://robotoss.github.io/rivqen/engineering/">Engineering</a> ·
  <a href="https://robotoss.github.io/rivqen/engineering/plan/work-packages">Development plan</a>
</p>

---

> [!WARNING]
> **Status: pre-alpha, documentation phase.** This repository contains the design and development documentation. No SDK is published yet. Code samples in the docs show the **planned** API.

## What is Rivqen?

Rivqen makes HTML pages inside mobile apps open faster and use less traffic:

- **Parallel first load.** The native SDK requests the page while the WebView starts.
- **Template and data.** A page is split into a stable template and small data blocks. When only data changes, only data travels.
- **Safe cache and offline.** Pages open from a local cache that is isolated per account, then revalidate.
- **One Rust core.** One deterministic core runs on Android and iOS: protocol, state machine, cache policy, diff.
- **Full stack.** Kotlin, Swift and TypeScript/React clients. Node.js, Java and PHP server SDKs.
- **Own protocol.** A versioned Rivqen protocol (RQP) with `data-rq-block` markup, plus a temporary legacy mode (until 1.5) for migration from Tencent VasSonic.

## Repository layout

| Path | Content |
|---|---|
| `docs/guide/` | Layer 1 — for people who use or evaluate Rivqen |
| `docs/engineering/` | Layer 2 — for people who build Rivqen (architecture, core, protocol, platforms, servers, security, quality, delivery, plan) |
| `docs/research/` | Upstream audit, standards, platform baselines, literature |
| `docs/legal/` | Licensing, third-party policy, clean-room provenance, brand |
| `examples/server-demo/` | Runnable RQP reference server (Node.js) with tests and byte measurements |
| `tools/upstream-lab/`, `evidence/` | Lab that traces the upstream servers; captured traces and measurements |
| `tools/mutation/` | Helper that turns a git diff into mutation-testing arguments |
| `AGENTS.md`, `CLAUDE.md`, `.claude/` | Rules, roles and skills for AI coding agents |
| `.github/workflows/` | `docs.yml` builds and deploys the site; `checks.yml` runs the fast checks |

The planned monorepo layout for code is in [docs/engineering/delivery/repository.md](docs/engineering/delivery/repository.md).

## Run the documentation locally

Requirements: Node.js 20.19 or later (22 LTS recommended).

```bash
npm ci
npm run docs:dev      # local server with hot reload
npm run docs:build    # production build into docs/.vitepress/dist
npm run docs:preview  # preview the production build
```

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Rivqen uses the Developer Certificate of Origin (`git commit -s`) and a strict clean-room policy.

Report security problems privately. See [SECURITY.md](SECURITY.md).

## License

Rivqen is licensed under the [Apache License 2.0](LICENSE). See [NOTICE](NOTICE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Disclaimer

This is an independent, community-developed open-source project. It offers a temporary compatibility mode for the Tencent VasSonic legacy protocol. Tencent and VasSonic are referenced solely for compatibility and attribution purposes. This project is not affiliated with, sponsored by, or endorsed by Tencent.
