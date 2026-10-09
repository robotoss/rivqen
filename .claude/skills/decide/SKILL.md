---
name: decide
description: Classify a design question in Rivqen as an architect decision or a human decision, then record it in the decision log or escalate it with AskUserQuestion. Use whenever a question blocks a task, an agent returns BLOCKED, or two specs disagree.
argument-hint: <question>
---
# Decision: $ARGUMENTS

Follow `docs/engineering/ai/decisions.md`.

1. Classify the question. It is **critical (human)** if it touches any of: public API or ABI; protocol (RQP fields, headers, versions, legacy dates); security or crypto (exceptions, keys, user-data storage); licenses or dependencies (new license, clean-room, brand); scope or schedule (gates, WP split, acceptance items, releases). When not sure, treat it as critical.
2. Collect 2–4 options with cost, risk and reversibility. Use the `researcher` agent for facts you do not have.
3. **Architect decision:** pick the option that is easiest to reverse when the options are close. Add a row to `docs/engineering/plan/decision-log.md` (next DL ID, date, WP, question, decision, reason, how to reverse). Tell the waiting lanes.
4. **Human decision:** ask with `AskUserQuestion` — context in 1–2 sentences with the page link, one question, options with cost and risk, recommended option first with "(Recommended)". Record the answer as an ADR in `docs/engineering/architecture/adr/index.md` or in the page that owns the topic. If there is no answer in this session, add the question to `docs/engineering/plan/open-questions.md` and mark the blocked tasks in the sprint record. Keep other lanes running.
