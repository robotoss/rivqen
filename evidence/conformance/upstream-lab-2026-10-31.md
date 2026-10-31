# Upstream lab conformance run, 2026-10-31

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The runner (`server/conformance`) tested the three upstream VasSonic servers by URL (`tools/upstream-lab/conformance.sh`), with the profile fixtures of `fixtures/legacy` (`--profile legacy-node`, `legacy-php`, `legacy-java`). Upstream: `Tencent/VasSonic` at `59936beff656d4b5718ff6444d6c5e001a2c5231`, unmodified, in Docker containers (WP-02 S2, H-112). Raw report: `upstream-lab-2026-10-31.json`.

The profile fixtures expect the safe behavior of DL-069 and DL-082, so a live upstream server fails where it shows an unsafe behavior that the profiles do not reproduce. Every failure falls into one of these classes; no other difference was found.

| Profile | Passed | Failed | U-12: no `vary: template-tag`; CDN fields kept | U-11 (`not_reproduced`) | U-13 (`not_reproduced`) | Tomcat writes `text/html;charset=utf-8` |
|---|---|---|---|---|---|---|
| `legacy-node` | 3 | 36 | 33 | 0 | 3 | — |
| `legacy-php` | 4 | 35 | 29 | 2 | 4 | — |
| `legacy-java` | 0 | 39 | 33 (with the Tomcat spelling on 31 of them) | 2 | 4 | 31 |

The Tomcat spelling is the same media type as the application value `text/html; charset=utf-8` (RFC 9110 §8.3.1); the servlet container writes it, not the upstream filter. The generator compares `content-type` as a media type (`tools/legacy-fixtures/src/trace.mjs`); the runner compares field values byte for byte.

Same day, the same lab reproduced all 93 stored traces S01–S21 (137 of 139 model checks; the two expected Java differences D-02 and D-12) and captured the traces S22–S29 (`evidence/traces/server/`).
