# Paired graph audit

Build the native probe once, using the machine's shared resource lock:

```sh
flock <home>/projects/impro-wt/.heavy.lock \
  systemd-run --user --scope -q -p MemoryMax=3500M -p MemorySwapMax=0 \
  cargo build --release -p impro-graph --examples
```

Then set `IMPRO_REFERENCE_DIR` to the read-only reference checkout and run:

```sh
bash scripts/graph-compare/run.sh
```

The runner acquires the lock in the checkout parent directory (the same shared
lock on this machine), clears the desktop display variables, and
creates its own 1600×1000 X11 display and D-Bus session. Do not wrap it in another
`flock` on the same lock file. It never attaches to an existing application.
Cargo uses the repository's three-job cap. The runner never builds binaries. A full run takes about 7–10 minutes after
acquiring the shared lock.

Required system tools are Xvfb, D-Bus, systemd user scopes, Python GTK3/WebKit
bindings, the GI Cairo bridge and Pillow. When the Cairo bridge is extracted to
a scratch directory outside the checkout, set `IMPRO_PARITY_GI_PATH` to the
extracted `usr/lib/python3/dist-packages/gi` directory. The existing window parity
README explains how to obtain the matching system package. This machine's audit
used `/tmp/impro-graph-bridge/usr/lib/python3/dist-packages/gi`.

Arguments:

```sh
bash scripts/graph-compare/run.sh --out .ceo/graph-compare-second
bash scripts/graph-compare/run.sh --reference /path/to/reference
bash scripts/graph-compare/run.sh --impro-bin target/release/examples/graph_probe
bash scripts/graph-compare/run.sh --reuse
```

`--reuse` recomputes the table from existing driver artifacts without opening
windows. Full runs include an additional fresh process per side to measure
initial-layout determinism. `--skip-repeat` omits these probes and records that
row as unmeasurable. To repeat the whole sequence, use another `--out` directory;
compare the complete JSON values, allowing the per-row timing tolerances.

Exit status is zero for completed comparisons, including differences and stated
coverage gaps. Driver failures, readiness deadlines, malformed artifacts, missing
prerequisites and missing binaries produce nonzero status. A completed comparison
is an audit result, not a claim of parity. Review the verdict counts and coverage
gaps before accepting changes.

`scenarios.py` generates the same 37-file fixture before each implementation runs:
36 text files and one PNG, with a 14-spoke hub, eight-file chain, five-file isolated
cluster, orphan, wiki/Markdown references, tags, missing target, source imports,
asset and two notes. A separate 3005-file fixture exercises truncation. Stable
modification times are supplied; filesystem creation times remain OS controlled.
Both sides receive the same JSON actions. Widget operations are translated only
at the driver boundary; graph algorithms are never copied into this harness.

The reference package, page API and bridge handler are discovered at runtime.
The real GTK graph host loads its unchanged page; synthetic DOM pointer, wheel
and keyboard events reach its normal listeners. The native driver uses the
public graph host API, GTK widget APIs and existing `view::testing` helpers.
Node pointer helpers reach the normal native handlers, but bypass GDK event routing.
Background drags use GTK-dispatched GDK events with monotonic timestamps; Shift
keys also carry modifiers through GTK event dispatch.
The reference page bridge events and native host signals are normalized to
`open`, `search` and `create` records. The reference's raw new-tab flag is measured
before its host consumes it. Editor tab/dock wiring is outside this harness.

Output defaults to `.ceo/graph-compare`:

- `actions.json`: exact shared action sequence.
- `fixture/` and `cap-fixture/`: generated inputs only.
- `reference/` and `impro/`: incremental `measurements.json`, driver logs and PNGs.
- `reference-repeat/` and `impro-repeat/`: fresh-start baseline evidence.
- `comparison.json`, `comparison.md`, `counts.json`: full values, table and counts.
- `behaviour-register.md`, `divergences.md`: generated documentation snapshots for review.

The committed BEHAVIOURS and DIVERGENCES documents record the reviewed audit;
running the harness does not overwrite tracked documentation.

All screenshots remain under the selected output directory and are never
committed. Configuration and caches are isolated there. Python uses `-I -B` and
bytecode writes are disabled; the reference checkout is never modified.

State measurements use settled frames with a bounded readiness wait. Replay has
a longer settle budget because reveal and cooling are sequential. Real elapsed
times are retained; a harness deadline is reported as a harness failure, without
claiming a product hang. Node RGB and opacity come from existing scene helpers.
Marker-centre RGB is also sampled from captured frames, excluding clipping,
controls and other markers; only kinds unobscured in both frames are compared. Horizontal marker radii are
also measured from those frames, allowing 1 px for boundary coverage.
Connection scenes filter to the isolated source pair and focus it before zooming.
Connection FWHM uses quarter-pixel samples across that pair,
with a 1 px tolerance for raster coverage. Label ink bounds, visibility and
peak text-role contrast are sampled around a focused orphan; font coverage is
allowed a 0.15 alpha tolerance. These raster rows remain separate from the phase-B nominal font
and arrow paint telemetry. Pixel identity and equal force-layout coordinates are not
expected between the independent renderers. Exact row tolerances appear in
`BEHAVIOURS.md` and the JSON table.

Survivor positions are sampled at graph delivery: around the reference's public
`setData` method and inside the native public updated signal. Sampling before
cooling distinguishes retained positions from subsequent simulation movement.
Reference unsaved-buffer calls reach the widget immediately; application editor
forwarding is a separate debounce layer, so the widget-level timing difference
must be investigated in the host before proposing a user-facing change.

CPU evidence counts `/proc` user+system ticks during two seconds of a blocking GLib
main loop with a single completion timeout. The idle sample does not use the
readiness polling loop. The reference includes its descendant page processes. The
short interval, timer granularity and other machine load limit precision. Hidden
frame counts, rather than aggregate CPU, establish that an unmapped view does
not advance the graph animation.

The phase-B probe also reports read-only nominal label metrics and bitmap sizes,
connection paint width/colour/alpha, painted arrow geometry/opacity, cursor names
and camera velocity. The reference observes Canvas2D paint calls in memory;
its source files and rendering algorithms are unchanged. Controlled frame
scenarios isolate zoom easing, velocity filtering and decay from event-dispatch
delays. `status-loading` captures the document-end status before data delivery.

Run the generated layout sweep with the same prerequisites and isolation:

```sh
bash scripts/graph-compare/run.sh --layout-sweep --out .ceo/layout-sweep
```

It compares hub/spokes, a chain, isolated clusters and 3000 files, with a fresh
process repeat of each implementation. `layout-results.json` records settle
time, spacing, cluster-centroid separation and maximum repeat displacement.
The large fixture skips the quadratic nearest-neighbour diagnostic.
