# Upstream attribution

Rivqen studies the public Tencent VasSonic project to offer protocol compatibility. This page records the facts about the upstream and how Rivqen refers to it.

## 1. Upstream facts

| Item | Value |
|---|---|
| Project | [Tencent/VasSonic](https://github.com/Tencent/VasSonic) |
| Pinned commit | `59936beff656d4b5718ff6444d6c5e001a2c5231` |
| Commit date | 2019-04-15 (last commit on `master`) |
| Modules | `sonic-android`, `sonic-iOS`, `sonic-java`, `sonic-nodejs`, `sonic-php`, `sonic-react`, `assets` |

<Badge type="tip" text="FACT" /> The upstream `LICENSE` file states:

- VasSonic source code and binaries are licensed under the **BSD 3-Clause License**, except listed third-party components.
- Copyright (C) 2017 THL A29 Limited, a Tencent company.
- Third-party component listed: **Android Source Code 4.4_r1**, Copyright (C) 2005-2015 The Android Open Source Project, under the **Apache License 2.0**.

## 2. What BSD-3-Clause allows and requires

| | |
|---|---|
| Allows | Use, modification and redistribution in source and binary form, including commercial use |
| Requires | Keep the copyright notice, the conditions and the disclaimer in redistributions |
| Forbids | Using the names of the copyright holder or contributors to endorse or promote derived products without permission |
| Does not give | Trademark rights; an explicit patent license |

Source: [OSI — BSD 3-Clause](https://opensource.org/license/bsd-3-clause).

Rivqen does not redistribute upstream code. It still names the upstream for attribution, because the protocol design and the ideas come from it.

## 3. How Rivqen refers to the upstream

| Allowed | Not allowed |
|---|---|
| "compatible with the Tencent VasSonic legacy protocol" | "Tencent Sonic", "Official VasSonic Next", "VasSonic 4" |
| "legacy protocol" as the protocol name | "Sonic" as the Rivqen product name |
| Links to the upstream repository | Upstream logo, QR code, screenshots, diagrams |
| "inspired by" / "behavior studied from" | "successor of", "endorsed by", "by Tencent" |

## 4. Standard statement

Use this text in README files, package descriptions and the site footer:

> This is an independent, community-developed open-source project. It offers a compatibility mode for the Tencent VasSonic legacy protocol. Tencent and VasSonic are referenced solely for compatibility and attribution purposes. This project is not affiliated with, sponsored by, or endorsed by Tencent.
