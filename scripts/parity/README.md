# Window parity captures

Build once with `cargo build --release -p impro --examples`. Install system
Python GTK3/VTE/WebKit bindings, `python3-gi-cairo`, and Pillow (`python3-pil`). The PNG comparator
also supports RGB/RGBA 8-bit PNGs with only Python's standard library.

Run `IMPRO_REFERENCE_DIR=/path/to/reference bash scripts/parity/run.sh --smoke`.
The runner builds nothing. Artifacts default to `.ceo/parity`; never commit them.
The reference tree is read only, including disabled Python bytecode writes.

`--reference DIR`, `--impro-example PATH` (alias `--impro-bin`), `--out DIR`,
`--theme dark|light|both`, `--scale 1|2`, and repeated `--scene NAME` select a run.
The binary must implement the QA example's `scene NAME ROOT PNG THEME` protocol.
Use `--list` for scene names. `--smoke` selects the default three-tab window in
both themes with the application default font settings. The full matrix uses
explicit DejaVu Sans / DejaVu Sans Mono settings to exercise live font wiring.
Run twice into the same output directory, preserving the first `summary.json`
elsewhere, then compare counts and hashes. Paths appear in settings pages, so
using a different output directory changes those pages.

Scenes are paired actions against the real window APIs in `reference.py` and
`crates/impro/examples/parity/scenes.rs`. Add a name to `SCENES` in `matrix.py`,
implement both actions and add their readiness predicates. Unknown scenes fail;
`notes-*` is the reserved live Markdown capability stub (exit 77 means skipped).
Capture waits for pending work, completed frames and a bounded stable-image loop.
No masks or pixel tolerance apply. Differences include all RGBA channels; bboxes
use exclusive right/bottom coordinates. `regions` are connected occupied 32px
tiles, while `bbox` is the exact extent of changed pixels. Mean channel error
includes alpha. PNG output includes source pairs, side-by-side and raw diff.

On a VM without system install rights, extract the matching `python3-gi-cairo`
package locally and set `IMPRO_PARITY_GI_PATH` to its `gi` directory. The adapter
adds that directory to the installed GI package search path. A missing Cairo
bridge fails before window creation.

The 1× display uses `xvfb-run -a -s '-screen 0 1600x1000x24'
dbus-run-session --`. At 2× the backing screen is 3200×2000, preserving the same
logical screen extent and fitting the entire 2400×1600 capture. Keeping a
1600×1000 physical screen at 2× clips the image; captures with a zero-filled
bottom strip are rejected. Main-window PNGs include decorations and measure
1200×800 logical pixels; GTK content requests are 1190×750 to account for them.
Dialogs retain their application sizes. The same process settings disable GTK
animations and cursor blinking, use a noninteractive startup file and `PS1='$ '`,
and clear window focus except for focused prompts/settings.

The scripted Git repository has fixed author/committer identities and timestamps,
a fixed branch, and two commits. `history.json` uses the numeric statistics from
the reference's `tests/fixtures/gitcommit_reference.json`; no source files are
copied or edited. Both applications open the same working tree and commit hash.

Graph captures are visual review artifacts, not a pixel score. JSON sidecars
compare eight nodes, two undirected connections (three directed source links),
control labels/order, effective defaults, legend RGB values, panel state and
geometry. Opposite directed links count as one connection, matching the native
renderer representation. Native force-layout positions and paint hashes are
excluded from determinism checks. The source graph is paused after data delivery;
the native preview is captured after indexing and dock allocation complete.

`--strict` fails on any changed pixel or capture error. The opt-in test gate uses
this mode with the default-font smoke fixture. Full-matrix nonzero pixels are
kept visible and explained in RESULTS.md, including grammar differences.

Run `PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 scripts/parity/test_harness.py`
for codec, metrics, clipped-capture and reserved scene checks.

Use `python3 scripts/parity/check_runs.py FIRST.json SECOND.json` to verify two
saved summaries, including all source image hashes. Graph rows compare complete
structural metadata instead of force-layout paint hashes; capture errors fail
the repeat check. Keep the scene set and output directory the same between runs.
