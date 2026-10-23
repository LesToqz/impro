# graph-lib-bench

Standalone measurements (not a member of the main workspace, own `Cargo.lock`
and `target/`) of which standard Rust libraries could make the impro graph
view render smoothly at full-screen sizes. Findings and the recommendation are
in [`RESULTS.md`](RESULTS.md); raw tables are in `results/`.

## Layout

| path | what |
|---|---|
| `src/fixture.rs` | deterministic 3000-node fixtures: `app` (2600 isolated notes on a phyllotaxis disc ≈60 world units apart as the app's layout settles them, 80 small clusters, one 50-spoke hub, 346 edges), `packed` (the same, 2.8× tighter) and `dense` (3000 nodes, ≈8700 edges) |
| `src/harness.rs` | camera paths (60-frame constant-velocity `pan` at zoom 1, `pan-fit` at the fit-all zoom, 60-frame `zoom-out` 2 → 0.04), 300-label selection, timing table |
| `src/labels.rs` | Pango label bitmaps, cached per (text, size) |
| `src/glscene.rs`, `src/egl.rs` | instanced OpenGL scene and the surfaceless EGL context |
| `src/bin/a_current.rs` | A: the real `impro_graph::view::draw::Painter` |
| `src/bin/b_cairo.rs` | B: plain cairo, one path per style batch |
| `src/bin/c_skia.rs` | C: tiny-skia, plain and `--cached` (layer + integer translate) |
| `src/bin/d_gl.rs` | D: glow, instanced nodes/edges/labels, offscreen on surfaceless EGL |
| `src/bin/d_femtovg.rs` | D2: femtovg on the same EGL context |
| `src/bin/e_wgpu.rs` | E: wgpu offscreen viability and one timing row |
| `src/bin/f_layout.rs` | F: layout engines (impro-graph `Layout`, `fjadra`, `force_graph`, `petgraph` structure) |
| `src/bin/glarea_probe.rs` | `gtk::GLArea` feasibility probe (Xvfb / private Wayland compositor) |
| `bench.sh` | phases that reproduce every table |

Resolutions are logical sizes 1920×1080, 2560×1440, 3840×2160 at device scale
1 and 2 (surfaces up to 7680×4320 device pixels). Every bin takes
`--res WxH@S[,…]`, `--labels` (300 labels, 11–14 px), `--fixture app|dense`,
`--frames N`.

## Reproduce

Heavy: serialise with the shared lock and cap memory.

```sh
cd scripts/graph-lib-bench
flock <home>/projects/impro-wt/.heavy.lock systemd-run --user --scope -q \
  -p MemoryMax=3500M -p MemorySwapMax=0 ./bench.sh build a b c      # CPU candidates
flock … ./bench.sh d d-sw femtovg wgpu layout                       # GPU + layout (no window is opened)
```

No candidate opens a window: the GL bins use a surfaceless EGL context on the
DRM render node. The GLArea probe needs a display: use Xvfb or the private
compositor and never the user's session:

```sh
unset WAYLAND_DISPLAY DISPLAY; export GDK_BACKEND=x11
xvfb-run -a -s '-screen 0 3840x2160x24' dbus-run-session -- target/release/glarea_probe
dbus-run-session -- bash run-wl.sh target/release/glarea_probe   # private Wayland, see RESULTS.md
```
