# Licensing

**Decision:** All original Rivqen code and documentation are licensed under the **Apache License, Version 2.0**.

## 1. Why Apache-2.0

| Reason | Detail |
|---|---|
| Explicit patent grant | Section 3 grants a patent license from each contributor, and ends it for anyone who sues over patents in the work. |
| Commercial use | Allowed, including in closed-source apps. |
| NOTICE mechanism | Clear rules for attribution through the `NOTICE` file. |
| Ecosystem fit | Compatible with Android (AOSP is Apache-2.0), common in Rust, Java and JS ecosystems. |
| Compatibility with upstream | Apache-2.0 code can coexist with BSD-3-Clause material if any is ever included (it keeps its own notices). |

<Badge type="tip" text="FACT" /> Source: [OSI — Apache License 2.0](https://opensource.org/license/apache-2-0), [Apache License text](https://www.apache.org/licenses/LICENSE-2.0).

## 2. Files

| File | Content |
|---|---|
| `LICENSE` | Full Apache-2.0 text |
| `NOTICE` | Project notice and the independence statement |
| `THIRD_PARTY_NOTICES.md` | Distributed third-party components and upstream attribution |
| `CODE_OF_CONDUCT.md` | Contributor Covenant 2.1 (CC BY 4.0) |

## 3. Per-file license information

From WP-20, the repository follows the [REUSE](https://reuse.software/) specification:

1. Each source file has an SPDX header: `SPDX-License-Identifier: Apache-2.0` and `SPDX-FileCopyrightText: The Rivqen Authors`.
2. Files that cannot have headers are covered by `REUSE.toml`.
3. CI runs `reuse lint`.

## 4. Inbound contributions

Contributions are accepted under Apache-2.0 (section 5, "inbound = outbound") with a DCO sign-off. There is no CLA. A license change of existing code would need agreement of all contributors.

## 5. Documentation

Documentation is Apache-2.0 like the code. Exception: `CODE_OF_CONDUCT.md` keeps its CC BY 4.0 license.
