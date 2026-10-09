# Conformance

Conformance means that a server SDK gives the same answer as the reference for every fixture. It is the gate for every server release.

**Status:** <Badge type="info" text="DESIGN" />

## 1. Components

| Component | Role |
|---|---|
| `fixtures/` | The truth (see [Golden fixtures](/engineering/protocol/fixtures)) |
| `server/rust-reference` | Oracle: must pass 100 % of fixtures |
| `server/conformance` | Runner: sends requests, compares responses |
| Server under test | Node.js, Java or PHP example app with the SDK |

## 2. How to run (planned)

1. Start the server under test with the fixture origin pages.
2. Run the conformance runner with the server URL.
3. Read the report: one line per fixture with pass, fail or skip, and a diff for failures.

```bash
# Planned — not available yet
rivqen-conformance --target http://localhost:8080 --profile legacy-canonical
```

## 3. Profiles

A profile is a named set of behaviors. `legacy-canonical` is the default. Other profiles reproduce documented divergences of upstream implementations (see [legacy divergences](/engineering/protocol/legacy-divergences)). A server SDK must pass the canonical profile and may declare extra profiles.

## 4. Pass criteria

| Level | Criterion |
|---|---|
| SDK release | 100 % of canonical-profile fixtures pass, directly and through the proxy lab |
| Profile claim | 100 % of that profile's fixtures pass |
| Regression | No previously passing fixture fails |
