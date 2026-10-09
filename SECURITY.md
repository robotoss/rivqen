# Security policy

## Project status

Rivqen is in the **documentation phase**. No SDK is released yet. This policy applies now to the documentation and the repository, and it will apply to every released package.

## Report a vulnerability

1. Do **not** open a public issue for a vulnerability.
2. Open a private report: go to **Security → Advisories → Report a vulnerability** in this repository (GitHub private vulnerability reporting).
3. Give the affected component, version or commit, and the steps to reproduce.
4. Give the impact as you see it.

We will:

1. Confirm that we received your report within **5 working days**.
2. Give a first assessment (severity, affected versions) within **15 working days**.
3. Agree a disclosure date with you. The default embargo is **90 days** or less.
4. Credit you in the advisory, if you want.

## Supported versions

| Version | Supported |
|---|---|
| Pre-release (`0.x`) | Latest `0.x` minor only |
| Stable (`>= 1.0`) | Defined at 1.0 release — see `docs/engineering/security/vulnerability-management.md` |

## Scope

In scope: all code, packages, protocol specifications and the documentation site in this repository.

Out of scope: third-party sites that integrate Rivqen, and the upstream Tencent VasSonic project (report those to their owners).

## Our commitments

- We do not ship a fix as "hidden version strings". A vulnerability is closed only by a patch or a documented mitigation.
- We publish a security advisory with affected versions, fixed versions and migration steps.
- See the full process in `docs/engineering/security/vulnerability-management.md`.
