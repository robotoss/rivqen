# Sprint record template

Copy this page to `<WP-ID>-S<n>.md`. Replace each `<…>`. Delete sections that do not apply. The **State** row is one of `Planned` (after the plan), `Running` (from the first lane until the close) and `Closed`. The session-start hook shows every record that is not `Closed`.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 0. Header

| Field | Value |
|---|---|
| WP | `<WP-ID>` — `<title>` |
| Sprint | `S<n>` |
| Integration branch | `wp/<WP-ID>` |
| Base commit | `<sha of main>` |
| State | Planned |
| Goal | `<one sentence>` |

## 1. Kickoff (first sprint only)

### What exists

| Item | Path | State |
|---|---|---|
| `<code, tests, fixtures, docs>` | `<path>` | `<usable / partial / missing>` |

### What is needed

| Acceptance item | Gap | Approach |
|---|---|---|
| `<from the WP page>` | `<what is missing>` | `<how we do it, with the pattern or library>` |

### Risks and questions

| ID | Risk or question | Class | Owner | State |
|---|---|---|---|---|
| `<R-1 / Q-…>` | `<text>` | Architect / Human | `<role>` | Open |

## 2. Plan

### Contracts (architect, before lanes start)

- `<path>` — `<types / traits / fixtures>`

### Tasks

| ID | Title | Tier | Lane | Depends on | State |
|---|---|---|---|---|---|
| T-01 | `<title>` | Junior / Middle / Senior | L1 | — | Planned |

### Ownership

| Lane | Tier | Owns | Reads |
|---|---|---|---|
| L1 | `<tier>` | `<paths>` | `<paths>` |

Shared files (architect only): `<list>`

### Exit criteria

- [ ] All tasks done or moved with a reason
- [ ] All tests green (full repository run)
- [ ] Mutation kill rate ≥ 80 % on critical modules in the sprint diff
- [ ] `/code-review` findings fixed or recorded
- [ ] Docs updated; site builds

## 3. Progress log

Add one line after each integrated lane or decision.

| Date | Event |
|---|---|
| `<YYYY-MM-DD>` | `<L1 integrated: 24 tests, mutation 31/36>` |

## 4. Results

| Check | Command | Result |
|---|---|---|
| All tests | `<command>` | `<n passed>` |
| Lint / format | `<command>` | `<clean>` |
| Mutation (sprint diff) | `<command>` | `<killed/total (%)>`; equivalent: `<n>` |
| Code review | `/code-review high` | `<n findings, n fixed>` |
| Docs build | `npm run docs:build` | `<ok>` |

## 5. Decisions and follow-ups

- Decisions: `<links to decision log / ADR>`
- Follow-ups for the next sprint: `<list>`
