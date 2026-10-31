# Sprint records

Each sprint has one record. The record is the shared memory of the work: the plan, the progress and the results. A new session reads it first and continues from it.

**Status:** <Badge type="info" text="DESIGN" />

## Records

| Sprint | WP | Goal | State |
|---|---|---|---|
| [WP-17-S1](/engineering/plan/sprints/WP-17-S1) | WP-17 | RQP markup rules, fixtures, four parser candidates | Closed |
| [WP-17-S2](/engineering/plan/sprints/WP-17-S2) | WP-17 | RQP negotiation, manifest and patch, JSON Schemas, protocol fixtures | Closed |
| [WP-17-S3](/engineering/plan/sprints/WP-17-S3) | WP-17 | `noscript` content model (H-71), manifest after the head start tag (H-72, a wire change), `svg`/`math` in blocks and `noscript` (H-73…H-75), SVG ids in patches (H-76) | Closed |
| [WP-17-S4](/engineering/plan/sprints/WP-17-S4) | WP-17 | Relax the over-tight rules from the WP-17 audit (H-77…H-86) | Closed |
| [WP-17-S5](/engineering/plan/sprints/WP-17-S5) | WP-17 | `template` and `noscript` in blocks, legacy bytes, WebView checks (H-90) | Closed |
| [WP-17-S6](/engineering/plan/sprints/WP-17-S6) | WP-17 | RQP byte overhead (H-101) | Closed |
| [WP-02-S1](/engineering/plan/sprints/WP-02-S1) | WP-02 | Legacy golden suite and HTTP conformance runner | Closed |

## How to add a record

1. Copy the [template](/engineering/plan/sprints/template) to `<WP-ID>-S<n>.md` in this folder.
2. Fill in the kickoff (first sprint only) and the plan.
3. Add a row to the table above.
4. Add the page to the sidebar in `docs/.vitepress/config.mts`.

## Related

- [Sprint workflow](/engineering/ai/sprint)
