# Documentation as you go

Documentation is part of each task. The docs must always describe the code that is in `main`. This keeps knowledge when a session ends, when context is compacted, or when another agent takes over.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 1. What to update and when

| Event | Update | Who |
|---|---|---|
| A task changes behavior | The engineering page of that module; status badges (`DESIGN` → `FACT` when code and tests exist) | The lane, in its owned pages |
| A task changes the public API | Guide pages and code samples; remove "Planned API" only when the API is released | Lane drafts, docs steward applies |
| A decision is made | Decision log or ADR | Architect |
| A question opens or closes | [Open questions](/engineering/plan/open-questions) | Architect |
| A task finishes | Sprint record: status, test results, mutation score | Architect |
| A sprint ends | WP status in [Work packages](/engineering/plan/work-packages); [Project status](/guide/status) | Architect, docs steward |
| A new page is added | Sidebar in `config.mts` | Architect or docs steward |
| A standard changes | The standards page **and** the matching `.claude/rules/` file, in the same commit | Architect |

## 2. The sprint record

The sprint record is the memory of the work. Keep it current during the sprint, not only at the end.

- Path: `docs/engineering/plan/sprints/<WP-ID>-S<n>.md`, from the [template](/engineering/plan/sprints/template).
- Update it after each integrated lane.
- A new session reads the sprint record first and continues from it.

## 3. Rules

1. Write in English. Use [ASD-STE100](/engineering/style-guide) rules for procedures.
2. Mark each statement `FACT`, `DESIGN`, `RESEARCH` or `GATE`.
3. Do not describe planned behavior as current.
4. Link to code by path, not by copied code, when the code is long.
5. Build the site before the sprint closes: `npm run docs:build`. Dead links fail the build.

## Related

- [Documentation style guide](/engineering/style-guide)
- [Sprint workflow](/engineering/ai/sprint)
