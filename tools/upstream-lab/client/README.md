# Client lab: conductor

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The conductor is one Node.js process (Node.js 22, standard library only at run time). It records every exchange between the Android harness and a lab server, runs the scripted steps of a scenario, and stores the evidence. An offline check validates the stored evidence. The interfaces are fixed in [`CONTRACT.md`](CONTRACT.md). Decisions: DL-099, DL-100, DL-103 (sprint record `docs/engineering/plan/sprints/WP-01-S1.md`).

This is lab tooling for the evidence track. It is not shipped. It binds `127.0.0.1` only and talks only to the lab servers of `../start.sh` (`127.0.0.1:18081-18083`).

## Files

| Path | Use |
|---|---|
| `run.mjs` | Runner CLI: runs scenarios against an emulator, stores the evidence |
| `check.mjs` | Offline check CLI for a stored evidence tree |
| `scenarios.json` | The scenario catalog (CS-01 … CS-19) |
| `functions.json` | Inputs of the pure-function cases (no expected outputs) |
| `src/proxy.mjs` | Recording proxy on `127.0.0.1:18090` with the fault steps |
| `src/control.mjs`, `src/channel.mjs`, `src/runhandle.mjs` | Control channel on `127.0.0.1:18091`, step hand-over, event intake |
| `src/conductor.mjs` | One run: servers, steps, event log |
| `src/runner.mjs`, `src/adb.mjs`, `src/lab.mjs`, `src/runjson.mjs` | Run selection and storage, adb argument builders, lab case upload, `run.json` |
| `src/check.mjs`, `src/determinism.mjs` | Schema, sequence, determinism and `run.json` checks |
| `src/httpfile.mjs`, `src/eventlog.mjs`, `src/canon.mjs`, `src/steps.mjs` | `.http` format, event log, canonical JSON, step vocabulary and scenario validation |

## Run

Start the lab servers (`sh tools/upstream-lab/start.sh`), boot the emulator, install the harness, then:

```bash
node tools/upstream-lab/client/run.mjs --scenario CS-05 --mode quick --repeat 3 --out evidence/traces/client
node tools/upstream-lab/client/run.mjs --all
node tools/upstream-lab/client/run.mjs --run-json --out evidence/traces/client   # rebuild run.json from the stored tree
node tools/upstream-lab/client/run.mjs --scenario CS-05 --dry-run   # prints the adb commands, runs nothing
```

| Option | Default | Meaning |
|---|---|---|
| `--scenario <id>` or `--all` | required, one of them | Scenario to run |
| `--mode quick\|standard` | the modes of the scenario | One mode only |
| `--repeat <k>` | `repeat` of the scenario | Runs per mode |
| `--out <dir>` | `evidence/traces/client` | Evidence root (it gets `android/` and `run.json`) |
| `--package` | `org.rivqen.lab.harness` | Harness package (`pm clear`, `am force-stop`) |
| `--component` | `.MainActivity` | Activity for the default start command |
| `--start-command` | `shell am start -W -n {package}/{component} --es run {run} --es control {control}` | adb arguments, with `{package}`, `{component}`, `{run}`, `{control}` (use `shell am instrument -w -e run {run} -e control {control} {package}/{component}` for an instrumentation) |
| `--serial`, `--adb` | none, `adb` | adb device and binary |
| `--upstream-sha`, `--emulator-image` | `59936bef`, kept from `run.json` | Written to `run.json` |
| `--step-timeout <ms>` | 60000 | A harness step that is not acknowledged in time fails the run |

For each run the runner does: `adb shell pm clear <package>`, start the proxy and the control channel, post the setup case and run the setup steps, post the scenario case, run the steps, send `end`, write the files, `adb shell am force-stop <package>`. Exit code: 0 all runs ok, 1 a run failed, 2 bad use or environment.

Timeouts: a harness step 60 s (`--step-timeout`), a `wait` step 30 s, the end of the run 5 s, a lab case 10 s, a long poll 30 s, an upstream answer 20 s. A hung harness fails the run; it never hangs the runner.

## Stored evidence

Files follow CONTRACT sections 5 and 8: `android/<scenario>/<mode>/run-<k>.jsonl`, `run-<k>-x<n>.http`, and `run.json`. `run.json` is rebuilt from the tree after every runner call (or with `--run-json`). It copies the meta file of each function runner, `<platform>/functions.jsonl.meta.json` (iOS: macOS version and build, SDK version, compiler, upstream SHA, case count, `limit`, `non_public_calls`), into a `functions` member (`functions.ios`, `functions.android`) when the file exists. Its `summary.unstable` entries keep a reason that a person wrote; a new one gets `runs differ: run N at line L`.

```json
{
  "date": "2026-11-05",
  "tools": { "node": "v22.0.0", "adb": "Android Debug Bridge version 1.0.41", "conductor": "1" },
  "emulator_image": "system-images;android-28;default;arm64-v8a",
  "upstream_sha": "59936bef",
  "scenarios": [ { "id": "CS-05", "modes": { "quick": { "runs": 3 }, "standard": { "runs": 3 } } } ],
  "summary": { "stable": [ { "scenario": "CS-05", "mode": "quick" } ], "unstable": [ { "scenario": "CS-05", "mode": "standard", "reason": "…" } ] }
}
```

## Check

```bash
node tools/upstream-lab/client/check.mjs [<evidence dir>] [--require-functions]
node --test tools/upstream-lab/client/test
```

`check.mjs` exits 0 (valid), 1 (findings) or 2 (bad use). It checks, for every run: JSON Lines with sorted keys and LF; the schema of each event of CONTRACT section 5 (unknown events and members fail); `seq` from 1 without gaps, `t` not going back, `run.start` first and `run.end` last with `ok: true`; one `.http` file per `proxy.request` and no other; no `http(s)` host other than `10.0.2.2`, `127.0.0.1`, `localhost` and `example.com|org|net`. Then, per scenario and mode, the determinism rule of CONTRACT section 6: runs that differ are an error unless `run.json` lists them as unstable with a reason, and runs that are equal are an error if `run.json` lists them as unstable. `functions.jsonl` lines may be `unsupported` (with a reason) and may carry extra members such as `ns_string_encoding`. A meta file must match `run.json` `functions.<platform>` exactly. It also compares the tree with `run.json` and with `scenarios.json` (modes, number of runs). `functions.jsonl` of each platform is checked against `functions.json` (`--require-functions` makes a missing file an error).

## Behavior that CONTRACT leaves open (assumptions)

These are the smallest choices that make the contract executable. T-06 decides whether `CONTRACT.md` states them.

| Topic | Choice |
|---|---|
| Fault steps | `hold`, `cut` and `drop_field` are armed for the next exchange that starts after the step. A `hold` that is released before its exchange arrives lets that exchange pass. `release` with no hold fails the run |
| Proxy buffering | The proxy reads the whole upstream answer (at most 8 MiB) before it delivers it. A request or answer over 8 MiB ends the exchange with `502` |
| Proxy events | `proxy.request` has the request as received. `proxy.response_head` and `proxy.response_end` describe what the proxy **delivered** (hop-by-hop fields removed, `drop_field` applied, `body_len` and `body_sha256` of the delivered wire bytes, not decoded). `complete` is `false` after a `cut` or an error. The `.http` file keeps the full upstream answer and adds `--- delivered ---` after a fault |
| `cut` | Closes the client connection after the first n body bytes with a FIN. A `cut` at or above the body length still closes the connection |
| Hop-by-hop fields | `Connection`, `Keep-Alive`, `Proxy-Connection`, `TE`, `Trailer`, `Transfer-Encoding`, `Upgrade` are not forwarded in either direction. The proxy adds `Host: 127.0.0.1:<port>` upstream if the request has none |
| `step.seq` | Every step object gets `seq` (1, 2, … over setup and steps together). The harness acknowledges with `step.done` whose `step.seq` is the same. The conductor writes `step.start` for every step and `step.done` for its own steps; the harness writes `step.done` for its steps. A failure in the conductor writes `step.done` with `error` |
| Poll | `GET /run/next` gives the next step only after the previous one is acknowledged. Until then it waits (up to the poll limit, then `204`) |
| Control checks | `Host` must be `127.0.0.1`, `localhost` or `10.0.2.2` with the control port. `POST /run/event` needs `application/json`, at most 1 MiB, and `thread` on every event. The harness may not send `proxy.*`, `run.*` or `step.start`. `?run=` is optional on `/run/event` |
| `wait` | Sees events after the latest `step.start` of `create_session` (or from the start). So an event of the setup load does not satisfy a `wait` of the scenario |
| End of run | After the last step the conductor waits up to 5 s for the harness to fetch `{ "do": "end" }`, then writes `run.end`. A harness that never fetches it fails the run |
| `message` and `is_legacy_response` inputs | `fields` is a list of `[name, value]` pairs of response fields |
| Cut size in the catalog | CS-13a and CS-13b cut after 128 body bytes, not 512: the lab pages `a`, `b`, `c` are 296, 296 and 315 bytes. `hold` at `body` in CS-18 uses 128 bytes too |
| CS-19 | `config.webview: true` asks the harness to map the steps to a real web view. The harness decides how |
| `D-30` and `D-39` cases | There is no function for the case of `cache-offline` or for the local-server decision in CONTRACT section 7. D-30 is observed through `message` (`cache-offline` in `fields`), D-39 through `etag`, `template_change` and `diff` |

## Mutation testing

Stryker 10.0.0 (command runner, as `tools/legacy-fixtures`) on `src/`. Run it in this directory, after `npm install`, one run at a time:

```bash
npm install
npx stryker run --mutate "$(node ../../mutation/diff-ranges.mjs --base main --ext .mjs --format stryker)"
```

`reports/`, `.stryker-tmp/` and `node_modules/` are ignored by git.
