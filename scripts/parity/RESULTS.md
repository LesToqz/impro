# Parity results — 2026-10-23

Two repeated runs cover 110 cases: 100 pixel comparisons, two native graph
structural comparisons, and eight explicitly unsupported live-note cases.
32 pixel comparisons are identical; 68 retain explained differences. There are
no capture errors. Every pixel count, bounding box, mean channel error and source
image SHA-256 is identical between repeats. Graph paint hashes are excluded;
its structural measurements repeat exactly. No image masks or tolerance apply.

The default-font smoke fixture is **0 / 960000 changed pixels in both themes**;
`bash scripts/parity/run.sh --smoke` exits 0 in 4.8 seconds. Two strict smoke
runs also exit 0. The larger matrix deliberately sets explicit interface/editor
font families, exposing an editor gutter font discrepancy absent from smoke.

The main window is 1200×800 logical pixels including decorations. Settings
captures are 900×700 and rename captures 304×155. At 2× these dimensions double.
The physical X backing screen doubles at 2× to avoid clipping; keeping the 1×
physical screen produced black image tails, detected and rejected by the harness.

## Reproduction and repeat checks

Build once: `cargo build --release -p impro --examples`.
Set `IMPRO_REFERENCE_DIR` to the read-only reference checkout. This VM also needs
`IMPRO_PARITY_GI_PATH` pointing to a locally extracted, ABI-matched GTK Cairo
bridge; see README.md. No reference files or system packages were changed.

```sh
bash scripts/parity/run.sh --out .ceo/final
cp .ceo/final/summary.json .ceo/final-first.json
bash scripts/parity/run.sh --out .ceo/final
python3 scripts/parity/check_runs.py .ceo/final-first.json .ceo/final/summary.json

bash scripts/parity/run.sh --out .ceo/scale2-final --scale 2 \
  --scene default --scene explorer-expanded --scene editor-rust \
  --scene terminal-right --scene palette --scene settings-editor \
  --scene git-vertical --scene commit-horizontal --scene notes-live
```

Repeat the second command group while retaining its first summary, then use
`check_runs.py` again. Results: **90/90**, **18/18**, and **2/2 smoke cases** have
identical measurements and hashes. The first 1× set includes a narrow rerun of
`palette-query` after correcting the reference entry caret to the end of its
query; the entire second sweep independently repeats that corrected state.
Earlier diagnostic runs are excluded from this table.

The fixture repository HEAD is `27ecd509bce633caecb855e314ee806a53147202`.
It has two fixed-identity/date commits on branch `work`; `history.json` uses
numeric statistics from the reference's own commit fixture. Both sides open
that same repository, with the resolved full hash for commit pages.

## Fixed in the application crate

- `startup_icons.rs`: startup preparation set a surface on the explorer's custom
  icon cell, clearing the icon name its renderer requires. Skip that preparation
  for the custom renderer. Diagnostic default captures fall from **1140 to 256**
  changed pixels in each theme: 884 missing-icon pixels are repaired. Those early
  captures were 1210×850 before correcting outer-window dimensions; all changed
  pixels were inside the unchanged top-left region. Final default-font smoke is
  exactly equal at the required 1200×800 size.
- `commands.rs`: the window used the switcher's generic footer, including an
  unsupported alternate-open hint. Supply the reference window's three hints.
  Switcher differences fall from **2379 dark / 2378 light to 154 in each theme**.
  Remaining pixels belong to the exposed gutter, not the prompt.

## Difference causes and defects for dispatch

Every non-identical pixel row below names one or more of these causes. The
bounding boxes are exclusive on their right/bottom edges. All counts include
both the legitimate grammar changes and defects; none are subtracted from the
reported totals.

| ID | Class / owner | Expected versus actual; minimal reproduction |
| --- | --- | --- |
| G1 | bug — `impro-editor`, `ViewChrome` gutter | With explicit DejaVu Sans UI and DejaVu Sans Mono editor settings, reference gutter labels use the UI family while Rust draws digits using the TextView family. Default-font smoke matches. `--scene default`: 256 changed pixels, bbox `[341,90,349,199]` at 1×; 783, `[683,180,698,398]` at 2×. Inspect `gutter.rs::rebuild_layouts` against the reference label's effective CSS font and tabular-digit feature. Reproduce using a normal full-matrix scene without `--smoke`; use the same effective label font when drawing digits. |
| T1 | bug — `impro-editor`, notebook tab sizing | `--scene terminal-right`: reference fits all three file tabs; Rust shows scroll arrows and hides the first tab. 1× bbox `[325,50,800,199]`, total 4905 dark / 4906 light: 256 G1 plus 4649 / 4650 tab pixels. 2× bbox `[650,100,1600,398]`, total 18403 dark / 18407 light: 783 G1 plus 17620 / 17624 tab pixels. `panel.rs::size_tabs` measures the panel; the reference measures the notebook. Use the notebook's current allocation, including during a dock resize. |
| R1 | grammar — `impro-editor` highlighter | `--scene editor-rust`: **27 grammar pixels + 256 G1 = 283** in either theme at 1×. At 2×: **70 grammar + 783 G1 = 853**. Grammar bbox `[475,149,478,160]` at 1×; the difference is token styling within the Rust source, outside the gutter. |
| M1 | grammar — `impro-editor` highlighter | `--scene editor-markdown` uses source mode. Link-span styling and the word `with` differ between grammars. **322 dark / 323 light grammar pixels + 160 G1 = 482 / 483**. Full bbox `[341,132,661,201]`. This does not exercise live Markdown rendering. |
| B1 | branding — `impro-settings-ui`, About license paragraph | `--scene settings-about`: reference describes its Python GTK bindings; Rust describes WebKitGTK and gtk-rs. Correct implementation-specific text causes 7134 dark / 7135 light pixels, bbox `[277,245,831,288]`. Product labels and configuration paths are normalized before capture. No defect is dispatched for truthful toolkit attribution. |
| P1 | bug — `impro-graph`, hosted GraphView body allocation | `--scene graph`: reference body width is **379**, native body width **380**, height 720 in both. The right-dock body occupies approximately `[815,75,1195,795]`; inspect its left border and custom-pane allocation with `impro-terminal`. Both control panels are exactly 240 wide. Preserve the reference body inset. This is a structural defect, independently of permitted force-layout changes. |

G1 also explains every remaining difference in expanded/selected/hover/menu
explorer, both search states, Notes section, Python/JSON/TOML source, find,
unsaved, tab hover/close, bottom/split/hover terminal, palette, notice and Git
graph captures. Its count depends on exposed line numbers: Python/find have
505; unsaved JSON 285; prompts obscure some lines and retain 154; other ordinary
three-tab scenes retain 256. Rows at 2× state their actual measured count.
Rename dialogs, all settings pages except About, fullscreen/maximized terminals,
and commit pages have no differing pixels.

## Native graph structure

Both themes have **8 nodes**, **2 undirected connections** (3 directed reference
links, with opposite directions joined into one native connection), identical
control labels in order, identical effective defaults, and an open 240-pixel
control panel. The one-pixel body-width discrepancy is P1. Force positions,
canvas drawing, GTK versus WebView controls and native layout are visual review
artifacts; they receive no pixel-equality score. Both sides are captured in full.

| Legend role | Dark, both sides | Light, both sides |
| --- | --- | --- |
| File | `#a3b5b8` | `#526a60` |
| Tag | `#22a55b` | `#22a35a` |
| Attachment | `#0ea5a4` | `#0d9e9d` |
| Missing target | `#a3b5b8` | `#526a60` |
| Code | `#3b82f6` | `#3b82f6` |

## Harness noise removed before final measurements

The Cairo bridge was absent, so reference custom drawing originally failed.
A local bridge extraction repaired that environment. Later issues were early
WebKit captures before its data paint, find-bar debounce, a query caret at the
wrong end, unnormalized theme names in a combo model, and a clipped 2× display.
Readiness predicates, frame completion, input normalization and screen validation
remove these causes. No final pixel difference is dismissed as harness noise.
Native graph geometry is measured after dock allocation completion.

## Verification

- `G_DEBUG=fatal-criticals GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- cargo test -p impro`: **112 passed, 1 ignored**, zero failures.
- `cargo fmt --check`: passed.
- `cargo clippy -p impro --all-targets -- -D warnings`: passed.
- `bash -n` on all root shell scripts and the parity entry point: passed.
- `python3 scripts/parity/test_harness.py`: five tests passed, including the stdlib PNG fallback, exact RGBA metrics, clipped captures reserved live-note capability entries, and repeat-checker failure detection.
- Required case-insensitive forbidden-text scan over crates, scripts, docs, README and packaging: no matches; branch commit-message scan: no matches.
- Images, logs, comparison JSON and local environment packages remain ignored artifacts. No dependencies, lockfile or reference files changed.

## Scene matrix

Each row's count is identical in both final passes. `skip` means the Rust driver
returned `unsupported` (exit 77); it never substitutes a source-mode screenshot
for the deferred live-note feature. Graph rows compare structure only.

| Scene | Theme | Scale | Changed / total (both runs) | Cause |
| --- | --- | --- | --- | --- |
| `default` | dark | 1× | 256 / 960000 | bug G1 |
| `default` | light | 1× | 256 / 960000 | bug G1 |
| `explorer-expanded` | dark | 1× | 256 / 960000 | bug G1 |
| `explorer-expanded` | light | 1× | 256 / 960000 | bug G1 |
| `explorer-selected` | dark | 1× | 256 / 960000 | bug G1 |
| `explorer-selected` | light | 1× | 256 / 960000 | bug G1 |
| `explorer-hover` | dark | 1× | 256 / 960000 | bug G1 |
| `explorer-hover` | light | 1× | 256 / 960000 | bug G1 |
| `explorer-menu` | dark | 1× | 256 / 960000 | bug G1 |
| `explorer-menu` | light | 1× | 256 / 960000 | bug G1 |
| `explorer-rename` | dark | 1× | 0 / 47120 | identical |
| `explorer-rename` | light | 1× | 0 / 47120 | identical |
| `search-results` | dark | 1× | 256 / 960000 | bug G1 |
| `search-results` | light | 1× | 256 / 960000 | bug G1 |
| `search-empty` | dark | 1× | 256 / 960000 | bug G1 |
| `search-empty` | light | 1× | 256 / 960000 | bug G1 |
| `explorer-notes` | dark | 1× | 256 / 960000 | bug G1 |
| `explorer-notes` | light | 1× | 256 / 960000 | bug G1 |
| `editor-python` | dark | 1× | 505 / 960000 | bug G1 |
| `editor-python` | light | 1× | 505 / 960000 | bug G1 |
| `editor-json` | dark | 1× | 256 / 960000 | bug G1 |
| `editor-json` | light | 1× | 256 / 960000 | bug G1 |
| `editor-rust` | dark | 1× | 283 / 960000 | grammar R1 + bug G1 |
| `editor-rust` | light | 1× | 283 / 960000 | grammar R1 + bug G1 |
| `editor-toml` | dark | 1× | 256 / 960000 | bug G1 |
| `editor-toml` | light | 1× | 256 / 960000 | bug G1 |
| `editor-markdown` | dark | 1× | 482 / 960000 | grammar M1 + bug G1 |
| `editor-markdown` | light | 1× | 483 / 960000 | grammar M1 + bug G1 |
| `editor-selection-find` | dark | 1× | 505 / 960000 | bug G1 |
| `editor-selection-find` | light | 1× | 505 / 960000 | bug G1 |
| `editor-unsaved` | dark | 1× | 285 / 960000 | bug G1 |
| `editor-unsaved` | light | 1× | 285 / 960000 | bug G1 |
| `tab-hover` | dark | 1× | 256 / 960000 | bug G1 |
| `tab-hover` | light | 1× | 256 / 960000 | bug G1 |
| `tab-close` | dark | 1× | 256 / 960000 | bug G1 |
| `tab-close` | light | 1× | 256 / 960000 | bug G1 |
| `terminal-bottom` | dark | 1× | 256 / 960000 | bug G1 |
| `terminal-bottom` | light | 1× | 256 / 960000 | bug G1 |
| `terminal-right` | dark | 1× | 4905 / 960000 | bug G1 + T1 |
| `terminal-right` | light | 1× | 4906 / 960000 | bug G1 + T1 |
| `terminal-split` | dark | 1× | 256 / 960000 | bug G1 |
| `terminal-split` | light | 1× | 256 / 960000 | bug G1 |
| `terminal-fullscreen` | dark | 1× | 0 / 960000 | identical |
| `terminal-fullscreen` | light | 1× | 0 / 960000 | identical |
| `terminal-maximized` | dark | 1× | 0 / 960000 | identical |
| `terminal-maximized` | light | 1× | 0 / 960000 | identical |
| `terminal-hover` | dark | 1× | 256 / 960000 | bug G1 |
| `terminal-hover` | light | 1× | 256 / 960000 | bug G1 |
| `palette` | dark | 1× | 154 / 960000 | bug G1 |
| `palette` | light | 1× | 154 / 960000 | bug G1 |
| `palette-query` | dark | 1× | 154 / 960000 | bug G1 |
| `palette-query` | light | 1× | 154 / 960000 | bug G1 |
| `switcher` | dark | 1× | 154 / 960000 | bug G1 |
| `switcher` | light | 1× | 154 / 960000 | bug G1 |
| `notice` | dark | 1× | 256 / 960000 | bug G1 |
| `notice` | light | 1× | 256 / 960000 | bug G1 |
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
| `git-horizontal` | dark | 1× | 256 / 960000 | bug G1 |
| `git-horizontal` | light | 1× | 256 / 960000 | bug G1 |
| `git-vertical` | dark | 1× | 256 / 960000 | bug G1 |
| `git-vertical` | light | 1× | 256 / 960000 | bug G1 |
| `commit-horizontal` | dark | 1× | 0 / 960000 | identical |
| `commit-horizontal` | light | 1× | 0 / 960000 | identical |
| `commit-vertical` | dark | 1× | 0 / 960000 | identical |
| `commit-vertical` | light | 1× | 0 / 960000 | identical |
| `graph` | dark | 1× | structure only | native-layout (graph); bug P1 |
| `graph` | light | 1× | structure only | native-layout (graph); bug P1 |
| `notes-live` | dark | 1× | skip | unsupported: notes-* stub |
| `notes-live` | light | 1× | skip | unsupported: notes-* stub |
| `notes-table` | dark | 1× | skip | unsupported: notes-* stub |
| `notes-table` | light | 1× | skip | unsupported: notes-* stub |
| `notes-selection` | dark | 1× | skip | unsupported: notes-* stub |
| `notes-selection` | light | 1× | skip | unsupported: notes-* stub |
| `default` | dark | 2× | 783 / 3840000 | bug G1 |
| `default` | light | 2× | 783 / 3840000 | bug G1 |
| `explorer-expanded` | dark | 2× | 783 / 3840000 | bug G1 |
| `explorer-expanded` | light | 2× | 783 / 3840000 | bug G1 |
| `editor-rust` | dark | 2× | 853 / 3840000 | grammar R1 + bug G1 |
| `editor-rust` | light | 2× | 853 / 3840000 | grammar R1 + bug G1 |
| `terminal-right` | dark | 2× | 18403 / 3840000 | bug G1 + T1 |
| `terminal-right` | light | 2× | 18407 / 3840000 | bug G1 + T1 |
| `palette` | dark | 2× | 479 / 3840000 | bug G1 |
| `palette` | light | 2× | 479 / 3840000 | bug G1 |
| `settings-editor` | dark | 2× | 0 / 2520000 | identical |
| `settings-editor` | light | 2× | 0 / 2520000 | identical |
| `git-vertical` | dark | 2× | 783 / 3840000 | bug G1 |
| `git-vertical` | light | 2× | 783 / 3840000 | bug G1 |
| `commit-horizontal` | dark | 2× | 0 / 3840000 | identical |
| `commit-horizontal` | light | 2× | 0 / 3840000 | identical |
| `notes-live` | dark | 2× | skip | unsupported: notes-* stub |
| `notes-live` | light | 2× | skip | unsupported: notes-* stub |
| `default (default-font smoke)` | dark | 1× | 0 / 960000 | identical |
| `default (default-font smoke)` | light | 1× | 0 / 960000 | identical |
