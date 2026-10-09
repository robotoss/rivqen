---
name: mutation-check
description: Run mutation testing only on the lines changed since a base ref (default main) for every language in the diff, triage surviving mutants, and report the kill rate. Use at the end of each task and at sprint close; critical modules need at least 80 % killed.
argument-hint: "[base-ref]"
---
# Mutation check

Follow `docs/engineering/quality/mutation.md`. Base ref: `$ARGUMENTS` if given, otherwise `main`. Below, `BASE` means that ref.

1. Make new files visible to `git diff`: commit them or run `git add -N <file>`. All commands below compare `merge-base(BASE, HEAD)` with the **working tree**, so committed and uncommitted changes count the same for every language.
2. Find the languages in the diff: `git diff --name-only $(git merge-base BASE HEAD)`. Skip test files.
3. Run the tool for each language with changed source lines. Run each command **in the directory where the tool runs** (repository root for Cargo; the package directory for Stryker, Gradle, Composer, SwiftPM). The helper prints paths relative to the current directory. `$ROOT` is the repository root (`git rev-parse --show-toplevel`).

| Language | Command |
|---|---|
| Rust | `git diff $(git merge-base BASE HEAD) > "$TMPDIR/m.diff"` then `cargo mutants --in-diff "$TMPDIR/m.diff" --no-shuffle` (default test tool, so doctests count) |
| TypeScript/JS | `npx stryker run --mutate "$(node $ROOT/tools/mutation/diff-ranges.mjs --base BASE --ext .ts,.tsx,.mts,.cts,.js,.jsx,.mjs,.cjs --format stryker)" --incremental` |
| PHP | `vendor/bin/infection --git-diff-lines --git-diff-base=BASE --min-msi=80 --threads=max` |
| Java / Kotlin | `node $ROOT/tools/mutation/diff-ranges.mjs --base BASE --ext .java,.kt --format pit-classes` → pass the globs as PIT `targetClasses` (see the module's Gradle config); Kotlin is advisory (Q-16) |
| Swift | `muter --files-to-mutate "$(node $ROOT/tools/mutation/diff-ranges.mjs --base BASE --ext .swift --format files)"` — advisory only |

   Skip a language when the helper prints an empty list. If the tool is not set up in this package yet, say so in the report. Do not install a new tool without the architect.
4. For each surviving mutant, decide:
   - The change is visible to a user, caller or invariant → write or improve a **behavior** test, re-run.
   - Equivalent (no behavior change) → record with the reason.
   - Not worth a test (log text, `Debug` output) → record as accepted with the reason.
   Do not write tests of implementation details only to kill a mutant.
5. Critical modules (`docs/engineering/quality/mutation.md` §2) need ≥ 80 % killed. Below that, the task is not done.
6. Report: per language `killed/total (%)`, equivalent `n`, accepted `n`, tests added, and the list of accepted mutants with reasons.
