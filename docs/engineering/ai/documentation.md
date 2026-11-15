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

## 4. Documentation check

<Badge type="info" text="DESIGN" /> Two checks compare the docs with the code (DL-179). The two directions find different defects. From the docs to the code: a statement that is false or stale. From the code to the docs: a public item that no page describes.

Rules for both checks:

1. The agents only read. They do not edit, build or commit.
2. Each finding has the page line, the text, the command and its output.
3. The architect verifies each finding in the source before a fix. A simple agent also gives false findings.
4. The architect does not rewrite a closed sprint record: a number that was true at that time stays.

### 4.1 Light check: at the end of each WP

Direction: from the docs to the code. Run it after the last sprint record is closed and before the MR text.

1. Start junior agents in parallel, one for each group of pages. Give each agent a list of concrete items to compare.
2. Scope: the pages that the WP changed, the pages that name the WP, the status pages, the plan pages (work packages, open questions, risks), the security pages that name the changed module, and the READMEs of the changed tools.
3. Items to compare:
   - Each path, function, type, constant, test name and count in backticks exists in the repository as written.
   - The states and the numbers agree between the status pages and the sprint records.
   - No finished item has the word "planned", and no page gives the item to another WP.
   - Each follow-up of the sprint record is visible from its target WP in [Work packages](/engineering/plan/work-packages), and each open item for the human is in [Open questions](/engineering/plan/open-questions).
   - Each decision id that a page cites has a record.
4. Fix the confirmed findings. Build the site. Commit on `wp/<WP-ID>`.

<Badge type="tip" text="FACT" /> First run (WP-06, 2026-11-14): six agents, about 5 minutes each, about 20 findings, 3 of them false; 14 files fixed.

### 4.2 Full check: at gate G3 and before each release gate

Both directions, on every page of the site. <Badge type="danger" text="GATE" /> It is a condition of gate G3 ([Gates](/engineering/plan/gates), H-145).

1. From the code to the docs first. Make a list of every public item of the core: functions, types, error codes, limits, Cargo features, commands. Each item must be on a page. Use the middle or senior tier: the agent must decide what a reader needs.
2. From the docs to the code. Check every statement with the `FACT` badge, with junior agents, as in section 4.1.
3. Run the measurements again: times, memory values and counts that need a run. A read-only agent cannot check them.
4. Write the result in the sprint record of the gate.

Do not run the full check before G3: WP-07 and WP-08 change the cache, data model, diff and session pages.

### 4.3 A check that repeats becomes a test

When the same comparison is needed more than one time, write a test that reads the page. Example: the row of `RQP_MARKUP_LIMIT` was missing in [errors](/engineering/core/errors) for two WPs, because the test `model_error_catalog_matches_errors_md` compares the code with a table in the test file, not with the page. Follow-up: [WP-08](/engineering/plan/work-packages#wp-08).

## Related

- [Documentation style guide](/engineering/style-guide)
- [Sprint workflow](/engineering/ai/sprint)
