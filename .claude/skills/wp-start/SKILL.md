---
name: wp-start
description: Kick off a Rivqen work package — analyse what exists, what is needed and how best to do it, list risks and questions, and create the first sprint record. Use when the user or the plan starts a new WP (for example "start WP-02").
argument-hint: <WP-ID>
---
# WP kickoff: $ARGUMENTS

You are the architect. Follow `docs/engineering/ai/sprint.md` §1. Do not write production code in this step.

1. Read the WP section for `$ARGUMENTS` in `docs/engineering/plan/work-packages.md`: goal, deliverables, acceptance, dependencies, phase.
2. Check the dependencies. If a dependency WP is not done, stop and report which one.
3. Read the pages the WP links to, the related ADRs (`docs/engineering/architecture/adr/index.md`), the gate in `docs/engineering/plan/gates.md`, and the open questions with this WP as owner.
4. **What exists.** Search the repository (`Glob`, `Grep`, `git log`) for code, tests, fixtures, evidence and docs for this WP. Write each item with its path and state (usable / partial / missing).
5. **What is needed.** For each acceptance item, write the gap.
6. **How best.** For each gap, write the approach: the pattern from `docs/engineering/standards/`, existing code to reuse, libraries (license class!). For facts outside the repository, delegate to the `researcher` agent; run several research questions in parallel.
7. **Risks and questions.** Classify each question with `docs/engineering/ai/decisions.md`. Decide architect questions now and add them to `docs/engineering/plan/decision-log.md`. Ask the human the critical ones with `AskUserQuestion` (max 4 per call, recommended option first).
8. Create the integration branch: `git checkout -b wp/$ARGUMENTS main`.
9. Copy `docs/engineering/plan/sprints/template.md` to `docs/engineering/plan/sprints/$ARGUMENTS-S1.md`. Fill in the header (State: `Planned`) and the kickoff section. Add the record to `docs/engineering/plan/sprints/index.md` and to the sidebar.
10. Set the WP status to "In progress" in `docs/engineering/plan/work-packages.md`.
11. Commit on `wp/$ARGUMENTS` with `git commit -s`.

Finish with a short summary for the human: what exists, the main gaps, the decisions made, the questions still open, and the proposed first sprint goal. Then continue with `/sprint-plan $ARGUMENTS` unless a critical question blocks it.
