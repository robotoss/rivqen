---
paths:
  - "**/test/**"
  - "**/tests/**"
  - "**/*Test.*"
  - "**/*Tests.*"
  - "**/*.test.*"
  - "**/*.spec.*"
---
# Test checklist

Source: `docs/engineering/quality/testing.md` and `docs/engineering/quality/mutation.md`.

- One test per behavior; the name says the behavior.
- Deterministic: fake clock and transport, fixed seeds, no sleeps, no real network in unit tests.
- No tests of trivial getters, generated code or third-party code; no large snapshots.
- Never change a legacy fixture to make code pass. Never skip or disable a failing test to get green.
- Mutation check on the diff; survivors are fixed, equivalent (with reason) or accepted (with reason). Do not add tests only to kill unimportant mutants.
