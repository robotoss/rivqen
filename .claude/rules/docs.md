---
paths:
  - "docs/**/*.md"
  - "docs/.vitepress/**"
---
# Documentation checklist

Source: `docs/engineering/style-guide.md` and `docs/engineering/ai/documentation.md`.

- English. ASD-STE100 for procedures: short sentences, imperative, one instruction per step.
- Mark statements with `<Badge type="tip" text="FACT" />`, `info` DESIGN, `warning` RESEARCH, `danger` GATE.
- Do not describe planned behavior as current. Keep "Planned API — not released" in samples until a release.
- No raw double curly braces in Markdown (Vue interpolation). Tables and diagrams must fit 375 px.
- New page → sidebar entry in `docs/.vitepress/config.mts` (architect-owned).
- Run `npm run docs:build` before you report; dead links fail the build.
