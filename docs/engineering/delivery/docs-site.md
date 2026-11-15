# Documentation site

This page explains how the documentation site is built and published, and how to add a page.

**Status:** <Badge type="tip" text="FACT" /> This setup exists in the repository.

## 1. Stack

| Part | Choice | Version | License |
|---|---|---|---|
| Static site generator | [VitePress](https://vitepress.dev/) | 1.6.4 (pinned) | MIT |
| Diagrams | [Mermaid](https://mermaid.js.org/) via `vitepress-plugin-mermaid` | 11.17.2 / 2.0.17 (pinned) | MIT |
| Search | VitePress local search (MiniSearch) | built in | MIT |
| Hosting | GitHub Pages via GitHub Actions | — | — |

Versions are pinned exactly in `package.json`. Update them in a separate pull request and check every diagram after the update.

<Badge type="tip" text="FACT" /> Known advisories on 2026-11-15 (`npm audit`, DL-187): `vite` 5.4.21 and `esbuild` 0.21.5 (through VitePress 1.6.4; dev server) and `katex` 0.16.47 (through Mermaid 11). The `markdown-it` copy inside VitePress 1.6.4 also has advisories that `npm audit` does not show. No fix exists in the VitePress 1.x line or in Mermaid 11. The site has only our own pages, so the risk is accepted. Do not expose `npm run docs:dev` to a network.

## 2. Run locally

1. Install Node.js 20.19 or later. Node.js 22 LTS is recommended.
2. Run `npm ci` in the repository root.
3. Run `npm run docs:dev`.
4. Open the URL that the command shows.

## 3. Add a page

1. Create a Markdown file in the correct layer: `docs/guide/`, `docs/engineering/`, `docs/research/` or `docs/legal/`.
2. Add the page to the sidebar in `docs/.vitepress/config.mts`.
3. Follow the [style guide](/engineering/style-guide).
4. Run `npm run docs:build`. The build fails on dead internal links.
5. Check every diagram in light and dark mode and at 375 px width.

## 4. Publish

1. Merge the pull request into `main`.
2. The `Docs` workflow builds the site and deploys it.
3. The site is at `https://robotoss.github.io/rivqen/`.

If you use a custom domain, set the repository variable or build environment `DOCS_BASE=/` and add `docs/public/CNAME`.

## 5. Layout rules

| Element | Rule (implemented in `docs/.vitepress/theme/custom.css`) |
|---|---|
| Diagrams | In a bordered box; wide diagrams scroll horizontally inside the box; the page never scrolls sideways |
| Tables | Scroll horizontally inside the table on small screens; long identifiers wrap |
| Status labels | VitePress `Badge`: FACT = tip, DESIGN = info, RESEARCH = warning, GATE = danger |
