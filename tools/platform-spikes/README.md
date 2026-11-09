# Platform spikes (WP-04)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Prototypes and measurements that prove what Android and iOS can do with public APIs (phase P2, gate G2). This is evidence code, not the base of the SDK (DL-133).

| Part | Content |
|---|---|
| `CONTRACT.md` | Interfaces between the parts; the architect owns it |
| `lab-env.sh` | Lab environment (source it; no side effects) |
| `capabilities.json` | Capability catalog with verdicts (`supported`, `conditional`, `impossible`) |
| `host/` | Spike host: TLS front and scenarios |
| `lab/` | Lab root, Android emulator, iOS simulators |
| `android/`, `ios/` | Prototype apps and run scripts |
| `ffi-bench/` | FFI benchmark: UniFFI against C ABI |
| `check/` | Offline check of the catalog and the evidence |

Sprint record: [WP-04 S1](../../docs/engineering/plan/sprints/WP-04-S1.md).
