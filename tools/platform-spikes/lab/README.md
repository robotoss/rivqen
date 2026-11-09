<!--
SPDX-License-Identifier: Apache-2.0
SPDX-FileCopyrightText: The Rivqen Authors
-->

# Platform lab scripts (WP-04, DL-138)

Scripts that make and run the lab of the platform spikes. All variables come from
`tools/platform-spikes/lab-env.sh` (the contract; the scripts do not change it). The lab root is
`/Volumes/BackCat/rivqen-p2-lab` (override: `LAB_HOME`; it must be `/Volumes/<name>/<dir>`).
Every script accepts `--dry-run` as the first argument: it prints the commands that change something
and runs none of them.

## Steps

| Step | Command | Result |
|---|---|---|
| 1. Make the lab | `sh tools/platform-spikes/lab/env.sh` | Lab SDK (emulator, platform-tools, two images), AVDs `rivqen-p2-api37` and `rivqen-p2-api29`, work folders. A second run changes nothing |
| 2. Start the emulator | `sh tools/platform-spikes/lab/emulator.sh start 37` (or `29`) | Cold boot, no window, on `emulator-5602`; waits for the boot |
| 3. Use it | `emulator.sh reverse`, `emulator.sh webview`, `emulator.sh record start\|stop <name>.mp4`, `emulator.sh status` | Port 18443 reversed; WebView package and version; screen recording pulled into `$LAB_OUT` |
| 4. Stop it | `sh tools/platform-spikes/lab/emulator.sh stop` | Stops only the emulator that this script started |
| 5. Simulators | `sim.sh create 27.0`, `boot`, `trust`, `record start\|stop <os> <name>.mp4`, `shutdown`, `delete <os>\|all`, `list` | Device `rivqen-p2-<os>` with `LAB_SIM_DEVICE_TYPE`; runtime identifier found from `simctl list runtimes -j` |
| 6. Clean | `sh tools/platform-spikes/lab/clean.sh` (lists), `... clean.sh --yes` (removes) | Removes the two lab AVDs and the lab simulators only |

`env.sh` needs the main SDK only as a read-only source of `cmdline-tools` and `licenses`, and a JDK
(`JAVA_HOME`). It stops with exit code 1 when the system volume has less than 20 GB free before it
downloads.

## Safety

- Every `adb` call has `-s $LAB_SERIAL` (`emulator-5602`). `emulator.sh start` refuses when any process
  listens on port 5602 or 5603, so a foreign emulator is never reused.
- `sim.sh` looks a device up by the exact name `rivqen-p2-<os>`. `delete` refuses a name without the
  prefix. No command boots or changes another simulator.
- `clean.sh` never deletes `LAB_HOME`. The human deletes top-level folders.
- The scripts never write the main SDK (`~/Library/Android/sdk`).

## Sizes (measured 2026-11-08)

<Badge type="tip" text="FACT" /> The first `env.sh` run took 2 min 19 s.

| Item | Download (zip) | On disk |
|---|---|---|
| Emulator 37.x | 416 MB | 1.2 GB |
| platform-tools r37.0.1 | 16 MB | 37 MB |
| Image API 37.0 `google_apis` `arm64-v8a` | 2,104 MB | 5.3 GB |
| Image API 29 `google_apis` `arm64-v8a` | 1,170 MB | 3.1 GB |
| Lab SDK in total | 3.7 GB | 9.8 GB |
| AVD `rivqen-p2-api37` after the first boot | | about 1 GB (4 GiB data partition, sparse) |
| Whole lab root after the first boot | | about 11 GB |

Cold boot of API 37 with `-no-window` and `swiftshader_indirect`: 55 s.

## Known limits

- `emulator.sh record` uses `screenrecord`; one file is at most 180 s.
- `sim.sh record` starts `simctl io recordVideo`. On the test run (iOS 27.0, a simulator booted with
  `simctl boot` and without Simulator.app), `simctl io` waited for a display and wrote no file. Recording
  needs a check with a display attached before ADR evidence relies on it.

## Cleanup

1. `sh tools/platform-spikes/lab/clean.sh --yes` removes the AVDs and the simulators.
2. The human deletes `/Volumes/BackCat/rivqen-p2-lab` (about 11 GB) and, when wanted, `/tmp/android-<user>/`.


Note (WP-04 S3): with Xcode 27.2 and iOS 27.0, `simctl io <udid> recordVideo` records a booted simulator without a window; Xcode 27.2 has no Simulator.app (it has DeviceHub.app). The S1 note that no file was written is out of date.
