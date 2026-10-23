# Graph behaviour audit

The final paired audit observes the running reference widget and impro's independent native
widget, with full-window checks for editor debounce and settings persistence.
Measurement does not alter either graph implementation. The oracle inputs are
its graph specification, parameter fixture, GTK tests, browser tests and core
tests; runtime observations are authoritative when text and behaviour differ.

The paired sequence starts from defaults: controls open, Project scope expanded,
other sections collapsed; tags/assets hidden, code/missing/unlinked files visible,
arrow markers enabled, zoom 1, detail 0.2, marker multiplier 1.15 and line width
1.25. Local depth is two with both directions enabled. The generated project has
a hub, a chain, an isolated cluster, orphan, tags, assets, missing targets,
Markdown/wiki references, source imports and two notes. A separate 3005-file
fixture exercises the cap.

Observable expectations to audit:

- Radius grows logarithmically with directed degree, bounded before its size
  multiplier, and scales by square root of zoom. File, tag, asset, missing-target,
  source, active and hovered colours follow theme roles in dark, light, midnight
  and ember. Lines keep screen width; arrow visibility increases with zoom.
- Labels fade with zoom and the detail control. Their font size depends on marker
  size, wraps at a bounded width, and remains readable for a highlighted marker.
  Hover dims unrelated markers and connections to 28 percent, retaining neighbours,
  and changes the cursor; leaving restores opacity.
- Movement up to 6 px remains a click. File activation opens the file; Ctrl and
  middle clicks request another tab. Tags emit search and missing targets emit
  creation. A file context menu lists Open file, Open another tab, Open in right
  pane, Locate in project tree, Copy file location. Tag right-click searches;
  missing-target right-click has no action.
- A dragged marker follows the pointer, stays pinned while held, heats the
  simulation and becomes free on release. Background dragging pans, followed by
  decaying inertia; a new press stops inertia. Wheel input uses an exponential
  factor, bounded zoom and an anchor; small Ctrl-wheel input is doubled. Zoom
  eases unless motion is reduced. Arrows pan, Shift accelerates, plus/minus zoom,
  Enter activates hover and Escape dismisses menus/controls then hover.
- Fit/reset commands must be compared at their available host entry points.
  Layout observations include settle time, spacing, cluster separation,
  determinism, unchanged survivor positions at delivery and neighbour-based
  placement of new files.
- Controls expose their section order, collapsed states, every label, range,
  increment and default. Search filters live. Global, Local and Note modes,
  local depth and traversal toggles, colour-rule add/remove/colour, reset
  retention, replay order/speed and replay with reduced motion are observable.
- Status includes loading, empty-filter, missing local focus and truncation
  notices. Preferences persist search, zoom, groups and panel/section state with
  debounce. Save, rename, deletion and unsaved-buffer edits update the graph;
  editor buffer forwarding is debounced 400 ms. Themes, accent, font and motion
  preferences apply live. Resize keeps the camera centre; an unmapped view
  performs no frame work and resumes visibly. A settled graph has negligible CPU.

The row register below contains measured verdicts, individual tolerances and
explicit coverage gaps. Raw values live in the run's `comparison.json`;
`DIVERGENCES.md` explains every remaining difference. An unmeasurable verdict is a
coverage gap, never a passing result.

## Measured controls

Both widgets expose the following initial numeric controls; ranges and steps
come from live GTK/DOM introspection. Camera zoom is a persisted continuous
setting, exercised through wheel input rather than a panel slider.

| Label / key | Default | Minimum | Maximum | Step |
| --- | --- | --- | --- | --- |
| Connection width / `lineSizeMultiplier` | 1.25 | 0.5 | 4 | 0.25 |
| Pull toward centre / `centerStrength` | 0.12 | 0 | 0.6 | 0.02 |
| File marker size / `nodeSizeMultiplier` | 1.15 | 0.5 | 3 | 0.05 |
| Space between files / `repelStrength` | 6 | 0 | 24 | 0.5 |
| Preferred spacing / `linkDistance` | 160 | 48 | 480 | 8 |
| Neighbourhood hops / `localDepth` | 2 | 1 | 6 | 1 |
| Label detail / `textFadeMultiplier` | 0.2 | -1 | 2 | 0.1 |
| Connection tension / `linkStrength` | 0.35 | 0 | 1.5 | 0.05 |
| Camera zoom / `scale` | 1 | 0.04 | 6 | continuous |

| Toggle label | Default |
| --- | --- |
| Referenced assets | off |
| Source and imports | on |
| Unlinked files | on |
| Reference direction | on |
| Tag markers | off |
| References to | on |
| Referenced by | on |
| Hide missing targets | off |

Search is labelled “Filter project files”, with placeholder “Find paths, names
or text…”. Action labels are “Add colour rule”, “Reset graph preferences”,
“Hide graph controls” and “Replay history”. The mode menu orders “Global graph”,
“Local graph”, “Note graph”. Section toggling and a nondefault persisted section
state are measured, as well as the initial section states.

<!-- audit-register -->

Measured 2026-10-23 19:20 IST.

180 rows: 176 same, 3 different, 1 unmeasurable.

| ID | Observable behaviour | Scenario | Verdict | Tolerance or coverage reason |
| --- | --- | --- | --- | --- |
| initial-defaults | Default visibility, force, display, local and camera preferences | default | same | exact |
| initial-graph | Default visible IDs and undirected line count | default | same | exact |
| search-label | Search field accessible label and placeholder | default | same | exact |
| panel-width | Open controls width | default | same | 1 logical px |
| initial-panel | Controls open by default | default | same | exact |
| sections | Section order and default collapsed states | default | same | exact |
| slider-localDepth | Control label, minimum, maximum, step and default: localDepth | default | same | numeric 1e-8; labels exact |
| slider-textFadeMultiplier | Control label, minimum, maximum, step and default: textFadeMultiplier | default | same | numeric 1e-8; labels exact |
| slider-nodeSizeMultiplier | Control label, minimum, maximum, step and default: nodeSizeMultiplier | default | same | numeric 1e-8; labels exact |
| slider-lineSizeMultiplier | Control label, minimum, maximum, step and default: lineSizeMultiplier | default | same | numeric 1e-8; labels exact |
| slider-centerStrength | Control label, minimum, maximum, step and default: centerStrength | default | same | numeric 1e-8; labels exact |
| slider-repelStrength | Control label, minimum, maximum, step and default: repelStrength | default | same | numeric 1e-8; labels exact |
| slider-linkStrength | Control label, minimum, maximum, step and default: linkStrength | default | same | numeric 1e-8; labels exact |
| slider-linkDistance | Control label, minimum, maximum, step and default: linkDistance | default | same | numeric 1e-8; labels exact |
| toggles | Every toggle label and default checked state | default | same | exact |
| control-buttons | Add rule, reset, close and replay labels | default | same | exact |
| section-toggle | Every section toggles its saved collapsed state | sections-toggled | same | exact |
| control-order | Control labels in widget/DOM order | default | same | exact |
| global-local-rows | Global mode hides local hops and direction rows | default | same | exact |
| radii-degree | World radii for orphan, hub, spoke, code, tag and missing target | expanded | same | 0.01 world unit |
| colour-dark-active | dark active marker RGB | active-dark | same | exact |
| colour-dark-hover | dark hover marker RGB | hover-dark | same | exact |
| colour-dark-file | dark settled file marker RGB | theme-dark | same | exact |
| colour-dark-tag | dark settled tag marker RGB | theme-dark | same | exact |
| colour-dark-asset | dark settled asset marker RGB | theme-dark | same | exact |
| colour-dark-unresolved | dark settled unresolved marker RGB | theme-dark | same | exact |
| colour-dark-code | dark settled code marker RGB | theme-dark | same | exact |
| pixel-radius-dark | Screenshot marker radii in logical pixels: dark | theme-dark | same | 1 px boundary coverage; shared unoccluded kinds only |
| pixels-dark | Screenshot marker-centre RGB samples: dark | theme-dark | same | 3/255 per RGB channel; shared unoccluded kinds only |
| colour-light-active | light active marker RGB | active-light | same | exact |
| colour-light-hover | light hover marker RGB | hover-light | same | exact |
| colour-light-file | light settled file marker RGB | theme-light | same | exact |
| colour-light-tag | light settled tag marker RGB | theme-light | same | exact |
| colour-light-asset | light settled asset marker RGB | theme-light | same | exact |
| colour-light-unresolved | light settled unresolved marker RGB | theme-light | same | exact |
| colour-light-code | light settled code marker RGB | theme-light | same | exact |
| pixel-radius-light | Screenshot marker radii in logical pixels: light | theme-light | same | 1 px boundary coverage; shared unoccluded kinds only |
| pixels-light | Screenshot marker-centre RGB samples: light | theme-light | same | 3/255 per RGB channel; shared unoccluded kinds only |
| colour-midnight-active | midnight active marker RGB | active-midnight | same | exact |
| colour-midnight-hover | midnight hover marker RGB | hover-midnight | same | exact |
| colour-midnight-file | midnight settled file marker RGB | theme-midnight | same | exact |
| colour-midnight-tag | midnight settled tag marker RGB | theme-midnight | same | exact |
| colour-midnight-asset | midnight settled asset marker RGB | theme-midnight | same | exact |
| colour-midnight-unresolved | midnight settled unresolved marker RGB | theme-midnight | same | exact |
| colour-midnight-code | midnight settled code marker RGB | theme-midnight | same | exact |
| pixel-radius-midnight | Screenshot marker radii in logical pixels: midnight | theme-midnight | same | 1 px boundary coverage; shared unoccluded kinds only |
| pixels-midnight | Screenshot marker-centre RGB samples: midnight | theme-midnight | same | 3/255 per RGB channel; shared unoccluded kinds only |
| colour-ember-active | ember active marker RGB | active-ember | same | exact |
| colour-ember-hover | ember hover marker RGB | hover-ember | same | exact |
| colour-ember-file | ember settled file marker RGB | theme-ember | same | exact |
| colour-ember-tag | ember settled tag marker RGB | theme-ember | same | exact |
| colour-ember-asset | ember settled asset marker RGB | theme-ember | same | exact |
| colour-ember-unresolved | ember settled unresolved marker RGB | theme-ember | same | exact |
| colour-ember-code | ember settled code marker RGB | theme-ember | same | exact |
| pixel-radius-ember | Screenshot marker radii in logical pixels: ember | theme-ember | same | 1 px boundary coverage; shared unoccluded kinds only |
| pixels-ember | Screenshot marker-centre RGB samples: ember | theme-ember | same | 3/255 per RGB channel; shared unoccluded kinds only |
| highlight-active | active marker colour | active | same | exact |
| highlight-hover | hover marker colour | hover | same | exact |
| highlight-accent | accent marker colour | accent | same | exact |
| radius-zoom-0.1 | Radius scales with square root of zoom: 0.1 | zoom-0.1 | same | 0.01 world unit after dividing by sqrt(zoom) |
| line-width-0.1 | Screenshot connection width at zoom 0.1 | zoom-0.1 | same | 1 px FWHM; excludes clipped/covered midpoint |
| line-colour-0.1 | Screenshot connection RGB at zoom 0.1 | zoom-0.1 | same | 20/255 per channel; raster coverage allowance |
| label-0.1 | Screenshot label visibility and ink extent at zoom 0.1 | label-0.1 | same | 3 px ink bounding box; visibility exact |
| radius-zoom-0.25 | Radius scales with square root of zoom: 0.25 | zoom-0.25 | same | 0.01 world unit after dividing by sqrt(zoom) |
| line-width-0.25 | Screenshot connection width at zoom 0.25 | zoom-0.25 | same | 1 px FWHM; excludes clipped/covered midpoint |
| line-colour-0.25 | Screenshot connection RGB at zoom 0.25 | zoom-0.25 | same | 20/255 per channel; raster coverage allowance |
| label-0.25 | Screenshot label visibility and ink extent at zoom 0.25 | label-0.25 | same | 3 px ink bounding box; visibility exact |
| radius-zoom-0.7 | Radius scales with square root of zoom: 0.7 | zoom-0.7 | same | 0.01 world unit after dividing by sqrt(zoom) |
| line-width-0.7 | Screenshot connection width at zoom 0.7 | zoom-0.7 | same | 1 px FWHM; excludes clipped/covered midpoint |
| line-colour-0.7 | Screenshot connection RGB at zoom 0.7 | zoom-0.7 | same | 20/255 per channel; raster coverage allowance |
| label-0.7 | Screenshot label visibility and ink extent at zoom 0.7 | label-0.7 | same | 3 px ink bounding box; visibility exact |
| radius-zoom-1 | Radius scales with square root of zoom: 1 | zoom-1 | same | 0.01 world unit after dividing by sqrt(zoom) |
| line-width-1 | Screenshot connection width at zoom 1 | zoom-1 | same | 1 px FWHM; excludes clipped/covered midpoint |
| line-colour-1 | Screenshot connection RGB at zoom 1 | zoom-1 | same | 20/255 per channel; raster coverage allowance |
| label-1 | Screenshot label visibility and ink extent at zoom 1 | label-1 | same | 3 px ink bounding box; visibility exact |
| radius-zoom-2 | Radius scales with square root of zoom: 2 | zoom-2 | same | 0.01 world unit after dividing by sqrt(zoom) |
| line-width-2 | Screenshot connection width at zoom 2 | zoom-2 | same | 1 px FWHM; excludes clipped/covered midpoint |
| line-colour-2 | Screenshot connection RGB at zoom 2 | zoom-2 | same | 20/255 per channel; raster coverage allowance |
| label-2 | Screenshot label visibility and ink extent at zoom 2 | label-2 | same | 3 px ink bounding box; visibility exact |
| label-detail--1 | Label ink visibility and size with detail -1 | detail--1 | same | 3 px ink bounding box; visibility exact |
| label-detail-2 | Label ink visibility and size with detail 2 | detail-2 | same | 3 px ink bounding box; visibility exact |
| arrows | Painted arrow opacity, length, half-width and notch at each zoom | zoom-1 | same | 0.01 logical px/opacity; culled markers omitted |
| label-alpha-0.1 | Peak screenshot label contrast versus text role at zoom 0.1 | label-0.1 | same | 0.15 alpha estimate; font coverage differs |
| label-alpha-0.25 | Peak screenshot label contrast versus text role at zoom 0.25 | label-0.25 | same | 0.15 alpha estimate; font coverage differs |
| label-alpha-0.7 | Peak screenshot label contrast versus text role at zoom 0.7 | label-0.7 | same | 0.15 alpha estimate; font coverage differs |
| label-alpha-1 | Peak screenshot label contrast versus text role at zoom 1 | label-1 | same | 0.15 alpha estimate; font coverage differs |
| label-alpha-2 | Peak screenshot label contrast versus text role at zoom 2 | label-2 | same | 0.15 alpha estimate; font coverage differs |
| label-font-size | Nominal label font size and wrapping width | label-1 | same | 0.01 logical px; requested sizes before glyph coverage |
| label-nominal-alpha-0.25 | Effective label alpha before glyph coverage: 0.25 | label-0.25 | same | 0.001 opacity |
| label-nominal-alpha-0.7 | Effective label alpha before glyph coverage: 0.7 | label-0.7 | same | 0.001 opacity |
| label-nominal-alpha-1 | Effective label alpha before glyph coverage: 1 | label-1 | same | 0.001 opacity |
| label-nominal-alpha-2 | Effective label alpha before glyph coverage: 2 | label-2 | same | 0.001 opacity |
| hover-dim | Hover leaves unrelated markers at 28 percent opacity | hover | same | 0.01 opacity |
| cursor | Hover installs a pointer cursor | hover | same | exact |
| hover-edges | Hover connection alpha after paint bucketing | hover | same | 0.001 opacity; touching/unrelated connections |
| cursor-shape | Exact grabbing and pointer cursor names | hover | same | exact |
| events-click | Emitted graph actions: click | click | same | exact |
| events-ctrl-click | Emitted graph actions: ctrl-click | ctrl-click | same | exact |
| events-middle-click | Emitted graph actions: middle-click | middle-click | same | exact |
| events-slop-6 | Emitted graph actions: slop-6 | slop-6 | same | exact |
| events-slop-7 | Emitted graph actions: slop-7 | slop-7 | same | exact |
| events-tag-click | Emitted graph actions: tag-click | tag-click | same | exact |
| events-missing-click | Emitted graph actions: missing-click | missing-click | same | exact |
| events-tag-right | Emitted graph actions: tag-right | tag-right | same | exact |
| events-missing-right | Emitted graph actions: missing-right | missing-right | same | exact |
| events-key-enter | Emitted graph actions: key-enter | key-enter | same | exact |
| menu-actions | Each file menu item emits its matching host action | menu-actions | same | exact |
| file-menu | Right-click file menu items and order | file-menu | same | exact |
| drag-held | Node drag is pinned to pointer and layout stays warm | drag | same | 0.03 layout alpha; flags exact |
| drag-follow | Held marker stays at the requested pointer displacement during simulation | drag | same | 1 screen px |
| drag-release | Releasing a drag emits no open action and settles | drag | same | exact |
| pan-distance | Background drag follows 80 by 32 screen pixels | pan | same | 0.1 px |
| pan-inertia | Inertia travel after the scripted 8-move fling | pan | same | 8 px scheduling allowance |
| pan-stop | Pressing the background stops inertia immediately | stop-on-press | same | 0.05 px |
| pan-decay | Pan velocity decay over a controlled 60 Hz frame | controlled-motion | same | 1e-6 retention factor |
| pan-filter | Released velocity after eight controlled 16 ms pointer moves | controlled-motion | same | 1e-6 logical px/ms |
| zoom-wheel-in | Wheel target factor or limit: wheel-in | wheel-in | same | 1e-6 zoom ratio |
| zoom-wheel-out | Wheel target factor or limit: wheel-out | wheel-out | same | 1e-6 zoom ratio |
| zoom-ctrl-wheel | Wheel target factor or limit: ctrl-wheel | ctrl-wheel | same | 1e-6 zoom ratio |
| zoom-wheel-max | Wheel target factor or limit: wheel-max | wheel-max | same | 1e-6 zoom ratio |
| zoom-wheel-min | Wheel target factor or limit: wheel-min | wheel-min | same | 1e-6 zoom ratio |
| anchor-wheel-in | Distance the pointer anchor moves: wheel-in | wheel-in | same | 0.1 px |
| anchor-wheel-out | Distance the pointer anchor moves: wheel-out | wheel-out | same | 0.1 px |
| reduced-motion | Live reduced-motion preference reaches the zoom target within 40 ms | reduced-wheel | same | exact |
| zoom-ease | Wheel zoom easing over a controlled 60 Hz frame | controlled-motion | same | 1e-6 fraction of remaining zoom |
| keyboard-ArrowLeft | Held 200 ms key movement: ArrowLeft | key-ArrowLeft | same | 25 px pan or 0.12 zoom ratio scheduling allowance |
| keyboard-ArrowRight | Held 200 ms key movement: ArrowRight | key-ArrowRight | same | 25 px pan or 0.12 zoom ratio scheduling allowance |
| keyboard-ArrowUp | Held 200 ms key movement: ArrowUp | key-ArrowUp | same | 25 px pan or 0.12 zoom ratio scheduling allowance |
| keyboard-ArrowDown | Held 200 ms key movement: ArrowDown | key-ArrowDown | same | 25 px pan or 0.12 zoom ratio scheduling allowance |
| keyboard-+ | Held 200 ms key movement: + | key-+ | same | 25 px pan or 0.12 zoom ratio scheduling allowance |
| keyboard-- | Held 200 ms key movement: - | key-- | same | 25 px pan or 0.12 zoom ratio scheduling allowance |
| keyboard-shift | Shift arrow travel relative to the unmodified arrow | key-shift | same | 0.35 travel ratio; 200 ms held keys |
| key-escape | Escape closes controls, then clears hover: key-escape | key-escape | same | exact |
| key-escape-again | Escape closes controls, then clears hover: key-escape-again | key-escape-again | same | exact |
| camera-reset | Reset camera recentres, restores zoom one and keeps the graph | camera-reset | same | 0.1 logical px; zoom exact |
| fit-camera | Fit camera command | default | unmeasurable | No reference fit-camera entry point exists in the graph widget or window. The native method is an extra feature without a paired oracle command. |
| layout-settle | Fresh layout wall time to settle | default | same | 1500 ms scheduling allowance |
| layout-spacing | Settled median nearest-neighbour spacing | default | same | 20 world units; independent layouts |
| layout-clusters | Isolated-cluster to hub centroid separation | default | different | 80 world units; independent layouts |
| layout-determinism | Maximum fresh-start world-position difference against a second process | default | same | 0.05 world unit in each implementation |
| search | Live search produces the filtered IDs | search | same | exact |
| local | Local depth two, both directions, visible IDs and rows | local | same | exact |
| local-outgoing | Outgoing-only traversal IDs | local-outgoing | same | exact |
| note-mode | Note graph visible IDs | note | same | exact |
| type-menu | Global / Local / Note graph menu labels and order | type-menu | same | exact |
| closed-chip | Closing the controls exposes the three-button chip | key-escape | same | exact |
| group-add | Colour rule count and generated colour: group-add | group-add | same | exact |
| group-remove | Colour rule count and generated colour: group-remove | group-remove | same | exact |
| group-colour | First matching colour rule colours file markers | group-colour | same | exact |
| reset | Reset restores defaults while retaining search, zoom and section visibility | reset | same | 1e-8 numeric; other fields exact |
| replay | Replay visible IDs at 250 ms and 1000 ms: replay | replay | same | exact |
| replay-reduced | Replay visible IDs at 250 ms and 1000 ms: replay-reduced | replay-reduced | same | exact |
| duration-replay | Replay reveal plus final cooling wall time: replay | replay | same | 1500 ms scheduling allowance |
| duration-replay-reduced | Replay reveal plus final cooling wall time: replay-reduced | replay-reduced | different | 1500 ms scheduling allowance |
| replay-speed | Actual reveal-plan chronology, item boundaries and reveal rate | replay | same | item indices and IDs exact; rate 1e-8 items/s |
| status-ready | Status is hidden after a nonempty graph arrives | default | same | exact |
| status-empty | Empty-filter status text and zero nodes | empty | same | exact |
| status-local | No-active-file local status and zero nodes | local-empty | same | exact |
| status-loading | Initial Mapping this project… status before data delivery | default | same | exact |
| truncation | Truncation cap count and notice text | cap | same | exact |
| live-save | Live save updates visible IDs | save | same | exact |
| retention-save | Survivor world-coordinate displacement at update delivery: save | save | same | 0.05 world unit at delivery |
| live-rename | Live rename updates visible IDs | rename | same | exact |
| retention-rename | Survivor world-coordinate displacement at update delivery: rename | rename | same | 0.05 world unit at delivery |
| live-delete | Live delete updates visible IDs | delete | same | exact |
| retention-delete | Survivor world-coordinate displacement at update delivery: delete | delete | same | 0.05 world unit at delivery |
| new-placement | New file distance from its surviving hub neighbour at delivery | save | same | 34 world units; independent seeds within a 24-unit jitter square |
| buffer-200 | Unsaved-buffer visible IDs after 200 ms | buffer | different | exact |
| buffer-600 | Unsaved-buffer visible IDs after 600 ms | buffer | same | exact |
| buffer-host-debounce | Full-window edit debounce and local graph following tab changes | default | same | 150 ms timer dispatch allowance; flags exact |
| font-live | Changing font family changes the focused label pixels | font | same | exact |
| resize | Dock-sized widget resize preserves centre and mapped graph | resize | same | 0.1 logical px |
| hidden | No frames advance during one second while unmapped | hidden | same | exact |
| shown | Showing the hidden widget restores a nonempty mapped graph | shown | same | exact |
| idle-cpu | Two-second settled CPU ticks (100 ticks per second on this machine) | idle | same | 2 ticks; page descendants included on reference side |
| persistence | Persisted blob restores search, panel, zoom and all graph preferences | persist | same | 1e-8 numeric; strings/flags exact |
| persistence-restart | Real preferences restored by a fresh full-window process | default | same | 1e-8 zoom; flags exact |
| persistence-delay | Real preference and zoom writes after their debounce delays | default | same | 150 ms dispatch/write allowance; early-write checks exact |
| pan-visible | Count of on-screen markers after the background drag | pan | same | 8 markers; independently placed scenes |
| click-visible | Graph remains nonempty and mapped after pointer/keyboard actions | key-escape-again | same | exact |
