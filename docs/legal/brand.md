# Name and brand

**Requirement `BRAND-001`:** the public name must be independent, checked for conflicts, and used consistently before packages are published.

**Status:** <Badge type="info" text="DESIGN" /> "Rivqen" is the chosen working name. Trademark clearance is pending (WP-21).

## 1. The name

| Item | Value |
|---|---|
| Name | **Rivqen** (pronounced "RIV-ken") |
| Origin | Coined word. "Riv" — river, a continuous flow of data. "Qen" — suggests "quick" and "engine". |
| Meaning in other languages | None known; to confirm during clearance |
| Replaces | The internal working name "Sonic Next" from the starting specification, which is **not** used publicly |

## 2. Why not "Sonic …"

1. "Sonic" is used in the upstream project name and by many other brands.
2. "Next" suggests an official next version of the upstream.
3. The upstream license gives no trademark rights and forbids endorsement claims.

"Sonic" stays only as the name of the compatible protocol: **Legacy protocol**.

## 3. Naming in code and packages

| Context | Name |
|---|---|
| Product | Rivqen |
| Protocol compatibility mode | `legacy` (config value), "legacy protocol" (text) |
| New protocol | Rivqen protocol |
| Crates | `rivqen-*` |
| npm scope | `@rivqen/*` |
| Maven group | `dev.rivqen` (to confirm) |
| Swift package | `Rivqen` |
| Composer vendor | `rivqen` |

## 4. Clearance checklist (before the first public release)

- [ ] Trademark search for "RIVQEN" and similar spellings/sounds in software classes (Nice classes 9 and 42) in the target markets: [USPTO](https://www.uspto.gov/trademarks/search), [EUIPO / TMview](https://www.tmdn.org/tmview/), [WIPO Global Brand Database](https://branddb.wipo.int/).
- [ ] Name checks on GitHub, crates.io, npm, Maven Central, Swift Package Index, Packagist.
- [ ] Domain names (for example `rivqen.dev`, `rivqen.io`).
- [ ] Linguistic check for negative meanings in major languages.
- [ ] Result recorded with date and markets.
- [ ] Package names reserved after clearance.

::: warning
An empty search result does not mean the name is free to use. Clearance needs a proper search and, ideally, a lawyer's review.
:::

## 5. Visual identity

| Element | Value |
|---|---|
| Mark | Three waves — a continuous flow of data ("Riv" = river) |
| Tile | Rounded square, teal `#0E7C86` |
| Waves | White, light cyan `#9FE7EC`, white at 55 % opacity |
| Geometry | Wave baselines at 20 / 32 / 44 on a 64 × 64 grid, x from 10 to 54, symmetric humps, so the group is centered on the tile |
| File | `docs/public/logo.svg` (hand-written SVG, no third-party artwork) |

The mark is original work made for Rivqen. A final logo and its trademark check are part of WP-21.
