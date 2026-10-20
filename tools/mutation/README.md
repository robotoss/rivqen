# Mutation helper

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

`diff-ranges.mjs` turns the changed lines of a git diff into arguments for a mutation tool, so that mutation testing runs only on new or changed code. The procedure is in [`docs/engineering/quality/mutation.md`](../../docs/engineering/quality/mutation.md).

1. Go to the directory where the mutation tool runs.
2. Run `node <repo>/tools/mutation/diff-ranges.mjs --base main --ext .ts,.js,.mjs --format stryker`. Other formats: `files` (Muter), `pit-classes` (PIT). The script header lists all options.
3. Give the output to the mutation tool (for StrykerJS: `--mutate`).

Tests: `node --test "tools/mutation/test/*.test.mjs"`.
