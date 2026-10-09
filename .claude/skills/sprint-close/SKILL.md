---
name: sprint-close
description: Close a Rivqen sprint — run all tests and linters, mutation check on the sprint diff, /code-review, security review where needed, docs build, update records, push the wp/ branch; when the WP is complete, give the human the merge request text and stop. Use when all lanes of a sprint are integrated.
argument-hint: <WP-ID> <sprint number>
---
# Sprint close: $ARGUMENTS

You are the architect. Follow `docs/engineering/ai/sprint.md` §6. Work on `wp/$0`.

1. **All tests.** Run the full test suite of every package in the repository (see `docs/engineering/standards/` for each language's command). Not only the changed packages.
2. **Lint and format** in check mode for every language in the repository.
3. **Mutation.** Run `/mutation-check main` on the sprint diff. Critical modules (parsers, cache, FSM, patch apply, security boundaries) need a kill rate ≥ 80 %. Record survivors: fixed, equivalent (with reason), or accepted (with reason).
4. **Code review.** Run `/code-review high` on the sprint diff (`main...wp/$0`). Fix each real finding (send it to a lane or fix a one-liner yourself). Record findings you do not fix, with the reason.
5. **Security.** If the sprint changed a security boundary, run the `security-reviewer` agent on that diff. Critical and high findings block the close.
6. **Docs.** Run `/doc-sync`, then `npm run docs:build`.
7. **Records.** Fill in "Results" in `docs/engineering/plan/sprints/$0-S$1.md`, set the state to Closed, update the WP status in `docs/engineering/plan/work-packages.md` and `docs/guide/status.md`.
8. Commit with `git commit -s` (no AI trailers). Push `wp/$0` as a backup.

If the WP is complete:

9. Check each acceptance item of the WP. Write the evidence (paths, test names, results) in the sprint record. Ask the human to confirm acceptance items that need a human (gate).
10. Commit the sprint record with `git commit -s`. Push `wp/$0`.
11. Give the human the merge request (MR) title and description: deliverables, test results, mutation score, review findings, changed pages.
12. Stop. The human opens and merges the MR into `main` and deletes `wp/$0`. Never push to `main`. Never force-push.

Give the human a short report: tests, mutation score, review findings, docs changed, what is next.
