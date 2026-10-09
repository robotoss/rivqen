# Sprint workflow

A work package (WP) is done in one or more sprints. A sprint is a small set of tasks (usually 3–8) that ends with integrated, tested and documented code. This page gives the procedure for each step.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 1. WP kickoff

Run `/wp-start <WP-ID>`. The architect does these steps. The researcher can help with step 3.

1. Read the WP in [Work packages](/engineering/plan/work-packages): goal, deliverables, acceptance, dependencies.
2. Read the pages the WP links to, the related ADRs, gates and open questions.
3. **What exists.** List the code, tests, fixtures and docs that already exist for this WP. Give paths.
4. **What is needed.** List each acceptance item and the gap to it.
5. **How best to do it.** For each gap, write the approach. Check the [standards](/engineering/standards/) and the existing patterns in the repository. Ask the researcher for facts that are not in the repository.
6. **Risks and questions.** List them. Classify each question as architect or human ([Decisions](/engineering/ai/decisions)). Ask the human the critical ones now.
7. Check the dependencies. If a dependency WP is not done, stop and report.
8. Write the kickoff section of the first sprint record.

Output: `docs/engineering/plan/sprints/<WP-ID>-S1.md` with the kickoff section filled in.

## 2. Sprint plan

Run `/sprint-plan <WP-ID>`.

1. Split the next part of the WP into tasks. One task gives one observable behavior.
2. For each task, write the brief (section 3).
3. Give each task a tier with the [routing rules](/engineering/ai/roles#_4-routing-rules).
4. Draw the dependencies. Group independent tasks into lanes.
5. Write the ownership table ([Parallel work](/engineering/ai/parallel#_3-ownership)).
6. List the contracts that the architect writes before the lanes start.
7. Write the sprint goal and the exit criteria.

## 3. Task brief

```yaml
id: WP-06/S1/T-03
title: Parse data-rq-block attributes
tier: middle            # junior | middle | senior
goal: One observable behavior, in one sentence
source_of_truth:
  - docs/engineering/protocol/markup.md §3
  - ADR-006
owns:                    # the only paths this lane may change
  - crates/rivqen-template/src/markup/**
reads:
  - crates/rivqen-integrity/**
contracts:               # fixed before the lane starts; do not change
  - crates/rivqen-template/src/block.rs (BlockId, BlockFormat)
acceptance:
  - Behaviors that must be true, each with a test name
tests:
  unit: [names]
  property: [invariants]
  mutation: changed lines in owns, kill rate >= 80 %
checks:                  # commands the lane runs before it reports
  - cargo fmt --check
  - cargo clippy --all-targets -- -D warnings
  - cargo nextest run -p rivqen-template
docs:                    # pages this lane updates
  - docs/engineering/core/template-engine.md §4
non_goals:
  - What this task does not do
security:
  - Invariants that must hold (no panic on any input, bounded memory)
```

## 4. Task report

Each lane returns this report. The architect rejects a report without check results.

```yaml
id: WP-06/S1/T-03
status: DONE            # DONE | BLOCKED | RESCOPE
summary: What changed, in 3 lines
changed_paths: [list]
checks:                 # command → result
  cargo nextest run -p rivqen-template: 42 passed
mutation: 37/44 killed (84 %); 3 equivalent (reasons); 4 survived → tests added
docs_updated: [pages]
shared_file_requests:   # changes for files the lane does not own
  - Cargo.toml: add proptest = "1" to [workspace.dependencies]
questions: [open questions, if any]
notes_for_review: Risky places the reviewer must look at
```

## 5. Testing inside a task

Write a small piece, test it, then move on.

1. Write the smallest piece of code that gives one behavior.
2. Write the test for that behavior. Run it.
3. Repeat until the task goal is done.
4. Run the mutation check on the task diff ([Mutation testing](/engineering/quality/mutation)). Fix weak tests. Do not add tests only to kill an unimportant mutant.
5. Run the checks in the brief.

## 6. Sprint close

Run `/sprint-close <WP-ID> <n>` after all lanes are integrated.

1. Run **all** tests of the repository, not only the changed packages.
2. Run all linters and formatters in check mode.
3. Run the mutation check on the **sprint diff** (`main...wp/<WP-ID>`). The kill rate on critical modules must be ≥ 80 %.
4. Run `/code-review high` on the sprint diff. Fix each finding or record why not.
5. For changes to a security boundary, run the security reviewer.
6. Build the site: `npm run docs:build`.
7. Update the sprint record, the WP status and the project status.
8. Commit on `wp/<WP-ID>` and push the branch as a backup.

If the WP is complete:

9. Check each acceptance item of the WP. Write the evidence in the sprint record.
10. Squash `wp/<WP-ID>` into one commit on `main`. Use a commit message that lists the deliverables.
11. Push `main`. Delete `wp/<WP-ID>`.

## Related

- [Parallel work](/engineering/ai/parallel)
- [Mutation testing](/engineering/quality/mutation)
- [Sprint records](/engineering/plan/sprints/)
