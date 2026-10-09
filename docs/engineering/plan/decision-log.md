# Decision log

The architect records here the decisions that do not need an ADR. ADRs are for decisions that the human approves ([Decisions](/engineering/ai/decisions)).

**Status:** <Badge type="info" text="DESIGN" />

## Entries

| ID | Date | WP | Question | Decision | Reason | How to reverse |
|---|---|---|---|---|---|---|
| DL-001 | 2026-10-09 | WP-24 | Where do AI rules live? | `CLAUDE.md` + `.claude/` for agents; full text in `docs/engineering/ai/` and `docs/engineering/standards/` | Agents load `.claude/` automatically; humans read the site. One source of truth: the site, with short checklists in `.claude/rules/` | Move the checklists into `CLAUDE.md` imports |
| DL-002 | 2026-10-09 | WP-24 | How do parallel agents avoid merge conflicts? | Worktree per lane, path ownership, contracts first, architect owns shared files | Conflicts become a planning error that is visible in the ownership table | Sequential lanes |
| DL-003 | 2026-10-09 | WP-24 | Scope of mutation testing | Diff only, critical modules, kill rate ≥ 80 % | Fast enough for each task; focuses on code that can break users | Set a full run per phase gate |
| DL-004 | 2026-10-09 | WP-24 | One rules file for all AI tools? | `AGENTS.md` holds the tool-neutral rules; `CLAUDE.md` starts with `@AGENTS.md` and adds Claude Code specifics | The [agents.md](https://agents.md) convention is read by many tools; Claude Code reads `AGENTS.md` itself only when no `CLAUDE.md` exists, so the import is the documented way to use both | Merge back into `CLAUDE.md` |
| DL-005 | 2026-10-09 | WP-24 | How to scope JVM mutation without line-level git support | Class patterns from changed files (`pkg.Class`, `pkg.Class$*`, `pkg.ClassKt`) | Open-source PIT has no git-diff mode; prefix globs would include sibling classes | Arcmutate plugins if Q-16 says yes |
| DL-006 | 2026-10-09 | Process (`chore/mr-delivery`) | How does a WP reach `main`? | Delivery to `main` only through an MR that the human merges. An agent pushes only its own branch (`wp/<WP-ID>` or `chore/<name>`), never `main`, and never force-pushes. The human deletes the branch after the merge. Decided by the human. | The human reviews every change before it reaches `main`. Replaces the old rule "the architect squashes the WP into `main` and pushes `main`". | A new decision by the human |

Decisions DL-001…DL-004 and DL-006 were confirmed by the human owner.

## How to add an entry

1. Use the next free ID.
2. Write one question per entry.
3. Link the sprint record in the WP column when there is one.
