# impro native graph view

This specification records impro product decisions for exploring a project of
notes, source files and their references. Values are selected for a readable
project overview and straightforward controls. The implementation is `crates/impro-graph`: pure engine modules plus a native
GTK DrawingArea and GTK controls. It creates no WebKit view or web process.
`crates/impro-graph/src/prefs.rs`, engine tests and GTK view tests define the executable contract.

## Initial view and settings

A new graph includes notes, source files, missing references and unlinked files.
Tag nodes and referenced assets are opt-in to avoid crowding a code project.
Direction markers start enabled because import and reference direction matters.
The controls panel starts open, with the scope section expanded; other sections
start collapsed. Search starts empty, colour rules start empty, zoom starts at
1, and reduced motion starts disabled (the host accessibility preference can
override it). Incoming and outgoing traversal both start enabled. The saved engine preference starts at two hops, showing immediate dependencies
and their neighbours; the application Local graph command explicitly starts at one.

| Setting / control label | Default | Range | Control step |
| --- | --- | --- | --- |
| `textFadeMultiplier` / Label detail | 0.2 | -1 to 2 | 0.1 |
| `nodeSizeMultiplier` / File marker size | 1.15 | 0.5 to 3 | 0.05 |
| `lineSizeMultiplier` / Connection width | 1.25 | 0.5 to 4 | 0.25 |
| `centerStrength` / Pull toward centre | 0.12 | 0 to 0.6 | 0.02 |
| `repelStrength` / Space between files | 6 | 0 to 24 | 0.5 |
| `linkStrength` / Connection tension | 0.35 | 0 to 1.5 | 0.05 |
| `linkDistance` / Preferred spacing | 160 | 48 to 480 | 8 |
| `scale` / Camera zoom | 1 | 0.04 to 6 | continuous |
| `localDepth` / Neighbourhood hops | 2 | 1 to 6 | 1 |

The 160-unit preferred spacing leaves room for short filenames. Modest centre
pull keeps disconnected files nearby. Linear force controls make the effect of
a slider change predictable: centre and link strengths are used directly;
repulsion is minus 120 times its control value, including zero for disabled
repulsion. The spacing range permits both compact and loose layouts.

The keys above are persisted inside the `graph` object in impro settings;
redundant local-view keys are dropped. Unknown settings are discarded. Invalid
numbers and wrong types use defaults, finite numbers clamp to bounds. Only hop
counts are rounded to integers; other persisted numbers remain continuous.
Slider steps affect interaction, not saved-value quantization. Up to twelve
ordered colour rules are accepted. New rules use hex colours, stepping around
the hue wheel by 137.5 degrees from a blue starting hue (210 degrees), with
65% saturation and 64% dark / 42% light luminance. Existing packed RGBA colours
remain accepted for saved preferences.

## Controls and accessibility

The canvas is labelled “Project files and references”. Before data arrives the
status reads “Mapping this project…”; an empty result reads “No files match this
view”. A capped result announces “View limited to N files with the most
connections”, where N is the visible file count.

Scope controls use “Filter project files”, “Tag markers”, “Referenced assets”,
“Hide missing targets”, “Unlinked files”, and “Source and imports”. In local
mode, “Neighbourhood hops”, “Referenced by”, and “References to” replace the
unlinked-file control. Numeric controls use the labels in the settings table.
“Reset graph preferences” restores filter, display, force and colour-rule
defaults, plus traversal defaults in local mode, while retaining search text,
zoom and section visibility. “Replay project history” is the timeline button's
accessible label.

A file's menu offers “Open file”, “Open another tab”, “Open in right pane”,
“Locate in project tree”, and “Copy file location”, in that order. These labels
express project actions; host action identifiers remain stable.

## Project data and filters

The index walks regular files without following symlinks. Markdown text and
supported source text are read up to 1 MiB; binary/invalid text is skipped.
Default ignored directories are `.git`, `.hg`, `.svn`, `__pycache__`,
`node_modules`, `.venv`, `venv`, `.mypy_cache`, `.pytest_cache`, `.tox` and `.idea`.

Notes (`.md` and `.markdown`) contribute wikilinks, aliases, heading/block
targets, embeds, relative Markdown links, inline and frontmatter tags. Heading
and block references connect to the containing file. Source parsers recognize
Python imports (`.py`), JavaScript/TypeScript imports, exports-from and requires
(`.js`, `.ts`, `.jsx`, `.tsx`, `.mjs`, `.cjs`, `.vue`), CSS imports/URLs
(`.css`, `.scss`, `.sass`), and HTML source/link references (`.html`, `.htm`).
Fenced code and inline-code masking keep Markdown examples out of the link index.

Notes use Markdown references and tags; source files contribute imports. Assets
appear only when referenced and enabled. A missing reference is a synthetic
node, and can be hidden. Search supports AND terms, OR clauses, negation,
quoted text, regular expressions and path/file/tag/content fields. Strict UI
filtering tests each file or tag; missing targets remain until their references
are removed. A local focus always survives search. Disabling unlinked files
removes nodes with no surviving edges. The first nonempty matching colour rule
colours a file; tags and missing targets retain their kind colours.

Resolution is case insensitive. Wiki paths try project root then source folder;
Markdown paths try source folder then root. Python imports try root, source,
then src. Filename matches precede aliases. Ambiguous names prefer the source
folder, shortest path, then lexical order. References with the same endpoints
and kind collapse with a count; self references remain in data. The renderer
uses distinct directed endpoint pairs for degree, counting a self pair twice,
and draws one line per unordered non-self pair with direction markers.
Local traversal uses breadth-first layers and the chosen incoming/outgoing
directions. These are impro file-exploration policies.

## Drawing and motion

Marker radius before zoom is `multiplier * min(24, 6 + 2*log2(1 + degree))`.
Negative degree is treated as zero. Logarithmic growth keeps hubs legible without
hiding nearby files. Screen radius multiplies this by the square root of zoom,
so markers shrink more slowly than spacing when zooming out. Label opacity is
`clamp((zoom - 0.12)/0.7 - detail*0.25, 0, 1)`. Connection width stays constant
in screen pixels. Direction opacity is `clamp((zoom - 0.18)/0.62, 0, 1)`;
dart length is `8*sqrt(width multiplier)` pixels, half-width is
`4*sqrt(width multiplier)`, and notch depth is `6*sqrt(width multiplier)`.
Label font size is
`12 + radius*0.2`, with a 224-unit wrap width and 12-unit hover offset.

Simulation runs at 60 ticks/second. Cooling reaches 0.002 after 240 ticks from
1; a structural edit reheats to 0.4. Velocity loses 45% per tick. Barnes-Hut
opening threshold is 0.65 and minimum squared repulsion distance is 64.
Collision uses a 32-unit radius and 0.7 correction strength, reserving a
64-unit gap for labels. New files start in a disc with 72 squared units of area
per file (5184 square units); files next to surviving neighbours use at most
24 units of jitter on either axis. Existing positions survive updates.

Alpha/colour easing approaches by 16% per 60 Hz frame, zoom by 18%. Pan velocity
filters with 20% new input and retains 86% per frame. Frame-independent easing
caps elapsed time at 50 ms. Wheel zoom is `exp(-pixelDelta/480)`, with line and
page input converted to 24 and 640 pixels; small control-wheel deltas double.
Wheel zoom toward a target below the current displayed zoom keeps the canvas
centre fixed; otherwise it keeps the world point under the pointer fixed.
This anchor rule also applies to control-wheel gestures. Hover leaves unrelated
items at 28% opacity. A click allows six pixels of movement. Local marker
weight is `12/(layer+1)`, keeping even the outer hop visible. Timeline reveal
uses chronological file order when timestamps exist, at `clamp(items/12,8,72)`
items per second, with reveal progress sampled at 30 Hz independently of the
display refresh rate. Reduced motion skips visual easing.

The host supplies theme roles. A standalone native view falls back to impro slate
background (#17212b), pale text (#e4edf5), blue accent (#63b3ed), green tags,
yellow assets, muted missing targets and cyan code markers. Focus and hover
colours precede rule colours, then kind colours, then ordinary file colour.

## Native rendering and performance

The view paints software-rasterised markers, anti-aliased connections and arrows
into premultiplied ARGB32 surfaces, then composites them with Cairo. Pango labels
use cached alpha-mask surfaces at integer device-pixel font sizes so a settled
label does not depend on the preceding zoom path; nominal font sizes and
wrapping stay continuous. Offscreen primitives are culled. Zoom-based label/arrow detail and dense
label decluttering limit work, preserving focus/hover labels. A per-frame label
render budget bounds cache misses. Connection surfaces are reused when geometry
and style are unchanged and invalidated when positions, camera or appearance
change. Settled redraw timings therefore differ from moving-scene costs.

The engine uses flat position/velocity arrays, Barnes–Hut repulsion and spatial
collision handling. Layout ticks allocate no memory after graph load. Indexing,
resolution and graph building run on cancellable workers; the current native view
advances bounded layout steps on GTK's frame clock. It attaches a tick callback
only for layout cooling, camera movement, replay or colour/display easing, removes
it when settled, and pauses while unmapped. No graph workers start before a graph
opens. Preference writes use a background write-behind thread.

The graph is capped at 3,000 nodes, favouring highly connected files and reporting
truncation in the status. Reported release measurements on the four-core VM under
Xvfb: settled zoomed-in draw at 3,000 nodes, 3.3 ms median / 6.5 ms p95; layout tick,
1.7 ms; open-to-first-frame for 600 files, 36–55 ms. These are measured fixture
results, not promises for every project or viewport. See
[ARCHITECTURE.md QA helpers](ARCHITECTURE.md#qa-helpers) for reproduction.

## Window integration and persistence

Either dock's + chooser can host a graph alongside terminal and Git panes, sharing
the per-dock capacity. View → Graph view, Ctrl+Alt+G and the palette reuse the right
graph. View → Local graph or Ctrl+Shift+Alt+G opens the active file at one hop;
the engine preference defaults to two hops and the panel supports one through six.
Local traversal follows editor tab changes. The type menu also offers Note graph,
restricted to `.impro/Notes`. Rebuild graph rescans; Reset camera recentres at zoom
1 and restarts the layout. Fit frames the existing layout.

Click a file to open it; Ctrl+click or middle click requests another tab. Context
menu open modes currently converge on the normal tabbed editor, including the
right-pane request. Tags focus explorer search; unresolved targets with safe,
simple names create a note in the project root. Drag a marker to pin it during
interaction, drag empty space to pan, and scroll to zoom about the pointer.
Arrow keys pan, Shift speeds movement, plus/minus zoom, Enter opens the hovered
file, and Esc closes the menu or controls panel before clearing hover. Application
palette, quick switcher and settings shortcuts remain available.

Saved files, editor buffer changes (debounced 400 ms), explorer operations and Open
Folder feed the index. Deltas preserve positions of surviving nodes. Hidden views
defer work until mapped. The active file is highlighted; themes, accent, fonts and
Reduce motion update in place. Preferences include zoom, panel and section states,
filters, forces, groups and traversal; saves debounce for 250 ms, normalize values,
and merge through core's atomic settings store. Camera pan and the layout itself
are not a persisted editor session.

Saved zoom is restored within the supported bounds and survives graph rebuilds.
After the initial nonempty layout settles, a restored zoom that leaves every
marker off-screen is replaced by a fit of the graph. Visible saved zooms stay
unchanged. Pointer or camera-key input cancels this one-time startup recovery.

Replay reveals files in creation-time order, using stable index order for missing
timestamps, then each file's outgoing references. It is a visualization of indexed
file timestamps and links, not a replay of Git commits. The replay button restarts
playback; reduced motion removes easing. Group drag-reordering, touch pinch,
hover document previews and a separate neighbour-links switch are not implemented.

## Verification

Engine tests cover parsing, resolution, strict filtering, query errors, bounded
preferences, deltas, deterministic placement, finite convergent layouts, pinned
nodes, repulsion accuracy, directional traversal and timeline endpoints. GTK tests
cover native controls, input, theme/state, persistence and lifecycle. Run:

```sh
G_DEBUG=fatal-criticals bash scripts/test.sh -p impro-graph
```

`crates/impro-graph/examples/graph_demo.rs` is an interactive host;
`graph_bench.rs` reports frame and layout timings. The application-level
`window_qa` example checks hosting and complete-window flows. Neither the graph
renderer nor its tests require a browser bridge.
