# Decision log

The architect records here the decisions that do not need an ADR. ADRs are for decisions that the human approves ([Decisions](/engineering/ai/decisions)).

**Status:** <Badge type="info" text="DESIGN" />

## Entries

| ID | Date | WP | Question | Decision | Reason | How to reverse |
|---|---|---|---|---|---|---|
| DL-001 | 2026-10-09 | WP-24 | Where do AI rules live? | `CLAUDE.md` + `.claude/` for agents; full text in `docs/engineering/ai/` and `docs/engineering/standards/` | Agents load `.claude/` automatically; humans read the site. One source of truth: the site, with short checklists in `.claude/rules/` | Move the checklists into `CLAUDE.md` imports |
| DL-002 | 2026-10-09 | WP-24 | How do parallel agents avoid merge conflicts? | Worktree per lane, path ownership, contracts first, architect owns shared files | Conflicts become a planning error that is visible in the ownership table | Sequential lanes |
| DL-003 | 2026-10-09 | WP-24 | Scope of mutation testing | Diff only, critical modules, kill rate ≥ 80 % | Fast enough for each task; focuses on code that can break users | Set a full run per phase gate |

Decisions DL-001…DL-003 were confirmed by the human owner.

## How to add an entry

1. Use the next free ID.
2. Write one question per entry.
3. Link the sprint record in the WP column when there is one.
