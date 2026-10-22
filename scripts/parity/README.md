# Window parity captures

Build once with `cargo build --release -p impro --examples`. Install system
Python GTK3/VTE/WebKit bindings and Pillow (`python3-pil`). The PNG comparator
also supports RGB/RGBA 8-bit PNGs with only Python's standard library.

Run `IMPRO_REFERENCE_DIR=/path/to/reference bash scripts/parity/run.sh --smoke`.
The runner builds nothing. Artifacts default to `.ceo/parity`; never commit them.
The reference tree is read only, including disabled Python bytecode writes.

`--reference DIR`, `--impro-example PATH` (alias `--impro-bin`), `--out DIR`,
`--theme dark|light|both`, `--scale 1|2`, and repeated `--scene NAME` select a run.
The binary must implement the QA example's `scene NAME ROOT PNG THEME` protocol.
Use `--list` for scene names. `--smoke` selects the default three-tab window in
both themes. Run twice into different output directories and compare summary
counts and image hashes to check determinism.

Scenes are paired actions against the real window APIs in `reference.py` and
`crates/impro/examples/parity/scenes.rs`. Add a name to `SCENES` in `matrix.py`,
implement both actions and add their readiness predicates. Unknown scenes fail;
`notes-*` is the reserved live Markdown capability stub (exit 77 means skipped).
Capture waits for pending work, completed frames and a bounded stable-image loop.
No masks or pixel tolerance apply. Differences include all RGBA channels; bboxes
use exclusive right/bottom coordinates. `regions` are connected occupied 32px
tiles, while `bbox` is the exact extent of changed pixels. Mean channel error
includes alpha. PNG output includes source pairs, side-by-side and raw diff.
