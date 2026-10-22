# Parity results — 2026-10-23 (finalize)

Two repeated runs cover 98 cases at 1× (44 scenes plus the seven live-note
scenes, dark and light, minus none skipped) and 22 cases at 2×. At 1×, **94 are
pixel-identical**, two (`settings-about`) differ only by branding text, and two
are native-graph structural comparisons whose structure is identical. At 2× all
**22 are pixel-identical**. There are no capture errors and no skipped scenes.
Every pixel count, bounding box, mean channel error and source-image SHA-256 is
identical between repeats (`check_runs.py`: 98/98 and 22/22). No masks or tolerance.
The default-font smoke fixture (`--smoke --strict`) is 0 changed pixels in both
themes and exits 0 on two consecutive runs.

## Before and after

Before (previous results, 1×): 32 identical, 68 differing — gutter font (G1) in
nearly every scene, notebook tab sizing (T1), grammar differences in the Rust and
Markdown scenes, About branding; the live-note scenes were unsupported stubs.

After: the only remaining pixel difference is the About page's license paragraph
(branding B1). All grammar, gutter and tab differences are removed; the live-note
scenes are implemented on both sides and identical.

## Reproduction

Build once: `cargo build --release -p impro --examples`.
Set `IMPRO_REFERENCE_DIR` to the read-only reference checkout. This VM also needs
`IMPRO_PARITY_GI_PATH` pointing to an ABI-matched GTK Cairo bridge (a locally
extracted `python3-gi-cairo` package; see README.md).

```sh
bash scripts/parity/run.sh --out .ceo/final
cp .ceo/final/summary.json .ceo/final-first.json
bash scripts/parity/run.sh --out .ceo/final
python3 scripts/parity/check_runs.py .ceo/final-first.json .ceo/final/summary.json

bash scripts/parity/run.sh --out .ceo/scale2 --scale 2 \
  --scene default --scene explorer-expanded --scene editor-rust \
  --scene terminal-right --scene palette --scene settings-editor \
  --scene git-vertical --scene commit-horizontal --scene notes-live \
  --scene notes-table --scene notes-code
```

Repeat the 2× command with its first summary retained, then use `check_runs.py`.

## Fixes made for this pass

| ID | Cause and fix |
| --- | --- |
| G1 | `impro-editor` gutter: the reference label font is the view family at low priority beneath the screen rule `* {font-family}`, with tabular digits. The gutter now resolves its digit font from a throwaway label carrying the same low-priority rule, so an explicit interface family wins. |
| T1 | `impro-editor` tabs: sizing measured the panel instead of the notebook, and a resize request made inside the notebook's own size-allocate is consumed by that allocation. Widths now come from the notebook and one idle `queue_resize` lays them out. |
| P1 | Not a product defect: the reference adapter measured the hosted graph body immediately after data arrival, before the dock allocation settled (379); both applications are 380 wide at rest. Both adapters now read the geometry after the capture. |
| R1 / M1 | Grammar: `name!` macros are one function token (Rust); Markdown prose is plain text and inline link text is a string, as in the reference grammar. |
| L1 | `impro-markdown`: a caret placed in a table before its grid existed (grids are built when a block nears the viewport) never entered the grid. The renderer now re-runs its caret pass when a grid is built (regression test `test_caret_placed_in_an_unbuilt_table_enters_its_grid_when_it_is_built`). |

## Remaining differences

| ID | Class | Detail |
| --- | --- | --- |
| B1 | branding — `impro-settings-ui`, About | `--scene settings-about`: the reference describes its Python GTK bindings, Rust describes WebKitGTK and gtk-rs. 7134 dark / 7135 light pixels, bbox `[277,245,831,288]`. Truthful toolkit attribution; no defect dispatched. |

Harness notes: the reference recognises notes outside a project by a fixed folder
pair of its own name, so its note popups were inactive under the rebranded folder;
the adapter points that single check at `.impro/Notes`. Live-note scenes apply a
fixed scroll offset that is re-applied until the lazily laid-out view holds it.

## Native graph structure

Both themes have 8 nodes, 2 undirected connections, identical control labels in
order, identical effective defaults, a 240-pixel control panel and a 380×720 body
on both sides. Force positions and canvas drawing are visual review artifacts.

## Verification

See `.ceo/report.md` for the test, fmt, clippy and forbidden-text results of this
branch. `python3 scripts/parity/test_harness.py`: five tests pass.

## Scene matrix

Each row's count is identical in both passes. Graph rows compare structure only.

| Scene | Theme | Scale | Changed / total | Cause |
| --- | --- | --- | --- | --- |
| `default` | dark | 1× | 0 / 960000 | identical |
| `default` | light | 1× | 0 / 960000 | identical |
| `explorer-expanded` | dark | 1× | 0 / 960000 | identical |
| `explorer-expanded` | light | 1× | 0 / 960000 | identical |
| `explorer-selected` | dark | 1× | 0 / 960000 | identical |
| `explorer-selected` | light | 1× | 0 / 960000 | identical |
| `explorer-hover` | dark | 1× | 0 / 960000 | identical |
| `explorer-hover` | light | 1× | 0 / 960000 | identical |
| `explorer-menu` | dark | 1× | 0 / 960000 | identical |
| `explorer-menu` | light | 1× | 0 / 960000 | identical |
| `explorer-rename` | dark | 1× | 0 / 47120 | identical |
| `explorer-rename` | light | 1× | 0 / 47120 | identical |
| `search-results` | dark | 1× | 0 / 960000 | identical |
| `search-results` | light | 1× | 0 / 960000 | identical |
| `search-empty` | dark | 1× | 0 / 960000 | identical |
| `search-empty` | light | 1× | 0 / 960000 | identical |
| `explorer-notes` | dark | 1× | 0 / 960000 | identical |
| `explorer-notes` | light | 1× | 0 / 960000 | identical |
| `editor-python` | dark | 1× | 0 / 960000 | identical |
| `editor-python` | light | 1× | 0 / 960000 | identical |
| `editor-json` | dark | 1× | 0 / 960000 | identical |
| `editor-json` | light | 1× | 0 / 960000 | identical |
| `editor-rust` | dark | 1× | 0 / 960000 | identical |
| `editor-rust` | light | 1× | 0 / 960000 | identical |
| `editor-toml` | dark | 1× | 0 / 960000 | identical |
| `editor-toml` | light | 1× | 0 / 960000 | identical |
| `editor-markdown` | dark | 1× | 0 / 960000 | identical |
| `editor-markdown` | light | 1× | 0 / 960000 | identical |
| `editor-selection-find` | dark | 1× | 0 / 960000 | identical |
| `editor-selection-find` | light | 1× | 0 / 960000 | identical |
| `editor-unsaved` | dark | 1× | 0 / 960000 | identical |
| `editor-unsaved` | light | 1× | 0 / 960000 | identical |
| `tab-hover` | dark | 1× | 0 / 960000 | identical |
| `tab-hover` | light | 1× | 0 / 960000 | identical |
| `tab-close` | dark | 1× | 0 / 960000 | identical |
| `tab-close` | light | 1× | 0 / 960000 | identical |
| `terminal-bottom` | dark | 1× | 0 / 960000 | identical |
| `terminal-bottom` | light | 1× | 0 / 960000 | identical |
| `terminal-right` | dark | 1× | 0 / 960000 | identical |
| `terminal-right` | light | 1× | 0 / 960000 | identical |
| `terminal-split` | dark | 1× | 0 / 960000 | identical |
| `terminal-split` | light | 1× | 0 / 960000 | identical |
| `terminal-fullscreen` | dark | 1× | 0 / 960000 | identical |
| `terminal-fullscreen` | light | 1× | 0 / 960000 | identical |
| `terminal-maximized` | dark | 1× | 0 / 960000 | identical |
| `terminal-maximized` | light | 1× | 0 / 960000 | identical |
| `terminal-hover` | dark | 1× | 0 / 960000 | identical |
| `terminal-hover` | light | 1× | 0 / 960000 | identical |
| `palette` | dark | 1× | 0 / 960000 | identical |
| `palette` | light | 1× | 0 / 960000 | identical |
| `palette-query` | dark | 1× | 0 / 960000 | identical |
| `palette-query` | light | 1× | 0 / 960000 | identical |
| `switcher` | dark | 1× | 0 / 960000 | identical |
| `switcher` | light | 1× | 0 / 960000 | identical |
| `notice` | dark | 1× | 0 / 960000 | identical |
| `notice` | light | 1× | 0 / 960000 | identical |
| `settings-appearance` | dark | 1× | 0 / 630000 | identical |
| `settings-appearance` | light | 1× | 0 / 630000 | identical |
| `settings-editor` | dark | 1× | 0 / 630000 | identical |
| `settings-editor` | light | 1× | 0 / 630000 | identical |
| `settings-files` | dark | 1× | 0 / 630000 | identical |
| `settings-files` | light | 1× | 0 / 630000 | identical |
| `settings-terminal` | dark | 1× | 0 / 630000 | identical |
| `settings-terminal` | light | 1× | 0 / 630000 | identical |
| `settings-graph` | dark | 1× | 0 / 630000 | identical |
| `settings-graph` | light | 1× | 0 / 630000 | identical |
| `settings-gitgraph` | dark | 1× | 0 / 630000 | identical |
| `settings-gitgraph` | light | 1× | 0 / 630000 | identical |
| `settings-notes` | dark | 1× | 0 / 630000 | identical |
| `settings-notes` | light | 1× | 0 / 630000 | identical |
| `settings-hotkeys` | dark | 1× | 0 / 630000 | identical |
| `settings-hotkeys` | light | 1× | 0 / 630000 | identical |
| `settings-about` | dark | 1× | 7134 / 630000 | branding B1 |
| `settings-about` | light | 1× | 7135 / 630000 | branding B1 |
| `git-horizontal` | dark | 1× | 0 / 960000 | identical |
| `git-horizontal` | light | 1× | 0 / 960000 | identical |
| `git-vertical` | dark | 1× | 0 / 960000 | identical |
| `git-vertical` | light | 1× | 0 / 960000 | identical |
| `commit-horizontal` | dark | 1× | 0 / 960000 | identical |
| `commit-horizontal` | light | 1× | 0 / 960000 | identical |
| `commit-vertical` | dark | 1× | 0 / 960000 | identical |
| `commit-vertical` | light | 1× | 0 / 960000 | identical |
| `graph` | dark | 1× | structure only | native-layout (graph); structure identical |
| `graph` | light | 1× | structure only | native-layout (graph); structure identical |
| `notes-live` | dark | 1× | 0 / 960000 | identical |
| `notes-live` | light | 1× | 0 / 960000 | identical |
| `notes-list` | dark | 1× | 0 / 960000 | identical |
| `notes-list` | light | 1× | 0 / 960000 | identical |
| `notes-code` | dark | 1× | 0 / 960000 | identical |
| `notes-code` | light | 1× | 0 / 960000 | identical |
| `notes-table` | dark | 1× | 0 / 960000 | identical |
| `notes-table` | light | 1× | 0 / 960000 | identical |
| `notes-selection` | dark | 1× | 0 / 960000 | identical |
| `notes-selection` | light | 1× | 0 / 960000 | identical |
| `notes-scrolled` | dark | 1× | 0 / 960000 | identical |
| `notes-scrolled` | light | 1× | 0 / 960000 | identical |
| `notes-fullwidth` | dark | 1× | 0 / 960000 | identical |
| `notes-fullwidth` | light | 1× | 0 / 960000 | identical |
| `default` | dark | 2× | 0 / 3840000 | identical |
| `default` | light | 2× | 0 / 3840000 | identical |
| `explorer-expanded` | dark | 2× | 0 / 3840000 | identical |
| `explorer-expanded` | light | 2× | 0 / 3840000 | identical |
| `editor-rust` | dark | 2× | 0 / 3840000 | identical |
| `editor-rust` | light | 2× | 0 / 3840000 | identical |
| `terminal-right` | dark | 2× | 0 / 3840000 | identical |
| `terminal-right` | light | 2× | 0 / 3840000 | identical |
| `palette` | dark | 2× | 0 / 3840000 | identical |
| `palette` | light | 2× | 0 / 3840000 | identical |
| `settings-editor` | dark | 2× | 0 / 2520000 | identical |
| `settings-editor` | light | 2× | 0 / 2520000 | identical |
| `git-vertical` | dark | 2× | 0 / 3840000 | identical |
| `git-vertical` | light | 2× | 0 / 3840000 | identical |
| `commit-horizontal` | dark | 2× | 0 / 3840000 | identical |
| `commit-horizontal` | light | 2× | 0 / 3840000 | identical |
| `notes-live` | dark | 2× | 0 / 3840000 | identical |
| `notes-live` | light | 2× | 0 / 3840000 | identical |
| `notes-table` | dark | 2× | 0 / 3840000 | identical |
| `notes-table` | light | 2× | 0 / 3840000 | identical |
| `notes-code` | dark | 2× | 0 / 3840000 | identical |
| `notes-code` | light | 2× | 0 / 3840000 | identical |
