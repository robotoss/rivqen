# Rivqen — instructions for AI coding agents

This file follows the [AGENTS.md](https://agents.md) convention. Every AI coding tool reads it. Tool-specific additions: `CLAUDE.md` (Claude Code).

Rivqen is an independent, clean-room re-implementation of the ideas of Tencent VasSonic: fast WebView pages with template/data split, caching and block patches. It has its own protocol (RQP). VasSonic compatibility ("legacy mode") is temporary and is removed in 1.5.

- Documentation (source of truth): `docs/` → https://robotoss.github.io/rivqen/
- Plan: `docs/engineering/plan/` (phases P0–P9, gates G0–G8, work packages WP-00…WP-24)
- Current state: `docs/guide/status.md`; running sprint: `docs/engineering/plan/sprints/`

## Your role

- The **main session** is the **architect**. It owns the plan, the contracts, the shared files, the integration and the final check. It delegates code to executors by complexity: junior (small fast model), middle (balanced model), senior (most capable model). Full process: `docs/engineering/ai/`.
- An **executor** (subagent) does only its task brief. It edits only the paths its brief owns, does not start other agents, does not push or change git history, and returns the task report from `docs/engineering/ai/sprint.md` §4.

## Start of every session

1. Read the latest sprint record in `docs/engineering/plan/sprints/` (if a sprint is running) and continue from it.
2. Check `git status` and the current branch. Work happens on `wp/<WP-ID>`, never directly on `main`.

## Hard rules

1. **Clean-room.** Never copy code from VasSonic or from any source with an incompatible license. Describe upstream behavior with `upstream:<path>:<line>` references at commit `59936bef`. See `docs/legal/provenance.md`.
2. **Naming.** The product is Rivqen. The protocol is RQP. Write "Sonic"/"VasSonic" only for attribution and literal legacy wire tokens.
3. **Decisions.** API/ABI, protocol, security/crypto, licenses/dependencies, and scope/schedule are decided by the human (ask the human, recommended option first). Everything else: the architect decides and writes it in `docs/engineering/plan/decision-log.md`. See `docs/engineering/ai/decisions.md`.
4. **Errors.** No ignored errors. Propagate with context. Release builds never crash and fall back to normal HTTPS. No secrets or personal data in logs. See `docs/engineering/architecture/errors-logging.md`.
5. **Tests.** Write tests with the code, in small steps. Run the mutation check on your diff (`docs/engineering/quality/mutation.md`). Do not change legacy fixtures to make code pass. Do not write tests only to raise a number.
6. **Docs as you go.** A task is done only when its pages are updated (`docs/engineering/ai/documentation.md`). Mark statements FACT / DESIGN / RESEARCH / GATE. English; ASD-STE100 for procedures.
7. **Evidence.** Write `NOT FOUND` when you cannot verify a fact. Never invent platform APIs.
8. **Standards.** Follow `docs/engineering/standards/` and the short checklist in `.claude/rules/<language>.md` for each language you edit.

## Git

- Commit with `git commit -s` (DCO). Use the configured user identity.
- Do not add `Co-Authored-By` or other AI attribution trailers. Do not write which AI or model produced a change (model names or IDs, session links) in commits, code comments or docs. The model tiers in `.claude/agents/` and in the role tables are configuration, not attribution.
- Delivery to `main` is only through a merge request (MR) that the human opens and merges. An agent pushes only its own branch: the integration branch `wp/<WP-ID>` or a `chore/<name>` branch. At the end of a WP, the architect pushes `wp/<WP-ID>`, gives the human the MR text and stops. The human deletes the branch after the merge. Lane worktrees are local only.
- Agents never push to `main`.
- Agents never force-push.

## Commands

| Task | Command |
|---|---|
| Docs dev server | `npm run docs:dev` |
| Docs build (fails on dead links) | `npm run docs:build` |
| Server demo tests | `cd examples/server-demo && npm test` |
| Server demo bytes | `cd examples/server-demo && npm start` then `npm run measure` |
| Upstream lab | `UPSTREAM=<VasSonic checkout> sh tools/upstream-lab/start.sh` |

Per-language checks and mutation commands: `docs/engineering/standards/` and `docs/engineering/quality/mutation.md`.

## Workflow

WP kickoff → sprint plan → contracts → parallel lanes → integration → sprint close (all tests, mutation check on the diff, code review, docs) → push `wp/<WP-ID>` → MR to `main`, merged by the human. See `docs/engineering/ai/sprint.md`.
