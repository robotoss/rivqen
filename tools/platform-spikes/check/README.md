<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

# Platform spikes evidence check (WP-04)

Offline check of the capability catalog and the stored evidence, against [CONTRACT.md](../CONTRACT.md) sections 5, 7, 8 and 9. No network. No dependencies (Node.js 22 standard library).

```sh
node tools/platform-spikes/check/check.mjs
node tools/platform-spikes/check/check.mjs --root <dir> --catalog <path>
node --test tools/platform-spikes/check/test
```

| Option | Default |
|---|---|
| `--root <dir>` | repository root (found from the script location) |
| `--catalog <path>` | `<root>/tools/platform-spikes/capabilities.json` |

Exit codes: `0` valid (one `ok:` summary line); `1` findings (one `ERROR` line each); `2` bad use or unreadable input.

## What it checks

| Rule | Source |
|---|---|
| Each catalog row has the fields, and the values are allowed (`catalog_fields`) | §7 |
| A conditional result has a condition; an impossible result has a URL source or a run | §7 |
| A capability verdict is the weakest result. It stays `pending` while a version of H-128 has no result (android `37`, `29`; ios `27.0`, `26.5`, `18.5`), or while a result is `not-tested` (`verdict_rules`) | §7, H-128 |
| Every linked run, metrics and recording file exists inside the root (`links_exist`) | §7, §9 |
| Each run line is a JSON object with sorted keys, a known `ev`, the `src` of that event, and an integer `seq`; `run.start` is first and `run.end` last (`run_log_shape`) | §5 |
| A result with runs has at least 3 runs, and their normalized logs are equal (`runs_stable`) | §7, §8 |
| A recording is at most 1,000,000 bytes; the linked recordings total at most 25,000,000 bytes (`recording_caps`) | §9 |

Normalization (§8): `t` and `seq` are removed; `k` is removed from `run.start`; the run id (`run.start.run`) is replaced by `<run>` in every string value; `app.stream.read` events are replaced by the sum of their `bytes`; `app.probe` events with a `metric.` name are removed; the value of a `Date` field in `fields` is blanked.

## Not checked here

Matrix generation (S3). Video content.

## Fixtures

`test/fixtures/valid/` is a small tree with one passing capability. Tests copy it to a temporary directory and change the copy.
