# Graph audit divergences

Audit completed 2026-10-23 13:18 IST.

175 observable rows: 151 same, 12 different, 12 unmeasurable. Every measured difference is listed below. Rows sharing a scenario can describe the same root cause. Missing telemetry is a coverage gap, not parity.

Reproduce with `bash scripts/graph-compare/run.sh` after setting the reference and Cairo-bridge environment variables described in README. The runner owns Xvfb, D-Bus, the shared lock and memory scope. Divergences do not produce a failing exit status.

Full values and tolerances: `.ceo/graph-compare/comparison.json`. Each driver writes `.ceo/graph-compare/{reference,impro}/measurements.json`; the JSON key is `steps.<scenario>`. PNG evidence is included only where captured. The reference checkout is read only.

## 1. Count of on-screen markers after the background drag

Owner: **T1**. Scenario: `pan`. Row: `pan-visible`.

Reference: `31`.

impro: `0`.

Tolerance: 8 markers; independently placed scenes.

Suspected location: `crates/impro-graph/src/view/scene.rs:724`. Proposed fix: Pass the background-press state into camera advancement so drag velocity is not integrated as released inertia.

Evidence: run the command above; inspect `steps.pan` in both measurement files and row `pan-visible` in `.ceo/graph-compare/comparison.json`.

Frames: `.ceo/graph-compare/reference/pan.png`, `.ceo/graph-compare/impro/pan.png`.

## 2. Background drag follows 80 by 32 screen pixels

Owner: **T1**. Scenario: `pan`. Row: `pan-distance`.

Reference: `[80.0,32.0]`.

impro: `[-510004.5553,-204001.8221]`.

Tolerance: 0.1 px.

Suspected location: `crates/impro-graph/src/view/scene.rs:724`. Proposed fix: Pass the background-press state into camera advancement so drag velocity is not integrated as released inertia.

Evidence: run the command above; inspect `steps.pan` in both measurement files and row `pan-distance` in `.ceo/graph-compare/comparison.json`.

Frames: `.ceo/graph-compare/reference/pan.png`, `.ceo/graph-compare/impro/pan.png`.

## 3. Inertia travel after the scripted 8-move fling

Owner: **T1**. Scenario: `pan`. Row: `pan-inertia`.

Reference: `[43.89,17.56]`.

impro: `[-185192.06,-74076.82]`.

Tolerance: 8 px scheduling allowance.

Suspected location: `crates/impro-graph/src/view/scene.rs:724`. Proposed fix: Pass the background-press state into camera advancement so drag velocity is not integrated as released inertia.

Evidence: run the command above; inspect `steps.pan` in both measurement files and row `pan-inertia` in `.ceo/graph-compare/comparison.json`.

Frames: `.ceo/graph-compare/reference/pan.png`, `.ceo/graph-compare/impro/pan.png`.

## 4. Closing the controls exposes the three-button chip

Owner: **T1**. Scenario: `key-escape`. Row: `closed-chip`.

Reference: `true`.

impro: `false`.

Tolerance: exact.

Suspected location: `crates/impro-graph/src/view/panel.rs:538`. Proposed fix: Explicitly show the guarded chip and its children when closing controls; show_all honours no-show-all and leaves the strip hidden.

Evidence: run the command above; inspect `steps.key-escape` in both measurement files and row `closed-chip` in `.ceo/graph-compare/comparison.json`.

Frames: `.ceo/graph-compare/reference/key-escape.png`, `.ceo/graph-compare/impro/key-escape.png`.

An isolated GTK probe confirms the visibility mechanism: a box guarded by `no-show-all` stays hidden after `show_all`, and becomes visible after explicit `show`. The native chip has that guard at `panel.rs:538`, while both close paths call only `show_all` at lines 701 and 869. Probe output: `.ceo/graph-compare/gtk-visibility.txt`. The runtime graph frames independently show the missing strip.

## 5. Distance the pointer anchor moves: wheel-out

Owner: **Phase B**. Scenario: `wheel-out`. Row: `anchor-wheel-out`.

Reference: `79.7545`.

impro: `0.0`.

Tolerance: 0.1 px.

Suspected location: `crates/impro-graph/src/view/input.rs:209`. Proposed fix: Use the canvas centre as the anchor when the wheel reduces zoom.

Evidence: run the command above; inspect `steps.wheel-out` in both measurement files and row `anchor-wheel-out` in `.ceo/graph-compare/comparison.json`.

## 6. Replay visible IDs at 250 ms and 1000 ms: replay

Owner: **T2**. Scenario: `replay`. Row: `replay`.

Reference: `[[".impro/Notes/First.md"],[".impro/Notes/First.md",".impro/Notes/Second.md","chain/chain-0.md","chain/chain-1.md","chain/chain-2.md"]]`.

impro: `[["hub/Hub.md"],["hub/Hub.md"]]`.

Tolerance: exact.

Suspected location: `crates/impro-graph/src/view/timelapse.rs:25`. Proposed fix: Align the index timestamp fallback with the reference before sorting replay chronology.

Evidence: run the command above; inspect `steps.replay` in both measurement files and row `replay` in `.ceo/graph-compare/comparison.json`.

Read-only source inspection explains the likely data cause: on this Linux filesystem the reference uses the minimum of change/modified timestamps when its runtime does not expose birth time; the native index prefers the filesystem creation timestamp. The fixture has equal old modified times, so the reference breaks ties by ID while impro starts with the first-created hub. The initial snapshot records reference node chronology and native filesystem birth-time evidence. Inspect `crates/impro-graph/src/index/scan.rs:163` together with the view sorting site.

## 7. Replay visible IDs at 250 ms and 1000 ms: replay-reduced

Owner: **T2**. Scenario: `replay-reduced`. Row: `replay-reduced`.

Reference: `[[".impro/Notes/First.md"],[".impro/Notes/First.md",".impro/Notes/Second.md","chain/chain-0.md","chain/chain-1.md","chain/chain-2.md"]]`.

impro: `[["hub/Hub.md"],["hub/Hub.md"]]`.

Tolerance: exact.

Suspected location: `crates/impro-graph/src/view/timelapse.rs:25`. Proposed fix: Align the index timestamp fallback with the reference before sorting replay chronology.

Evidence: run the command above; inspect `steps.replay-reduced` in both measurement files and row `replay-reduced` in `.ceo/graph-compare/comparison.json`.

Read-only source inspection explains the likely data cause: on this Linux filesystem the reference uses the minimum of change/modified timestamps when its runtime does not expose birth time; the native index prefers the filesystem creation timestamp. The fixture has equal old modified times, so the reference breaks ties by ID while impro starts with the first-created hub. The initial snapshot records reference node chronology and native filesystem birth-time evidence. Inspect `crates/impro-graph/src/index/scan.rs:163` together with the view sorting site.

## 8. Screenshot label visibility and ink extent at zoom 1

Owner: **Phase B**. Scenario: `label-1`. Row: `label-1`.

Reference: `{"visible":true,"height":13,"width":47}`.

impro: `{"visible":true,"height":17,"width":46}`.

Tolerance: 3 px ink bounding box; visibility exact.

Suspected location: `crates/impro-graph/src/view/draw.rs:164`. Proposed fix: Check Pango font sizing and label bitmap insets against the measured orphan ink bounds at zoom one.

Evidence: run the command above; inspect `steps.label-1` in both measurement files and row `label-1` in `.ceo/graph-compare/comparison.json`.

Frames: `.ceo/graph-compare/reference/label-1.png`, `.ceo/graph-compare/impro/label-1.png`.

This is a raster observation, not a nominal font/alpha mismatch proven by state telemetry. Font-mask coverage, cached bitmap scaling and glyph padding can affect these samples. Request the paint telemetry listed below before adjusting style constants.

## 9. Peak screenshot label contrast versus text role at zoom 0.7

Owner: **Phase B**. Scenario: `label-0.7`. Row: `label-alpha-0.7`.

Reference: `0.753`.

impro: `0.589`.

Tolerance: 0.15 alpha estimate; font coverage differs.

Suspected location: `crates/impro-graph/src/view/draw.rs:164`. Proposed fix: Measure nominal label alpha separately from font-mask coverage before adjusting native label fade.

Evidence: run the command above; inspect `steps.label-0.7` in both measurement files and row `label-alpha-0.7` in `.ceo/graph-compare/comparison.json`.

Frames: `.ceo/graph-compare/reference/label-0.7.png`, `.ceo/graph-compare/impro/label-0.7.png`.

This is a raster observation, not a nominal font/alpha mismatch proven by state telemetry. Font-mask coverage, cached bitmap scaling and glyph padding can affect these samples. Request the paint telemetry listed below before adjusting style constants.

## 10. Isolated-cluster to hub centroid separation

Owner: **Phase B**. Scenario: `default`. Row: `layout-clusters`.

Reference: `321.21`.

impro: `433.6`.

Tolerance: 80 world units; independent layouts.

Suspected location: `crates/impro-graph/src/view/scene.rs:269`. Proposed fix: Check cluster-separation distributions on a second fixture before tuning native spacing, since exact independent layout coordinates are not required.

Evidence: run the command above; inspect `steps.default` in both measurement files and row `layout-clusters` in `.ceo/graph-compare/comparison.json`.

Frames: `.ceo/graph-compare/reference/default.png`, `.ceo/graph-compare/impro/default.png`.

## 11. Unsaved-buffer visible IDs after 200 ms

Owner: **Phase B**. Scenario: `buffer`. Row: `buffer-200`.

Reference: `[".impro/Notes/First.md",".impro/Notes/Second.md","?Buffer Target","?Missing Note","chain/chain-0.md","chain/chain-1.md","chain/chain-2.md","chain/chain-3.md","chain/chain-4.md","chain/chain-5.md","chain/chain-6.md","chain/chain-7.md","cluster/cluster-0.md","cluster/cluster-1.md","cluster/cluster-2.md","cluster/cluster-3.md","cluster/cluster-4.md","gallery.md","hub/Hub.md","hub/spoke-0.md","hub/spoke-1.md","hub/spoke-10.md","hub/spoke-11.md","hub/spoke-12.md","hub/spoke-13.md","hub/spoke-2.md","hub/spoke-3.md","hub/spoke-4.md","hub/spoke-5.md","hub/spoke-6.md","hub/spoke-7.md","hub/spoke-8.md","hub/spoke-9.md","orphan.md","src/helper.ts","src/main.py","src/util.py","src/widget.ts"]`.

impro: `[".impro/Notes/First.md",".impro/Notes/Second.md","?Missing Note","chain/chain-0.md","chain/chain-1.md","chain/chain-2.md","chain/chain-3.md","chain/chain-4.md","chain/chain-5.md","chain/chain-6.md","chain/chain-7.md","cluster/cluster-0.md","cluster/cluster-1.md","cluster/cluster-2.md","cluster/cluster-3.md","cluster/cluster-4.md","gallery.md","hub/Hub.md","hub/spoke-0.md","hub/spoke-1.md","hub/spoke-10.md","hub/spoke-11.md","hub/spoke-12.md","hub/spoke-13.md","hub/spoke-2.md","hub/spoke-3.md","hub/spoke-4.md","hub/spoke-5.md","hub/spoke-6.md","hub/spoke-7.md","hub/spoke-8.md","hub/spoke-9.md","orphan.md","src/helper.ts","src/main.py","src/util.py","src/widget.ts"]`.

Tolerance: exact.

Suspected location: `crates/impro-graph/src/view/flow.rs:301`. Proposed fix: Compare the full editor forwarding path before changing debounce; keep the 400 ms policy until a window trace establishes a user-visible difference.

Evidence: run the command above; inspect `steps.buffer` in both measurement files and row `buffer-200` in `.ceo/graph-compare/comparison.json`.

This is a widget-boundary diagnostic. The reference receives direct `set_text` calls; the native widget itself debounces buffer updates. The reference application can debounce before calling its widget. This row does not establish an application-level delay defect.

## 12. Replay reveal plus final cooling wall time: replay-reduced

Owner: **Phase B**. Scenario: `replay-reduced`. Row: `duration-replay-reduced`.

Reference: `11483`.

impro: `13616`.

Tolerance: 1500 ms scheduling allowance.

Suspected location: `crates/impro-graph/src/view/flow.rs:503`. Proposed fix: Re-measure completion after T2 aligns chronology, then check whether unchanged replay topology unnecessarily reheats the layout.

Evidence: run the command above; inspect `steps.replay-reduced` in both measurement files and row `duration-replay-reduced` in `.ceo/graph-compare/comparison.json`.

Treat this as a timing observation requiring repetition. The earlier complete pair in `.ceo/graph-compare-reviewed` completed this step in approximately 13.8 seconds on the reference and 13.6 seconds on impro. The final reference sample is shorter. Chronology and the time of the last structural change affect cooling, so align the T2 data difference before changing replay pacing.

## Blank, crash and deadline observations

The frame-paced background drag is the blank-canvas candidate: start from a settled graph at zoom 1, press empty canvas at (2,2), send eight pointer moves by (10,4) px with a 16 ms main-loop interval, then release at (82,34). Compare the held pan and the visible marker count. The native probe uses GTK-dispatched GDK events with monotonic timestamps. The matching reference receives DOM pointer events. The controls remain present; this does not reproduce disappearance of the entire dock body.

No process crash or full-widget disappearance was observed in the completed scripted run. An initial exploratory run reached replay and hit an overly short 12-second harness settle deadline; the completed run allows 30 seconds for reveal plus cooling. The original deadline log is `.ceo/graph-compare-initial/impro-driver.log`. This was not evidence of a product hang.

## Remaining measurements and Phase B requests

The complete unmeasurable register is in BEHAVIOURS.md and the JSON table. Needed view helpers are read-only line/arrow/label paint telemetry, effective nominal font size/wrap/alpha, cursor names, camera velocity and controlled frame stepping. No hook was added inside the production view.

Full-window work is still needed for actual editor tab/dock routing, the editor-to-graph 400 ms debounce boundary, persisted settings through a fresh application process, and write timing. The transient loading state was not captured before data delivery. Wayland and the user desktop were not accessed. Exact arrow opacity and font-wrap metrics remain unmeasured; raster bounds and sampled connection widths/colours are included where unobscured.
