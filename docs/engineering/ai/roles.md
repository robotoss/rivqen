# Roles and routing

This page defines each agent role and the rules that send a task to a role.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 1. Architect

The architect is the main Claude Code session. Use the most capable model that is available for it.

The architect:

1. Runs the WP kickoff and writes the sprint plan.
2. Writes the **contracts** before the lanes start: public types, traits and interfaces, error enums, fixtures, test names.
3. Assigns each task to a tier and gives each lane its own files.
4. Answers questions from agents. Decides non-critical questions and records them. Sends critical questions to the human.
5. Integrates the lanes, runs all checks, runs `/code-review`, and makes the final decision on each sprint.
6. Owns the shared files: workspace manifests, lock files, `docs/.vitepress/config.mts`, `AGENTS.md`, `CLAUDE.md`, `.claude/`, status tables.

The architect does not write large implementation code. When a lane fails, the architect re-plans the task. It does not silently do the task itself.

## 2. Executors

| Tier | Agent file | Model | Typical tasks |
|---|---|---|---|
| Junior | `.claude/agents/junior.md` | `haiku` | Fixtures from a written spec; renames; boilerplate that follows an existing example; link and table fixes; test data; changelog lines |
| Middle | `.claude/agents/middle.md` | `sonnet` | One module from a clear spec: a validator, a header parser, an adapter behind a defined port, a server SDK handler, a demo screen; unit and property tests; mutation check |
| Senior | `.claude/agents/senior.md` | `opus` | FSM, concurrency, FFI, `unsafe`, cache atomicity, streaming, security-sensitive parsing, performance-critical code, unclear or conflicting specs, ADR drafts |

## 3. Specialists

| Role | Agent file | Model | Output |
|---|---|---|---|
| Researcher | `.claude/agents/researcher.md` | `sonnet` | Facts with sources; `NOT FOUND` when there is no evidence. No code. |
| Docs steward | `.claude/agents/docs-steward.md` | `sonnet` | Shared pages, sidebar, status tables, STE style |
| Security reviewer | `.claude/agents/security-reviewer.md` | `opus` | Findings with severity, attack path and fix proposal. Read-only. |

## 4. Routing rules

Use the first row that matches.

| If the task… | Send to |
|---|---|
| Touches `unsafe`, FFI, threads, async cancellation, the session FSM, cache commit, or crypto | Senior |
| Parses untrusted input (HTML, headers, JSON, patches) for the first time | Senior |
| Has a spec gap or two specs disagree | Senior (proposal) → Architect (decision) |
| Is one module, the spec is complete, the pattern exists elsewhere | Middle |
| Is mechanical and fully specified; a wrong result is easy to see | Junior |
| Needs facts from outside the repository | Researcher |
| Changes shared doc pages or the sidebar | Docs steward |
| Changes a security boundary | Security reviewer after the executor |

## 5. Escalation between tiers

1. An executor that finds a gap stops and returns `BLOCKED` with the question. It does not guess.
2. An executor that finds the task larger than its brief returns `RESCOPE` with a proposal.
3. After **two** failed attempts at one tier, the architect moves the task one tier up.
4. A senior result in a critical module gets a second review: the security reviewer or a second senior, never the same agent.

## 6. Domain focus

The tier says how hard a task is. The domain says what knowledge it needs. The brief names the domain, and the agent reads the pages of that domain first.

| Domain | Main pages | Typical output |
|---|---|---|
| Upstream evidence | [Upstream audit](/research/upstream-vassonic), [legacy traces](/engineering/protocol/legacy-traces) | Source map, traces, `upstream:<path>:<line>` references |
| Protocol | [Protocol](/engineering/protocol/) | Specs, schemas, fixtures, compatibility matrix |
| Rust core | [Rust core](/engineering/core/) | Code with property and fuzz tests |
| Android / iOS | [Platforms](/engineering/platforms/android) | SDK code, demo, device tests |
| Web | [Web and React](/engineering/platforms/web-react) | TypeScript packages, E2E tests |
| Server | [Server SDKs](/engineering/server/) | Middleware, conformance results |
| Security | [Security](/engineering/security/) | Findings, test vectors |
| Performance | [Benchmarks](/engineering/quality/benchmarks) | Reproducible reports |

## 7. Rules for every agent

1. Read `AGENTS.md` and the task brief first. Read the rules file for each language you edit.
2. Edit only the files that your brief owns. Request changes to other files in your report.
3. Do not spawn other agents. Only the architect delegates.
4. Do not push, merge, rebase shared branches, or change git history.
5. Write tests with the code. Run the checks in your brief before you report.
6. Update the doc pages in your brief. List any other page that needs a change.
7. Report in the [standard format](/engineering/ai/sprint#_4-task-report).

Agents **must not**:

1. Invent a WebView, `WKWebView` or other platform API that is not documented.
2. Replace evidence with "logical" behavior.
3. Count a task as passed on green unit tests when a device test is required.
4. Put secrets, tokens or private user data into prompts, code, tests or logs.
5. Change legacy fixtures to make new code pass.
6. Turn on an experimental transport for everyone.
7. Copy code from upstream or from sources with an incompatible license (clean-room).

Agents **must**:

1. Write `NOT FOUND` when evidence does not exist.
2. Label statements as FACT, DESIGN or RESEARCH.
3. Cite `upstream:<path>:<line>` for every upstream fact.

## Related

- [AI development](/engineering/ai/)
- [Parallel work](/engineering/ai/parallel)
- [Decisions](/engineering/ai/decisions)
