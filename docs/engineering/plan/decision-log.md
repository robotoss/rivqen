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
| DL-007 | 2026-10-09 | [WP-17](/engineering/plan/sprints/WP-17-S1) | How are new dependencies approved? | Any dependency in the **Allowed** license class of [third-party licenses](/legal/third-party) may be added; each one gets a row here or in the sprint record. For JS/TS packages: check for security incidents (supply-chain compromise, malicious versions, CVEs) and pin the latest version that contains the fixes. Other license classes go to the human. Decided by the human. | Fast start without a stop for each permissive dependency; npm supply-chain attacks are the main dependency risk | A new decision by the human |
| DL-008 | 2026-10-09 | [WP-17](/engineering/plan/sprints/WP-17-S1) | How do tokenizer-only and tree-building parsers agree on RQP markup? | Validity is decided on tokens only: a defined token stack (M-05) plus conservative document, context and content rules (M-06…M-24, `markup.md` §2). A tree is never the source of truth. | A tree builder needs the full WHATWG algorithm; a token stack is small and identical in four languages; the restrictions remove the cases where the two disagree | Relax a rule (compatible); adding a rule later is not compatible |
| DL-009 | 2026-10-09 | [WP-17](/engineering/plan/sprints/WP-17-S1) | `noscript` view | Scripting-enabled view (RAWTEXT), plus M-11 so that a markup view gives the same result | The Rivqen WebView always runs scripts | Scripting-disabled view if a server-only consumer appears |
| DL-010 | 2026-10-09 | [WP-17](/engineering/plan/sprints/WP-17-S1) | Attribute values in markup rules | Raw (undecoded) source values for `data-rq-block` and `type` (M-13, M-15, M-16) | No character reference decoder is needed in tokenizer-only parsers (PHP, Rust) | Decode references in all candidates |
| DL-011 | 2026-10-09 | [WP-17](/engineering/plan/sprints/WP-17-S1) | Block contexts inside tables | Blocks only inside `td`, `th`, `caption` (or outside tables); never a direct child of `table`, row group, row or column group (M-19) | Foster parenting moves such elements out of the table | Allow more contexts with fixtures that prove agreement |
| DL-012 | 2026-10-09 | [WP-17](/engineering/plan/sprints/WP-17-S1) | Parser library choice in candidates | A candidate may use any tokenizer with exact source positions from the Allowed license class, or an own tokenizer, as long as it implements `markup.md` §2 on tokens | The rules need tokens with positions; some libraries (for example jsoup) expose only a tree | Fix one library per language |
| DL-013 | 2026-10-09 | [WP-17](/engineering/plan/sprints/WP-17-S1) | Rust candidate (`tools/rqp-markup/rust`): tokenizer and dependencies | Own WHATWG tokenizer subset (DL-012); `lol_html` cannot be driven by the M-04/M-05 foreign-region flag nor re-tokenize `noscript` (M-11); `html5gum` raw attribute spans not documented. Runtime: `sha2` 0.11.0, `base64` 0.23.1 (MIT OR Apache-2.0). Dev: `proptest` 1.11.0, `serde_json` 1.0.151. Tool: `cargo-mutants` 27.1.0 installed with `cargo install --locked` (version of the Rust standard) | Allowed license class (DL-007); exact rules need exact token spans | Switch to a library tokenizer when one exposes spans and state control |

Decisions DL-001…DL-004, DL-006 and DL-007 were confirmed by the human owner.

## How to add an entry

1. Use the next free ID.
2. Write one question per entry.
3. Link the sprint record in the WP column when there is one.
