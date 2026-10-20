# Decisions

The architect decides most questions. The human decides critical questions. Every decision is written down.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 1. Who decides

| Area | Example | Decides | Record |
|---|---|---|---|
| **Public API and ABI** | A new method in the Kotlin SDK; a change to a UniFFI type | **Human** | ADR |
| **Protocol** | A field in the RQP patch; a header; a version; legacy dates | **Human** | ADR |
| **Security and crypto** | An exception to a security rule; a hash or key choice; storage of user data | **Human** | ADR + threat model |
| **Licenses and dependencies** | A dependency with a new license; a break of clean-room rules; brand | **Human** | ADR + third-party list |
| **Scope and schedule** | Move a gate; split or merge a WP; drop an acceptance item; a release | **Human** | Roadmap / WP page |
| Internal design | Module layout, private types, algorithm inside one crate | Architect | Decision log |
| Patterns and libraries | Choice between two allowed crates with the same license class | Architect | Decision log |
| Test design | Which properties to test; mutation exclusions | Architect | Sprint record |
| Docs structure | Page split, wording | Architect | — |

When you are not sure if a question is critical, treat it as critical.

**Delegated: fail-closed fixes from reviews (human decision H-80, 2026-10-19).** The architect decides a fix from a code or security review when all of these hold: it only makes a rule stricter (a document, patch or response that passed before can now fail, never the reverse); the fail case falls back to the normal page or the full document; and it costs at most 2 pages of the markup corpus (0.25 %) or none. The architect writes it in the decision log with the four-part record of DL-028 and the corpus cost, and lists it in the merge request, where the human can reverse it. A relaxation, a rule that costs more, and every API/ABI, license, scope or crypto question stay with the human.

## 2. Non-critical: the architect decides

1. Collect the options. Ask the researcher or a senior for facts if needed.
2. Pick one option. Prefer the option that is easier to reverse.
3. Write an entry in the [decision log](/engineering/plan/decision-log): ID, date, WP, question, options, decision, reason, how to reverse.
4. Tell the lanes that depend on it.

## 3. Critical: ask the human

1. Stop the work that depends on the answer. Continue other lanes.
2. Ask with the `AskUserQuestion` tool. Use the template below.
3. Put the recommended option first and mark it "(Recommended)".
4. Write the answer as an ADR or in the page that owns the topic.
5. If the human does not answer in this session, add the question to [Open questions](/engineering/plan/open-questions) and write the blocked tasks in the sprint record.

Template:

```text
Context:   one or two sentences, with the WP and the page link
Question:  one decision, not several
Options:   2–4, each with its cost and risk
Recommend: the option and why
Blocks:    the tasks that wait for this answer
```

## 4. Agents ask the architect

Subagents never ask the human directly. They return `BLOCKED` with the question. The architect answers, or escalates by the rules above.

## Related

- [ADR register](/engineering/architecture/adr/)
- [Decision log](/engineering/plan/decision-log)
- [Open questions](/engineering/plan/open-questions)
