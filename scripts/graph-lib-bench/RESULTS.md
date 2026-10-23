# Graph view rendering: measurements and recommendation

Written 2026-10-24 (IST) from the runs of 2026-10-23 (IST) in `results/`
(`results/TABLES.md` has every table; this file copies the ones that decide).
Reproduce with the commands in `README.md`. Every number is a release build,
60 frames per scenario, in milliseconds per frame unless stated.

## 1. Headline

**The CPU renderer is not slow. The way a full-screen cairo surface is
presented on Wayland is.**

1. Offscreen, the real `Painter` (candidate A) is fast and beats every other
   CPU candidate: p50 **1.2 ms @1920×1080**, **3.2 ms @2560×1440**,
   **9.3 ms @3840×2160**, **31 ms @4K×2** (7680×4320 device pixels), fixture
   `app`, pan, no labels. tiny-skia, plain cairo and femtovg are slower
   (tables 3.1, 3.3).
2. OpenGL on the VM's real GPU (SVGA3D, GL 4.3 core, Mesa 26.0.8) is **not
   faster than the Painter offscreen at 1080p–1440p** on the app-like fixture
   (2.4 / 4.1 ms against 1.2 / 3.2 ms), and about equal at 4K (8.0 against
   9.3 ms). It wins clearly only on the dense 8.7k-edge fixture (2.5 ms against
   16.5 ms p50 at 1080p, section 3.2).
3. **On a real Wayland compositor the on-screen frame interval stutters with a
   `GtkDrawingArea`, even when almost no rendering is done.** In the private
   headless GNOME Shell (Wayland) at 2560×1440 the current Painter gives a
   frame-clock interval of p95 66 ms / p99 67 ms, and a *pure blit* of an
   already rendered surface (no drawing at all) gives p95 34 ms / p99 104 ms.
   A `GtkGLArea` with the instanced GL scene gives p50 16.7 ms and p95 16.7 ms
   at 1080p and 1440p; at 4K p95 is 33 ms (section 3.3). So the lag the user
   sees has a cause that faster rasterising cannot remove: the blit probe
   already misses frames.
4. The GL path costs the main thread ≈0.1 ms CPU per frame (submit only; the
   GPU time is table 3.2). The CPU path costs 5–23 ms of main-thread time per
   frame at 1080p–4K (handler p50 of the `current` probe).

**Recommendation: option (3), a GL path in a `GtkGLArea` with the current
cairo `DrawingArea` kept as fallback and as the reference renderer.** Raw GL
through `glow` and our own shaders; no vector library. Plan, risks, effort
(≈36 worker-hours) and acceptance numbers are in sections 7–10.

What this does **not** show is in section 11, together with a single command
to compare both paths on your own display.

## 2. Environment

| item | value |
|---|---|
| CPU | AMD Ryzen 7 8845HS, 12 threads visible to the VM |
| RAM | 10 GB (benchmarks capped to 3.5 GB per process) |
| Kernel / session | Linux 7.0.0-38, VMware, GNOME Wayland (user); private headless GNOME Shell (`gnome-shell --headless --wayland`, virtual monitor 3840×2160) for the on-screen runs |
| GL, real GPU | `GL_RENDERER` = `SVGA3D; build: RELEASE;  LLVM;`, `GL_VERSION` = `4.3 (Core Profile) Mesa 26.0.8-1ubuntu0.3`, GLSL 4.30, vmwgfx / render node `/dev/dri/renderD128` |
| GL, forced software | llvmpipe (LLVM 21.1.8, 256 bits), GL 4.5 core under Xvfb/GLX |
| wgpu | Vulkan backend = lavapipe (CPU); GL backend = the same SVGA3D, `4.3 (Core Profile)` |
| Toolchain | rustc 1.99.1; workspace MSRV 1.88 |
| gtk-rs | 0.18 (GTK 3.24, `v3_24`), cairo-rs 0.18.5 |

Fixtures (own generator, `src/fixture.rs`, deterministic): `app` is 3000 nodes,
346 edges (2600 isolated notes on a phyllotaxis disc, 80 small clusters, one
50-spoke hub), `packed` is the same 2.8× tighter, `dense` is 3000 nodes and
8653 edges. Scenarios: `pan` (60 frames, constant velocity at zoom 1),
`pan-fit` (same at the fit-all zoom), `zoom-out` (60 frames, zoom 2 → 0.04).
The cold frame is the first frame including caches and shader compile.

## 3. Results

### 3.1 CPU candidates, offscreen (fixture `app`, no labels, scenario `pan`)

A = the real `impro_graph::view::draw::Painter` (this branch's commit),
A′ = the same with `Painter::motion = true`, B = plain cairo (one path per
style batch), C = tiny-skia 0.12, C′ = tiny-skia with a cached layer and an
integer translate.

| size | candidate | cold | p50 | p95 | p99 | max |
|---|---|---:|---:|---:|---:|---:|
| 1920×1080 | A | 9.54 | **1.16** | 1.57 | 1.71 | 1.97 |
| | A′ | 12.98 | 0.53 | 0.90 | 0.94 | 1.02 |
| | B | 17.44 | 12.18 | 15.92 | 17.30 | 17.32 |
| | C | 8.11 | 3.24 | 3.78 | 3.87 | 3.98 |
| | C′ | 35.31 | 7.43 | 8.46 | 20.58 | 20.73 |
| 2560×1440 | A | 13.70 | **3.22** | 3.58 | 3.65 | 4.03 |
| | A′ | 7.77 | 1.03 | 1.52 | 3.35 | 3.45 |
| | B | 20.96 | 13.05 | 16.42 | 16.79 | 17.11 |
| | C | 12.90 | 6.50 | 9.78 | 10.37 | 10.68 |
| | C′ | 49.12 | 13.17 | 14.19 | 29.61 | 30.07 |
| 3840×2160 | A | 34.15 | **9.28** | 9.73 | 9.80 | 9.92 |
| | A′ | 20.75 | 3.08 | 3.59 | 3.65 | 3.79 |
| | B | 34.03 | 21.54 | 24.49 | 24.76 | 25.55 |
| | C | 27.63 | 13.55 | 15.66 | 15.97 | 16.38 |
| | C′ | 95.19 | 29.25 | 31.37 | 51.32 | 52.11 |
| 4K at scale 2 (7680×4320) | A | 126.57 | **31.44** | 32.31 | 32.73 | 32.81 |
| | A′ | 70.35 | 14.62 | 15.55 | 15.58 | 15.66 |
| | B | 81.68 | 36.88 | 41.99 | 49.69 | 51.11 |
| | C | 134.75 | 32.89 | 34.43 | 34.56 | 34.82 |
| | C′ | 405.28 | 118.28 | 120.82 | 167.42 | 168.75 |

Reading: A beats B by 1.2× (4K at scale 2) to 10× (1080p) and C by 1.05× to
2.8× on `app`, the gap closing as the surface grows; the cached-layer
variant C′ is worse than not caching (it re-blits a full-size layer). On the
`dense` fixture A is the weak one: p50 16.5 ms, p95 49.9 ms at 1080p, and at 4K
scale 2 zoom-out p50 82 ms, p95 196 ms, max 411 ms (there B is worse, p95
810 ms, and C has a lower p50, 56 ms, but a worse p95, 305 ms).

Memory: Painter RSS 25 MB at 1080p; the largest surface (7680×4320 ARGB) is
133 MB.

### 3.2 GL offscreen, hardware against llvmpipe (fixture `app`, no labels, `pan`)

Draw time with the GPU forced to finish (`glFinish`), and draw plus
`glReadPixels` into a cairo surface (the cost if GL were used offscreen and
blitted by cairo, which is what GL is **not** recommended for).

| size | variant | cold | p50 | p95 | p99 | max |
|---|---|---:|---:|---:|---:|---:|
| 1920×1080 | SVGA3D draw | 6.80 | 2.36 | 2.51 | 2.54 | 2.67 |
| | SVGA3D draw + readback | 17.29 | 4.06 | 4.35 | 4.55 | 4.61 |
| | llvmpipe draw | 35.19 | 1.68 | 1.97 | 2.12 | 2.20 |
| | llvmpipe draw + readback | 15.34 | 2.60 | 3.00 | 3.17 | 3.21 |
| 2560×1440 | SVGA3D draw | 8.81 | 4.06 | 4.20 | 4.25 | 4.50 |
| | SVGA3D draw + readback | 21.34 | 6.23 | 6.62 | 6.67 | 7.03 |
| | llvmpipe draw | 9.82 | 2.18 | 2.43 | 2.49 | 2.51 |
| | llvmpipe draw + readback | 17.67 | 4.03 | 4.45 | 4.47 | 4.52 |
| 3840×2160 | SVGA3D draw | 32.42 | 7.99 | 8.41 | 8.47 | 8.64 |
| | SVGA3D draw + readback | 54.92 | 12.33 | 13.09 | 13.22 | 13.25 |
| | llvmpipe draw | 10.14 | 4.31 | 4.59 | 4.71 | 4.89 |
| | llvmpipe draw + readback | 28.18 | 8.17 | 8.56 | 8.75 | 9.13 |
| 4K at scale 2 | SVGA3D draw | 55.26 | 28.60 | 29.62 | 31.01 | 31.62 |
| | SVGA3D draw + readback | 254.30 | 44.50 | 46.21 | 48.29 | 51.34 |
| | llvmpipe draw | 19.11 | 12.82 | 13.24 | 13.49 | 13.58 |
| | llvmpipe draw + readback | 84.00 | 27.44 | 28.22 | 28.34 | 28.66 |

Reading: the hardware GPU of this VM is **not faster than the CPU
rasteriser**, neither than the Painter nor than llvmpipe (which runs the same
shaders on the 12 vCPUs). Draw time grows about linearly with the pixel count
(2.4 → 4.1 → 8.0 → 28.6 ms), so it is bound by fill/clear of the framebuffer
on SVGA3D, not by the 3000 + 346 instances. This is why GL is only interesting
as a *presentation* path (section 3.3), not as a raw speed-up.

Dense fixture, SVGA3D draw only: 2.52 ms p50 at 1080p (A: 16.5 ms) and 28.7 ms
at 4K scale 2 zoom-out (A: 82 ms p50, 196 ms p95). Here GL is the better
rasteriser. `packed` pan 4K scale 2: A 48.1 ms (p95 75), C 41.2, GL+readback
44.5.

Other GL candidates (same fixture and sizes, partial matrix, not like-for-like):

| candidate | 1080p@1 pan p50 / p95 | 4K@1 pan p50 / p95 | 4K@2 pan p50 / p95 | note |
|---|---|---|---|---|
| D2 femtovg 0.27 on SVGA3D, 300 labels, GPU finish | 3.39 / 4.07 | 6.36 / 7.11 | 9.85 / 11.27 | per-frame path tessellation; faster than our raw GL at 4K×2 (28.6), which I did not investigate (inference: our full-size clear/blend into a renderbuffer is not minimal) |
| E wgpu 30.0.1, Vulkan = lavapipe (CPU) | 1.97 / 2.42 | 4.59 / 5.29 | 14.09 / 14.85 | |
| E wgpu 30.0.1, GL backend = SVGA3D | 0.58 / 0.94 | 0.73 / 0.89 | 0.95 / 1.15 | submit time only, no GPU completion: **not comparable** with the other rows |

wgpu is **not suitable for a GTK 3 widget**: presenting needs a native surface
(window or Wayland surface handle); a `GtkGLArea` or `DrawingArea` gives none,
and rendering offscreen then copying to cairo brings back the readback cost of
table 3.2. It also costs 116 crates and ~63 s of rustc time (section 5).

### 3.3 On-screen frame intervals (GTK3 `GtkWindow` in a private compositor)

Probe `glarea_probe` (`PROBE_MODE`): `current` draws the real `Painter` into a
`DrawingArea` every frame with a moving camera; `blit` repaints one already
rendered cairo surface (no rasterising at all); `gl` draws the instanced scene
into a `GtkGLArea`; each 120 frames, fixture `app`, no labels,
`GdkFrameClock` interval between frames (a frame is "on time" at 16.7 ms).
Private headless GNOME Shell (Wayland), window = size shown.

| size | mode | interval p50 | p95 | p99 | max | handler p50 / p95 |
|---|---|---:|---:|---:|---:|---:|
| 1920×1080 | current | 16.75 | 33.27 | 49.83 | 50.00 | 5.56 / 23.75 |
| | current, motion LOD | 16.68 | 31.79 | 54.08 | 84.75 | 3.05 / 10.88 |
| | blit only | 16.66 | 33.09 | 83.31 | 83.33 | 1.03 / 3.14 |
| | **GLArea** | 16.67 | **16.67** | 50.00 | 83.33 | 0.07 / 0.20 |
| 2560×1440 | current | 18.71 | 66.11 | 66.67 | 83.98 | 8.46 / 50.75 |
| | current, motion LOD | 16.92 | 53.48 | 118.36 | 133.21 | 5.14 / 25.55 |
| | blit only | 16.65 | 33.60 | 104.14 | 150.00 | 2.20 / 5.64 |
| | **GLArea** | 16.67 | **16.67** | 66.67 | 66.67 | 0.08 / 0.18 |
| 3840×2160 | current | 48.95 | 70.44 | 150.00 | 150.05 | 22.74 / 47.27 |
| | current, motion LOD | 33.08 | 63.81 | 116.67 | 266.96 | 10.62 / 20.12 |
| | blit only | 30.80 | 66.31 | 167.70 | 316.67 | 4.10 / 8.28 |
| | **GLArea** | 16.67 | 33.33 | 66.67 | 83.33 | 0.09 / 0.18 |

Earlier 1600×900 runs in the same compositor (`probe-wl-*.md`): blit p95
17.3 ms (p99 66.7, max 247), cairo-vector p95 18.0 (p99 66.7, max 184), current
Painter p95 30.8, GLArea p95 16.7 (p99 50, max 100–183). Labels on the GLArea
(300 labels, 1600×900): handler p95 3.6 ms, interval p95 16.7 ms.

Same probe under Xvfb (X11, GLX, llvmpipe, `probe-x-*.md`): 1080p `current`
p95 33 ms, `blit` p95 16.7, `GLArea` p95 16.7; 4K `current` p95 66.7, `blit`
p95 50, `GLArea` p95 33.3.

The proof of concept (`poc/glarea_poc.rs`, scripted pan + zoom-out, 1920×1080,
private Wayland): handler p50 0.07 ms, interval p50 16.67 / p95 16.67 / p99
50.0 / max 83.3 ms (`poc-wl4k-auto.md`); under Xvfb at 1280×720 p99 16.67,
max 66.7.

What the numbers say, and what is inference:

* **Measured:** with a `DrawingArea`, frame intervals degrade with window area
  (blit p95: 17 ms at 900p, 33 ms at 1080p and 1440p, 66 ms at 4K) while the handler
  itself stays at 1–8 ms; the pure blit already misses frames at 1440p and 4K
  where the GL area does not (the GL p95 is 16.7 ms at 1080p and 1440p).
* **Measured:** a GL area's handler is ≈0.1 ms and the interval is 16.7 ms at
  p50 in every row; its tail is not clean (p99 50–67 ms, max 66–83 ms, and at
  4K p95 33 ms). One-off 50–180 ms gaps also appear in all modes, so I treat
  p99/max as partly compositor noise of a headless VM.
* **Inference (not measured here):** the per-frame cost behind the blit rows
  is the copy of the cairo surface into a Wayland shared-memory buffer that GTK
  3's Wayland backend performs after each `draw`, and the compositor's upload
  of that buffer; a GL area instead hands the compositor a GPU texture, so
  nothing proportional to the window area crosses the CPU. I did not profile
  GTK or mutter to confirm this; it fits "cost grows with area, independent of
  the drawing work" and nothing else I measured contradicts it.

### 3.4 Labels (300 labels of 11–14 px)

Fixture `app`, scenario `pan` (≈300 labels visible), offscreen:

| size | A (own code) | B cairo + cached bitmaps | C tiny-skia + cached pixmaps | D GL atlas, draw (GPU finish) | D GL atlas + readback |
|---|---|---|---|---|---|
| 1920×1080 | 2.69 / 3.57 | 6.27 / 10.13 | 5.99 / 6.82 | 2.71 / 3.11 | 4.29 / 4.77 |
| 2560×1440 | 6.65 / 7.90 | 8.94 / 11.02 | 9.34 / 11.00 | not run | 6.58 / 7.83 |
| 3840×2160 | 15.04 / 15.92 | 21.08 / 34.69 | 17.01 / 18.16 | 8.38 / 9.23 | 12.68 / 13.61 |

(p50 / p95 ms.) Adding 300 labels costs the Painter +1.5 ms (1080p) to
+5.8 ms (4K); in GL they cost +0.35 ms (1080p, 2.36 → 2.71) and +0.4 ms
(4K, 7.99 → 8.38). Rendering a Pango label bitmap costs about 0.02–0.06 ms each
(466 renders = 26 ms, 1690 = 47 ms at 1080p), so a cold or fast-zoom frame
that needs hundreds of new bitmaps costs tens of milliseconds in every
candidate: the existing render budget (`LABEL_RENDER_BUDGET` = 120 per frame)
must stay.

### 3.5 Layout engines (secondary; the layout is not the complaint)

`results/layout-app.md`, `layout-dense.md`; 3000 nodes; petgraph 0.8.3 builds
the `UnGraph` in 0.02 ms (`app`) / 0.14 ms (`dense`).

| engine | ms/tick p50 | ticks / settle | quality on `app` | deterministic |
|---|---:|---|---|---|
| impro-graph `Layout` (own) | 1.49 (dense 1.68) | 241 / 364 ms | min gap 44.9, 0 overlaps, edge length cv 0.36 | yes |
| fjadra 0.2.1 (link+charge+center+collide) | 8.50 (dense 10.4) | 300 / 2588 ms | min gap −0.1, 90 overlapping pairs | yes |
| force_graph 0.4.0 (O(n²) repulsion) | 32.9 (dense 32.7) | 30 / 992 ms | min gap −15.6, 2 overlaps, 98 % within r=12730 (not settled) | yes |

The app's own layout is 6–20× cheaper per tick and gives better spacing on
the same fixture: **keep it**. A force-layout crate would not make the view
smoother.

## 4. Why no library renders better here

| candidate | verdict |
|---|---|
| B plain cairo vector | 1.2–10× slower than the Painter; not a candidate |
| C tiny-skia | SIMD CPU, 1.05–2.8× slower than the Painter on `app`; lower p50 but worse p95 than the Painter on `dense` at 4K×2 |
| D glow + own shaders | same raster speed as llvmpipe/Painter on this VM, but a ~0.1 ms main-thread cost and a texture handoff to the compositor: **the only candidate that fixes the on-screen interval** |
| D2 femtovg | works (3.4–9.9 ms), 47 crates, +17 s build, tessellates every frame, own text engine (labels differ from Pango); gives no seam to the existing label cache |
| E wgpu | needs a native surface GTK 3 cannot give; 116 crates; not suitable |
| F petgraph / fjadra / force_graph | slower and worse than the own layout; not adopted |

## 5. Licence, maintenance, build cost

Versions are the ones pinned in this standalone `Cargo.toml`. Dependency count
is the resolver's normal-dependency closure over all targets (it overstates
what a Linux build compiles). Build time is rustc seconds summed over the
candidate's own crates and not-shared dependencies, `jobs=3`, from one clean
`--timings` build (`timing.sh`). I did not re-check crate release dates or
issue activity in this pass: confirm both at T7 start.

| crate | version | licence | MSRV | closure deps | rustc time | note |
|---|---|---|---|---:|---:|---|
| glow | 0.18.0 | MIT OR Apache-2.0 OR Zlib | – | 20 | 2.8 s (with khronos-egl, libloading) | thin GL bindings, loader supplied by us |
| khronos-egl | 6.0.0 | MIT / Apache-2.0 | – | 4 | (above) | only for offscreen benches, not needed in the app |
| libloading | 0.8.9 | ISC | 1.71 | – | (above) | the app needs it only to look up libepoxy symbols |
| tiny-skia | 0.12.0 | BSD-3-Clause | – | 22 | 6.2 s | |
| femtovg | 0.27.0 | MIT OR Apache-2.0 | 1.88 | 47 | 17.0 s | |
| wgpu | 30.0.1 | MIT OR Apache-2.0 | 1.87 | 116 | 63.1 s | |
| petgraph | 0.8.3 | MIT OR Apache-2.0 | 1.64 | 14 | 5.3 s (with fjadra, force_graph) | |
| fjadra | 0.2.1 | MIT OR Apache-2.0 | 1.80 | 0 | (above) | |
| force_graph | 0.4.0 | MIT | – | 15 | (above) | |
| cairo-rs | 0.18.5 | MIT | 1.70 | 45 | already in the workspace | |

Whole standalone project: 286 compile units, 307 s of summed rustc time.
Benchmark binaries (release, unstripped, include the shared bench code):
`d_gl` 0.71 MB, `c_skia` 0.91 MB, `b_cairo` 0.58 MB, `d_femtovg` 2.1 MB,
`e_wgpu` 7.3 MB. Recommended addition to the app: `glow` (+ `libloading`),
≈3 s of build time, a few hundred KB of code.

## 6. GL viability inside GTK 3.24 / gtk-rs 0.18

* **Context.** `gtk::GLArea` realises under both the private Wayland
  compositor (SVGA3D, GL 4.3 core, not legacy, not ES) and Xvfb/GLX
  (llvmpipe, GL 4.5 core). `set_required_version(3, 3)` works; GTK 3 needs ≥ 3.2
  core and this VM has 4.3. The shaders are `#version 330 core`.
* **Symbols.** gtk-rs 0.18 has no GL function loader. GTK 3 loads GL through
  libepoxy, which is already linked into the process. The probe and POC use
  `libloading::Library::new("libepoxy.so.0")` and, for glow's
  `from_loader_function(|name| …)`, read the exported pointer variable
  `epoxy_<name>` (for example `epoxy_glClear`), double-dereferenced:
  `**lib.get::<*const *const c_void>(b"epoxy_glClear\0")`. The lookup is
  lazy-resolving, so it must happen with the GLArea's context current (inside
  `connect_render` or after `make_current()` in `connect_realize`). This is
  verified to work (GL_RENDERER printed through it, scene drawn). The
  `epoxy` crate is a possible alternative that I did not evaluate.
* **Framebuffer.** GTK binds the area's own FBO before `render`; do not bind
  framebuffer 0. Size is `allocated_width() * scale_factor()`. Use
  `set_auto_render(false)` and `queue_render()` on change so an idle graph
  costs nothing (the POC redraws only while the camera moves). Disable the
  depth buffer.
* **No GL.** The POC prints `GLArea cannot create a GL 3.3 context: <error>`
  when `a.error()` is set after `make_current()` in `connect_realize`. That
  branch **was not exercised** here (this VM always has GL, and llvmpipe is
  always available as software GL), so the detection code is untested. To
  force it at T7: `GDK_GL=disable` (GTK 3 environment switch) or an
  unsupported `required_version`. Also fall back when `libepoxy.so.0`
  cannot be opened, when a shader fails to compile or link, and when the
  context is below 3.3.
* **Software GL.** If GTK picks llvmpipe (VM without 3D) the GL path still
  works (table 3.2: llvmpipe is as fast as the Painter, 1.7–4.3 ms for 1080p–4K),
  so "GL present" does not imply "hardware GL": do not gate on the renderer
  string, gate on context creation and shader build.
* **X11.** The same widget works under X11 (Xvfb), so the user's session type
  does not matter.

## 7. Recommendation

Option (3): **GPU path with CPU fallback**, specifically:

1. Add a `GtkGLArea` renderer that draws the existing `Scene` with instanced
   shaders (raw GL via `glow`, shaders in the crate). No vector or scene
   library: nothing measured beats own shaders here, and the Painter is the
   better CPU renderer than any library.
2. Keep `Painter` + `DrawingArea` unchanged as the fallback and as the
   reference for parity tests. It is also the right renderer when GL is
   absent, and it is already fast offscreen. Do not spend time on further
   CPU rasteriser optimisation for this complaint: the blit-only probe
   shows the interval problem remains with zero rendering.
3. Keep the layout, camera, hit-testing, label cache, and input code as they
   are. Do not adopt petgraph, fjadra, force_graph, tiny-skia, femtovg or wgpu.

The case rests on the on-screen probes of section 3.3, which come from a
private headless compositor (section 11). If the user's own run of
`compare.sh` does not reproduce the gap between `current` and `gl`, do not
implement this.

## 8. Integration plan for T7

### 8.1 Crates

`impro-graph` gains `glow = "=0.18.0"` and `libloading = "0.8"` (workspace
`Cargo.toml` and lock change: allowed only in T7). `gtk` already enables
`v3_24` (needed for `GLArea::set_required_version`, `GLArea::error`).
No `khronos-egl`, no `epoxy` crate.

### 8.2 Module layout in `crates/impro-graph/src/view/`

| new / changed | content |
|---|---|
| `gl/mod.rs` | `GlPainter` with `new(loader)`, `render(&mut self, scene, w, h, PaintOptions)`, `Drop`; owns programs, VAOs, buffers, atlas |
| `gl/loader.rs` | the epoxy lookup of 6 above; returns `Option<glow::Context>` (None = fall back) |
| `gl/shaders.rs` | the four GLSL 330 programs (disc, line, dart, label) as `const &str` |
| `gl/buffers.rs` | per-instance vertex buffers and the dirty tracking that decides when to re-upload |
| `gl/atlas.rs` | A8 label atlas, shelf packer, LRU eviction |
| `labels.rs` (extracted from `draw.rs`) | `fn place_labels(scene, w, h, dev, motion, &mut LabelCache) -> &[LabelPlacement]`: the current `label()`/`motion_labels()` logic minus the cairo calls; the `Painter` and `GlPainter` both consume it, so the visibility rules stay in one place |
| `widget.rs`, `build.rs` | `canvas` becomes a `gtk::Stack` holding the `DrawingArea` and (when enabled) the `GLArea`; event handlers are connected to both through the existing `on!` macro; `Widget::canvas()` returns the stack's active child as `gtk::Widget` |
| `draw.rs` | `Painter` unchanged except the label extraction |

**Unchanged and reused by the GL path:** `Scene` (layout positions, camera
state, eased appearance arrays, hover/focus/drag state), `camera.rs`, `input.rs`,
hit testing (CPU, on `Scene`), `palette.rs`, `geometry.rs` formulas, `LabelCache`
(the Pango bitmaps), the tick/easing loop in `flow.rs` (it calls
`queue_render()` instead of `queue_draw()` for the GL widget).

### 8.3 Frame contract (what the GL path must reproduce)

Notation as in `draw.rs`: `dev` = device pixels per logical pixel,
`s` = `cam.zoom`, `rs` = sqrt(`s`); all values below are in **device pixels**
unless noted.

* **Projection** (vertex shader; uniform `u_cam = (pan_x, pan_y, s, dev)`):
  centre = `(pan + s·layout_xy)·dev`; radius = `layout_radius · rs · dev`.
  Positions come from `scene.layout.x()/y()/radius()` and are re-uploaded with
  `glBufferSubData` when the layout moves (3000 × 16 B ≈ 48 KB, once per tick,
  not per frame). The POC clamps radius to ≥ 0.8 px; check `Canvas::disc` and
  match its small-radius behaviour exactly (parity test).
* **Order:** clear to `palette.background`; ghosts (discs with their alpha);
  edges; arrows; nodes (the highlighted node last, with its ring); labels
  (the highlighted label last).
* **Edges** (instanced quad per `scene.lines[k]`): from `a + u·ra` to
  `b − u·rb` where `u` is the unit vector a→b and `ra`, `rb` the node radii;
  skip when `|ab| − ra − rb ≤ 0`. Width = `scene.disp.line · dev` device px.
  Colour = `mix(palette.line, palette.line_highlight, line_mix[k])`, alpha =
  `line_alpha[k] · palette.line.a`, skipped when it quantises to 0 (the
  Painter rounds alpha to 1/40; GL does not quantise, parity tolerance covers
  it). AA: quad half-extent `w/2 + 0.5`, fragment alpha `clamp(w/2 + 0.5 −
  |d|, 0, 1)` where `d` is the perpendicular distance in px.
  Per-instance: endpoint indices (endpoints are read from the node buffer in
  the vertex shader, so a layout tick does not rebuild the edge buffer),
  `line_alpha`, `line_mix`.
* **Arrows** (instanced, 2 triangles, `scene.arrows[k]`): `fade =
  clamp((s − 0.18)/0.62, 0, 1)`; skipped when `!show_arrow` or `fade ≤ 0`.
  `len = 6·sqrt(disp.line)·4/3·dev`, `half = len/2`, `notch = 0.75·len`,
  `gap = s·dev`. Tip `t = b − u·(rb + gap)`; wings at `t − u·len ± n·half`
  (`n` = perpendicular of `u`), notch at `t − u·notch`. Alpha =
  `arrow_alpha[k] · palette.arrow.a · fade`; skipped when `alpha·40 < 0.5`, when
  the visible shaft `|ab| − ra − rb ≤ disp.line·dev`, or (moving frames only)
  when the shaft is shorter than `1.5·len`. AA with a 1 px edge-distance
  ramp in the fragment shader (or a 2 px padded quad and a polygon SDF).
* **Nodes** (instanced quad with a circle SDF): coverage
  `clamp(r − |p| + 0.5, 0, 1)`, colour `scene.color[i]`, alpha
  `scene.alpha[i] · scene.base_alpha(i)`, skipped when `alpha·50 < 0.5`. The
  highlighted node: alpha `scene.alpha[i]`, drawn last, plus a ring at radius
  `r + 0.5·dev`, width `dev`, colour `palette.ring`.
* **Hover dimming and fades** need no GL-side logic: the `Scene` already eases
  `alpha`, `color`, `line_alpha`, `line_mix`, `arrow_alpha`, `drop` (the dim
  factor `HOVER_DIM = 0.28` lives in `Scene`). GL uploads these as
  per-instance attributes only on frames where `elements_dirty` or easing is
  running (3000 × 16 B per attribute set), nothing otherwise.
* **Labels:** placement from the extracted `place_labels`: font size
  `12 + 0.2·radius` (world) × `rs` on screen (the hovered label at zoom < 1
  keeps the unscaled size), alpha = `palette.text.a · label_opacity(s, detail)
  · scene.alpha[i]` (hovered: 1), not drawn under 3 px, centred on the node,
  baseline offset `(radius + 5)·rs + drop[i]`, wrap 224. Bitmaps = the existing
  `LabelCache` (Pango → cairo ARGB32 alpha masks, with their 0.1 size
  tolerance and 120-per-frame render budget). **Texture strategy:** upload each
  bitmap's alpha channel once into an A8 (`R8`) atlas texture
  (2048², 4 MB, shelf-packed, glow `tex_sub_image_2d`), one instance of
  `(rect, uv, alpha)` per visible label, drawn in one call; text colour =
  `palette.text.rgb` in the shader. Evict with the `LabelCache` (when the
  cache drops an entry or the atlas is full, clear the atlas and re-place the
  visible set; the benchmark atlas has no eviction, that is new work).
  Quantise label rects to whole pixels (the benchmark does `round()`), or the
  text shimmers during a pan.
* **Antialiasing:** analytic coverage in the fragment shaders, as above. No
  MSAA (GTK's GLArea FBO is not multisampled and MSAA costs fill rate on
  SVGA3D).
* **Blending:** `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` for RGB and `ZERO, ONE` for
  alpha so the opaque background stays opaque. **Known difference:** the
  Painter composites each alpha-class of edges as one union coverage mask, so
  overlapping edges of one class do not darken each other; plain blending
  does. Accept and bound it with the parity tolerance; if the dense fixture
  shows a visible difference, render edges of the dense case into an R8
  coverage texture with `MAX` blending and composite once per class.
* **HiDPI (scale 2):** framebuffer size `allocated_* · scale_factor()`;
  `dev = scale_factor()`; every width, radius and label size above is already
  in device px. Scale 2 was measured offscreen only (table 3.2, 28.6 ms at
  4K×2): on-screen scale 2 is **not measured**. Re-render labels at
  `size·dev` as the Painter does.

### 8.4 Selection, fallback, switch

* `IMPRO_GRAPH_RENDERER=cairo|gl`; unset = `gl` when it can be created,
  otherwise `cairo`. `cairo` never builds a `GLArea`.
* The Stack starts on the GLArea. In `connect_realize`: `make_current()`; if
  `error()` is set, the loader fails, GL < 3.3, or any shader compile/link
  fails → log once at `warn`, `stack.set_visible_child(&drawing_area)`,
  destroy the `GlPainter` and never try again in that process.
* In `connect_unrealize` (tab switch, window close): `make_current()` then
  drop the `GlPainter` so GL objects die with the right context; re-created
  lazily on the next render.
* A runtime GL error (`glGetError` in debug builds, context lost) triggers the
  same switch.
* The Painter and its caches are kept allocated only while it is the active
  renderer.

### 8.5 Testing

1. **Settled-pixel parity guard.** For the fixtures in `testing.rs`, render the
   settled frame with `Painter::render` and with `GlPainter` into an offscreen
   FBO (surfaceless EGL needs no window; under Xvfb the llvmpipe context
   is also fine), `glReadPixels`, and compare to the Painter image:
   per-channel absolute difference ≤ 8/255 on ≥ 99.5 % of pixels and a
   mean difference ≤ 0.8/255; label bounding boxes must overlap by
   ≥ 90 %. The tolerance exists because of the union-mask difference above and
   AA kernel differences; tighten it after the first implementation, never
   loosen it silently. Run it in CI-like `cargo test` under `xvfb-run`
   (llvmpipe) so it needs no GPU; skip with a clear message when no GL
   context can be created.
2. **Placement unit tests** for `place_labels` (the existing label tests in
   `testing.rs` move onto it unchanged).
3. **Fallback tests:** `IMPRO_GRAPH_RENDERER=cairo` builds no GLArea;
   `GDK_GL=disable` (or an impossible `required_version`) ends on the
   `DrawingArea` with one log line; both under Xvfb.
4. **Frame-interval acceptance** with `glarea_probe`-style harness against
   the private compositor, section 10.

### 8.6 Risks

| risk | effect | mitigation |
|---|---|---|
| VM or machine without 3D | GLArea cannot realise | detection and fallback of 8.4; llvmpipe still renders the GL path as fast as the Painter, so even software GL is not slower |
| Wayland compositor behaviour of the user's real session differs from the private one | the interval gain may be smaller or larger | the user runs `compare.sh` before T7 starts (section 11) |
| the GL tail is not clean: probe p99 50–67 ms, max 83 ms at 1080p, and p95 33 ms at 4K | acceptance (no frame > 34 ms) may fail on the compositor, not on our code | measure in T7 with the real widget; if it persists, check `glFinish`-free pacing and the frame-clock `update` phase before blaming the shaders |
| GPU time grows with pixels on SVGA3D (28.6 ms at 4K×2, over one frame) | scale-2 4K could still drop frames | reduce fill: skip the full clear when the scene is opaque-covered, draw into a texture and not a renderbuffer (femtovg is 3× faster at 4K×2, see 3.2); dynamic resolution while moving is the last resort |
| `epoxy_*` lookup depends on a libepoxy symbol layout | breaks on a libepoxy change | one function in `gl/loader.rs`, tested by the realise path, falls back to cairo when any core symbol is null |
| dense graphs: union-mask vs blended edge look | visible difference at crossings | 8.3 note, parity test on `dense` |
| two renderers to keep in step | drift | one placement function, one formula set, the parity test |
| context loss / driver bug | blank view | switch to cairo at runtime (8.4) |
| `gtk::Stack` around the canvas changes event routing and the `canvas()` accessor | regressions in input | the existing input tests plus a click/drag/scroll check on both children |

### 8.7 Effort (worker-hours, one competent worker, includes review fixes)

| item | hours |
|---|---:|
| `gl/` module: programs, buffers, loader, nodes/edges/arrows | 12 |
| label extraction (`labels.rs`) and the atlas path | 7 |
| widget Stack, realise/fallback/env switch, unrealize handling | 5 |
| parity guard, fallback tests, test fixtures under Xvfb | 6 |
| compositor measurement, tuning, acceptance run | 4 |
| docs and clean-up | 2 |
| **total** | **36** (range 30–44) |

## 9. Optional cheaper first step (not measured)

None of the CPU-side changes can fix the on-screen interval (blit-only shows
it), so I do not propose one. If the user's own run shows `current` is fine
on their session, stop here and keep everything as is.

## 10. Acceptance numbers for T7

On the private compositor, widget embedded in the real window, fixture `app`
and `packed`, pan, zoom-out, fling, with 300 labels:

* frame-clock interval **p95 ≤ 17 ms at 1920×1080 and 2560×1440** (baseline
  today: 33 / 66 ms; POC GL: 16.7 / 16.7);
* **no frame > 34 ms** during pan, zoom and fling after the first two frames
  (POC GL max was 83 ms at 1080p, 67 ms at 1440p: this number must be
  *achieved*, the POC does not show it);
* 4K: p95 ≤ 34 ms (POC GL: 33 ms) is the floor, 17 ms the target;
* idle CPU 0 %: no frame-clock tick and no wake-ups with a settled scene,
  as today;
* GL handler main-thread time p95 ≤ 4 ms with 300 labels;
* settled-pixel parity (8.5) green; fallback tests green;
* `IMPRO_GRAPH_RENDERER=cairo` output identical to today's.

## 11. What the numbers do NOT show

* The on-screen runs use a **private headless GNOME Shell** with a virtual
  monitor on this VM, not the user's real session. Same compositor family and
  the same client GL stack (SVGA3D), but a different output path (no real
  scan-out, possibly a different compositor renderer), so the *size* of the
  blit stall may differ on the real screen. The direction (area-proportional
  presentation cost with `DrawingArea`) is consistent across Wayland runs and
  appears on X11/Xvfb as well, but a real session was not measured.
* The frame-clock interval measures frame delivery as GTK sees it, not what
  the monitor shows, and 120 frames per run is a small sample for p99/max.
* The device scale of the user's session is unknown; on-screen scale 2 was not
  run. The user's screen size is unknown too (probe sizes 1600×900 to 3840×2160).
* The GL probe and POC draw no hover dimming, arrows, rings or fades, and
  the POC's labels are not wired; their cost is estimated from the offscreen
  label runs only.
* The "no GL" fallback branch was not exercised.
* The "why" in 3.3 is an inference.
* Timings of a shared VM: other work (the user's desktop, another worker)
  was running; the benches use fixed seeds and a shared lock, but run-to-run
  spread of ±10 % in p50 is plausible. The tails (p99, max) are the least
  reliable numbers.

**One command for the user, on their own display** (opens two ~2-second
1920×1080 windows on the real session, then prints the numbers; the first
run builds the probe, a few minutes):

```sh
cd scripts/graph-lib-bench && ./compare.sh
```

What to look at in the output: two blocks, `mode current` (today's cairo
`DrawingArea`) and `mode gl` (`GtkGLArea`). Compare the `frame-clock
interval` lines: `p50` should be ≈16.7 in both; if `current` shows `p95` ≥ 33
and `gl` shows `p95` ≈ 16.7, the gain is real on the real session and T7 is
justified. If both are ≈16.7, the stutter does not come from presentation on
that session and T7 should not start. The `GL_RENDERER` line tells you whether
the GPU or llvmpipe renders; a `GLArea realize error` line means GL is not
available there (the fallback case). Set `PROBE_SIZE=2560x1440` (any size
that fits the screen) to test your full-screen size.
