# Matrix generator

Writes two pages from `tools/platform-spikes/capabilities.json`:

- `docs/engineering/platforms/ios-parity-matrix.md`
- `docs/engineering/platforms/android-matrix.md`

```sh
node tools/platform-spikes/matrix/gen.mjs           # write the pages
node tools/platform-spikes/matrix/gen.mjs --check   # exit 1 when a page is stale or missing
node --test "tools/platform-spikes/matrix/test/*.test.mjs"
```

Node.js 22, standard library only, no network. Options for tests: `--catalog <file>`, `--out-dir <dir>`. Exit codes: 0 current, 1 stale page or defective catalog, 2 usage error. The generator checks the catalog with the rules of `check/src/catalog.mjs` first.

## Page shape

- One row per capability, one column per version of H-128, then the capability verdict. Cells hold a status badge.
- The long text (title, condition, note) is in a numbered list under the tables. The `#` column is the list number.
- Links to runs, metrics and recordings are GitHub links (`https://github.com/robotoss/rivqen/blob/main/<path>`), because the files are outside `docs/`.
- The iOS page has one table per item of the spike matrix (`docs/engineering/platforms/ios.md` section 4). `IOS_ITEM_MAP` in `src/render.mjs` gives the items of each capability. An item without a capability shows "not measured". A new iOS capability that is not in the map stops the generator.
- Decision on shared rows: the FFI (`CAP-F`) and URL (`CAP-U`) capabilities are in a short section "Shared rows (FFI and URL)" on both pages, with the same text. Only the note numbers differ. Each result in the notes is one target (host, device, simulator).

## 375 px rule

A table cell may show at most 14 characters (badge and link text count, markup does not). A row may show at most 60 characters in all cells. The generator fails when a row is longer (`MAX_CELL`, `MAX_ROW` in `src/render.mjs`).
