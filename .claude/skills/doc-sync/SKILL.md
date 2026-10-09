---
name: doc-sync
description: Update the Rivqen documentation that the current change affects — module pages, status badges, guide samples, decision log, open questions, sprint record, WP status, sidebar — and build the site. Use at the end of every task and before every commit that changes behavior.
---
# Doc sync

Follow `docs/engineering/ai/documentation.md`. The docs must describe the code that is in the branch.

1. List the changed files: `git diff --name-only $(git merge-base HEAD main)`.
2. For each changed area, find its pages: `Grep` the docs for the module, crate, package or file names.
3. Update each page you own (subagents: only the pages in your brief; list the others in `shared_file_requests`):
   - Behavior that now exists in code with tests → change `DESIGN` to `FACT` for that statement.
   - New public API → update the guide sample; keep "Planned API — not released" until a release.
   - New page → ask the architect to add it to `docs/.vitepress/config.mts`.
   - New decision → decision log or ADR. Opened/closed question → `docs/engineering/plan/open-questions.md`.
   - Changed standard → the page in `docs/engineering/standards/` and the matching `.claude/rules/` file together.
4. Style: English, ASD-STE100 for procedures, short sentences, tables readable at 375 px, no raw `{{ }}`.
5. Run `npm run docs:build`. Fix dead links.
6. Report the pages you changed.
