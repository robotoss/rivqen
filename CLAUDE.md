@AGENTS.md

# Claude Code additions

The shared rules are in `AGENTS.md` (imported above). This part maps them to Claude Code features.

## Roles

- Main session = **architect**. Delegate with the subagents in `.claude/agents/`:

| Agent | Model | Use for |
|---|---|---|
| `junior` | haiku | Mechanical, fully specified tasks |
| `middle` | sonnet | One module from a clear spec, with tests |
| `senior` | opus | FSM, concurrency, FFI, `unsafe`, cache commit, security-sensitive parsing, unclear specs |
| `researcher` | sonnet | Facts with sources; read-only |
| `docs-steward` | sonnet | Shared doc pages, sidebar, status tables |
| `security-reviewer` | opus | Read-only review of security boundaries |

- Executors run with worktree isolation (`isolation: worktree`). Lane worktrees start from the current `HEAD` (`worktree.baseRef: "head"`), so check out `wp/<WP-ID>` before you dispatch.
- Ask the human critical questions with the `AskUserQuestion` tool. Put the recommended option first and mark it "(Recommended)".

## Skills

| Skill | Use |
|---|---|
| `/wp-start <WP>` | WP kickoff: what exists, what is needed, how best, risks, questions |
| `/sprint-plan <WP>` | Tasks, tiers, lanes, ownership, contracts, exit criteria |
| `/dispatch <WP> <sprint>` | Start the lanes as subagents in worktrees; integrate |
| `/decide <question>` | Classify and record a decision, or escalate to the human |
| `/doc-sync` | Update the docs that the current change affects |
| `/mutation-check [base]` | Mutation testing on changed lines only |
| `/sprint-close <WP> <sprint>` | Full tests, mutation, `/code-review high`, docs, commit, push `wp/<WP>`; at WP end give the human the MR text and stop (never push `main`) |

`/code-review` is the built-in Claude Code review command. Use `/code-review high` on the sprint diff at sprint close.
