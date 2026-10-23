# Graph behaviour audit

Phase A observes the running reference widget and impro's independent native
widget. It does not alter either graph implementation. The oracle inputs are
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

The measured row register, individual tolerances and explicit coverage gaps are
appended after the paired run. Raw values live in the run's `comparison.json`;
`DIVERGENCES.md` ranks every measured difference. An unmeasurable verdict is a
coverage gap, never a passing result.
