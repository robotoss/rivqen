---
name: dispatch
description: Run the lanes of a planned Rivqen sprint — start junior/middle/senior subagents in worktrees with their briefs, answer BLOCKED questions, and integrate finished lanes one at a time into the wp/ integration branch. Use after /sprint-plan.
argument-hint: <WP-ID> <sprint number>
---
# Dispatch: $ARGUMENTS

You are the architect. Follow `docs/engineering/ai/parallel.md`.

## Before you start
1. Check out `wp/$0`. The contracts and the sprint record must be committed (`git status` clean). Lane worktrees start from this `HEAD`.
2. Read the briefs in `docs/engineering/plan/sprints/$0-S$1.md`. Set its State row to `Running` and commit, so that a new session finds the active sprint.

## Start lanes
3. For each lane with no open dependency, start one subagent of the brief's tier (`junior`, `middle`, `senior`, `docs-steward`) with worktree isolation. Start independent lanes in the same message so they run in parallel. Max 4 code lanes.
4. The prompt to each subagent is the full YAML brief plus: "Return the task report from docs/engineering/ai/sprint.md §4."

## When a lane returns
5. `BLOCKED` → answer it (or `/decide`), then resume the same subagent with the answer.
6. `RESCOPE` → update the plan; re-tier or split the task.
7. Two failures at one tier → move the task one tier up.
8. `DONE` → integrate:
   1. Check the report has check results and a mutation result (middle/senior).
   2. `git diff --stat wp/$0...<lane-branch>` — only owned paths changed. Otherwise send it back.
   3. `git merge --no-ff <lane-branch>` into `wp/$0`.
   4. Apply `shared_file_requests`; regenerate lock files.
   5. Run the fast checks of the affected packages. On failure, send the failure back to the lane.
   6. `git worktree remove <path>` and delete the lane branch.
   7. Add a progress-log line to the sprint record and commit with `git commit -s`.
9. Start the lanes whose dependencies are now integrated.
10. For a diff that crosses a security boundary, run the `security-reviewer` agent on that diff before you mark the task done.

## Conflicts
A merge conflict means the ownership table was wrong. Stop, assign the path to one owner, resolve with the owner's version, record the path as shared in the sprint record.

When all lanes are integrated, continue with `/sprint-close $0 $1`.
