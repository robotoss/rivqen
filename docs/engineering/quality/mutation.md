# Mutation testing

Mutation testing changes the code on purpose (for example `<` to `<=`) and checks that a test fails. A surviving mutant shows a weak or missing test. Rivqen runs it **only on new or changed lines**, so that it stays fast and useful.

**Status:** <Badge type="info" text="DESIGN" /> policy in force from WP-24 · <Badge type="tip" text="FACT" /> tool versions checked 2026-10-09

[[toc]]

## 1. Policy

| Rule | Value |
|---|---|
| Scope | Changed lines (or changed files/classes where the tool cannot do lines) since the base: `main` for a sprint, the lane start for a task |
| Gate | Kill rate **≥ 80 %** on **critical modules** (section 2) |
| Other modules | Run and report; no gate |
| When | End of each task (task diff); sprint close (sprint diff) |
| Full runs | Not part of the sprint. Optional before a phase gate. |
| Survivors | Each one is: **fixed** (a better test), **equivalent** (no behavior change, with the reason), or **accepted** (not worth a test, with the reason) |

::: warning Do not inflate the test suite
A test that kills a mutant must check a behavior that matters. If the only way to kill a mutant is a test of an implementation detail, mark the mutant as accepted and write why. Delete tests that duplicate other tests.
:::

## 2. Critical modules

| Area | Paths (planned) |
|---|---|
| Markup and template parsing | `crates/rivqen-template` |
| Diff and patch apply | `crates/rivqen-diff` |
| Session state machine | `crates/rivqen-session` |
| Cache keys, partitions, commit | `crates/rivqen-cache` |
| Protocol codecs | `crates/rivqen-proto`, `crates/rivqen-proto-legacy` |
| Integrity and revisions | `crates/rivqen-integrity` |
| Bridge message validation | `sdk/web-core` (bridge), platform bridge adapters |
| Server decision functions | Core decision code of `server/node`, `server/java`, `server/php` |

## 3. Tools and commands

### Helper: changed lines

`tools/mutation/diff-ranges.mjs` converts `git diff` into the arguments that the tools need. It compares `merge-base(base, HEAD)` with the working tree, follows renames, skips test files and warns about untracked files. Run it in the directory where the mutation tool runs: it prints paths relative to that directory and includes only files below it.

```bash
ROOT=$(git rev-parse --show-toplevel)
node $ROOT/tools/mutation/diff-ranges.mjs --base main --ext .ts,.tsx,.js,.mjs --format stryker  # file:start-end,…
node $ROOT/tools/mutation/diff-ranges.mjs --base main --ext .swift --format files              # file,file
node $ROOT/tools/mutation/diff-ranges.mjs --base main --ext .java,.kt --format pit-classes     # pkg.Class*,…
```

### Per language

`$ROOT` is the repository root, set as shown above.

| Language | Tool (version, license) | Scope | Command |
|---|---|---|---|
| Rust | cargo-mutants 27.1.0, MIT | Lines | `D=$(mktemp) && git -c core.quotePath=false diff --src-prefix=a/ --dst-prefix=b/ --no-ext-diff $(git merge-base main HEAD) -- '*.rs' > "$D" && cargo mutants --in-diff "$D" --no-shuffle` |
| TypeScript | StrykerJS 10.0.0 + vitest-runner 10.0.0, Apache-2.0 | Lines | `npx stryker run --mutate "$(node $ROOT/tools/mutation/diff-ranges.mjs --base main --ext .ts,.tsx,.mts,.cts,.js,.jsx,.mjs,.cjs --format stryker)" --incremental` |
| PHP | Infection 0.35.6, BSD-3-Clause | Lines | `vendor/bin/infection --git-diff-lines --git-diff-base=main --min-msi=80 --threads=max` |
| Java | PIT 1.30.0 + Gradle plugin `info.solidsoft.pitest` 1.19.0, Apache-2.0 | Classes | `./gradlew pitest -Ppitest.targetClasses="$(node $ROOT/tools/mutation/diff-ranges.mjs --base main --ext .java --format pit-classes)"` |
| Kotlin | PIT (as Java) | Classes | Same as Java with `--ext .kt`. Advisory until [Q-16](/engineering/plan/open-questions) is closed. |
| Swift | Muter (tag 16, MIT) | Files | `muter --files-to-mutate "$(node $ROOT/tools/mutation/diff-ranges.mjs --base main --ext .swift --format files)"`. Advisory, no gate. |

The exact Gradle property for `targetClasses` is set up in WP-14 / WP-10. The command above shows the intent. The class globs (`pkg.Class*`) also cover inner classes, lambdas and Kotlin file facades (`ClassKt`).

### Notes per tool

1. **Rust.** `--in-diff` mutates only code in changed hunks. It does not react to changes in test code only. Use the default test tool (`cargo test`) for mutation runs: `cargo-nextest` skips doctests, so mutants caught only by doctests would show as missed. Skip a function with `#[mutants::skip]` (needs `mutants` as a normal dependency) and a comment. Configure in `.cargo/mutants.toml` (for example `exclude_re = ["impl Debug"]`).

   Run setup for a crate with many test binaries (measured at the close of [WP-06 S2](/engineering/plan/sprints/WP-06-S2), DL-178):

   - Give the test selection to the build phase: `-C=--test=<name>` (one per test file, `-C=--lib` for the unit tests). Arguments after `--` reach only the test phase, so each mutant still builds every test binary of the package.
   - Use one run per source file (`-f <file>`) with the test files of that file. `cargo test` runs the test binaries in name order, so in one common set the last binary gets the least time.
   - Measure the test time of the selected set at the current machine load and set `--timeout` to 4 times that value or more. Run a timeout again with a longer limit before you count it: a hang and a slow test are not the same result.
   - Run the survivors of a narrow set again on the full package tests before you record them.
   - cargo-mutants copies the tree before each run. In this repository the copy is about 7 GB, because the build directories of other packages (`server/rust-reference/target`, `tools/rqp-markup/rust/target`) and `node_modules` are in it. <Badge type="warning" text="RESEARCH" /> A run from a clean `git worktree` avoids the copy of these directories; not measured.
2. **TypeScript.** Stryker has no git mode; the helper gives it line ranges. Set `"thresholds": { "break": 80 }` for critical packages and use the `typescript` checker so that mutants that do not compile are not counted.
3. **PHP.** Infection is the most complete: line scope, base ref and minimum score built in.
4. **JVM.** Open-source PIT has no git-diff scope (the Maven `scmMutationCoverage` goal was removed in PIT 1.18.0). We scope by changed classes. Line-level git scope and Kotlin-aware mutation exist in the commercial Arcmutate plugins (free licences for open-source projects are announced, not verified). This is a dependency/licence decision → [Q-16](/engineering/plan/open-questions). For Android, PIT runs only on JVM unit tests: keep logic in pure Kotlin/JVM modules or in the Rust core.
5. **Swift.** Muter's last tag is from 2023, but the main branch is active in 2026. It scopes by file only, and Swift 6 support is not verified. Use it as a signal, not as a gate. Keep logic in the Rust core where possible.

## 4. Triage of survivors

1. Read the mutant: which line, which change.
2. Ask: "Can a user, a caller or an invariant see this change?"
   - **Yes** → write or improve a behavior test. Re-run.
   - **No, the code is equivalent** → record it as equivalent with the reason. Consider a simpler code that has no equivalent mutant.
   - **No, but it is not worth a test** (logging text, `Debug` output) → record it as accepted; exclude the pattern in the tool config if it repeats.
3. Write the result in the task report: `killed/total (%)`, equivalent `n`, accepted `n`.

## Related

- [Test strategy](/engineering/quality/testing)
- [Sprint workflow](/engineering/ai/sprint)
- Sources: [cargo-mutants book](https://github.com/sourcefrog/cargo-mutants/tree/main/book/src), [StrykerJS docs](https://github.com/stryker-mutator/stryker-js/tree/master/docs), [Infection CLI options](https://infection.github.io/guide/command-line-options.html), [PIT](https://github.com/hcoles/pitest), [gradle-pitest-plugin](https://github.com/szpak/gradle-pitest-plugin), [Muter](https://github.com/muter-mutation-testing/muter)
