---
name: sprint-plan
description: Plan the next sprint of a Rivqen work package — split into tasks with briefs, assign junior/middle/senior tiers, group into parallel lanes with disjoint file ownership, list contracts and exit criteria. Use after /wp-start or after a sprint closes with work left.
argument-hint: <WP-ID>
---
# Sprint plan: $ARGUMENTS

You are the architect. Follow `docs/engineering/ai/sprint.md` §2–§3 and `docs/engineering/ai/parallel.md`.

1. Open the latest sprint record for `$ARGUMENTS` in `docs/engineering/plan/sprints/`. If the last sprint is closed, create the next record `S<n+1>` from the template.
2. Pick the next slice of the WP. It must end in integrated, tested behavior. Use 3–8 tasks.
3. Write one brief per task with the YAML format in `docs/engineering/ai/sprint.md` §3. Each brief has: goal, source of truth, owns, reads, contracts, acceptance with test names, checks, docs, non-goals, security.
4. Assign the tier with `docs/engineering/ai/roles.md` §4 (first matching row wins).
5. Draw the dependencies. Group independent tasks into lanes. Max 4 code lanes at the same time.
6. Write the ownership table. One path belongs to one lane. Shared files (manifests, lock files, `config.mts`, `AGENTS.md`, `CLAUDE.md`, `.claude/`, status tables) belong to you.
7. List the contracts that you write before the lanes start: types, traits, interfaces, error codes, fixtures, test names, compiling stubs.
8. Write the exit criteria (keep the template checklist; add WP-specific items).
9. Save the briefs in the sprint record (section 2) so that a new session can restart from them.
10. Write the contracts on `wp/$ARGUMENTS`, run the fast checks, and commit with `git commit -s`.

Show the human the plan in a short table (task, tier, lane, depends on). Continue with `/dispatch` unless the human must decide something first.
