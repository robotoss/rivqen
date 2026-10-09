# Contributing to Rivqen

Thank you for your interest in Rivqen. This file tells you how to contribute. Read the [Engineering handbook](docs/engineering/index.md) before you start a large change.

## 1. Before you start

1. Read the [Code of Conduct](CODE_OF_CONDUCT.md).
2. Read the [clean-room provenance policy](docs/legal/provenance.md). It is mandatory.
3. Find the work package (WP) for your change in the [work package plan](docs/engineering/plan/work-packages.md).
4. For a new feature or a protocol change, open an issue first. Wait for a maintainer to agree.

> [!CAUTION]
> **Clean-room rule.** Do not copy, translate or port source code from Tencent VasSonic or from any project with an incompatible license. Describe behavior in your own words and cite the upstream file and line. A pull request that contains copied code is closed.

## 2. Developer Certificate of Origin (DCO)

Rivqen uses the **Developer Certificate of Origin 1.1**. Every commit must have a `Signed-off-by` line. The line certifies that you have the right to submit the change under the project license.

1. Configure your real name and e-mail in Git.
2. Commit with the `-s` flag: `git commit -s -m "docs: fix typo"`.
3. Check that the commit message ends with `Signed-off-by: Your Name <you@example.com>`.
4. If you forgot the sign-off on the last commit, run `git commit --amend -s --no-edit`.

<details>
<summary>Full text of the Developer Certificate of Origin 1.1</summary>

```text
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.


Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```

</details>

## 3. License of contributions

All contributions are licensed under the [Apache License 2.0](LICENSE) (inbound = outbound, Apache-2.0 section 5). Do not add files under other licenses without maintainer approval and an entry in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## 4. AI-assisted contributions

You can use AI tools. You are responsible for the result.

1. Review every generated line yourself.
2. Do not paste secrets, private code, or user data into AI prompts.
3. Check that generated code is not a copy of third-party code.
4. Follow the [AI agent workflow](docs/engineering/plan/ai-agents.md) for agent-driven tasks.

## 5. Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```text
<type>(<scope>): <summary in imperative, 72 characters or fewer>

<body: what and why>

Refs: WP-04, ADR-005
Signed-off-by: Your Name <you@example.com>
```

| Type | Use for |
|---|---|
| `feat` | A new feature |
| `fix` | A bug fix |
| `docs` | Documentation only |
| `spec` | A change to a normative specification or fixture |
| `test` | Tests only |
| `refactor` | Code change without behavior change |
| `perf` | Performance change |
| `build` / `ci` | Build system or CI |
| `chore` | Maintenance |

Scopes: `core`, `protocol`, `cache`, `transport`, `android`, `ios`, `web`, `react`, `server-node`, `server-java`, `server-php`, `security`, `docs`, `legal`, `plan`.

## 6. Pull requests

1. Make one logical change in each pull request.
2. Link the issue and the work package.
3. Fill in the pull request template.
4. Make sure that all CI checks pass.
5. Get the reviews that the change type needs (see the table).

| Change type | Required reviewers |
|---|---|
| Documentation | 1 maintainer |
| Protocol, fixtures, public API | Protocol owner + 1 platform owner |
| Security-relevant code (bridge, cache, crypto, FFI, `unsafe`) | Security owner + 1 maintainer |
| Release, licensing, brand | Release owner |

## 7. Documentation changes

1. Follow the [documentation style guide](docs/engineering/style-guide.md) (ASD-STE100 rules).
2. Label each statement as FACT, DESIGN or RESEARCH.
3. Run the site locally:

```bash
npm ci
npm run docs:dev
```

4. Build the site before you push:

```bash
npm run docs:build
```

## 8. Report a security problem

Do not open a public issue. Follow [SECURITY.md](SECURITY.md).
