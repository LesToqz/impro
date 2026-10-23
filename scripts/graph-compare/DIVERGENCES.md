# Graph audit divergences

Final reviewed audit measured 2026-10-23 19:20 IST.

180 observable rows: **176 same, 3 different, 1 unmeasurable**. The register in
[BEHAVIOURS.md](BEHAVIOURS.md) preserves the measured verdicts, including accepted
divergences. Acceptance does not turn a `different` row into `same`.

## Evidence and reproduction

Read-only archive root: `<home>/projects/impro-wt/.qa-archive/`.
The final source of truth is `graph-fixes/final-reviewed/comparison.json`, with
`comparison.md`, `behaviour-register.md` and `actions.json` beside it. Driver
values are in `final-reviewed/{reference,impro}/measurements.json`, keyed by
`steps.<scenario>`. The phase-B explanations and regression results are in
`graph-fixes/report.md`; the additional layout measurements are in
`graph-fixes/layout-sweep/layout-results.json`. The phase-A source is
`graph-view-parity-a/graph-compare/comparison.json`.

For a new paired run, follow [README.md](README.md), including the full-window
executable for debounce and persistence coverage. The exact reproduction row IDs
and scenarios below refer to the archived actions. The runner owns Xvfb, D-Bus,
the shared lock and memory scope. Differences do not produce a failing exit
status. Neither the reference checkout nor user settings are written.

## Before and after

| Reviewed stage | Rows | Same | Different | Unmeasurable |
| --- | ---: | ---: | ---: | ---: |
| Phase A, 2026-10-23 13:18 IST | 175 | 151 | 12 | 12 |
| Phase B baseline on `9a47b90` | 175 | 152 | 11 | 12 |
| Phase B final, 2026-10-23 19:20 IST | 180 | 176 | 3 | 1 |

The baseline counts come from the phase-B report. Phase B added five nominal
label-alpha/camera-filter rows and measured eleven former coverage gaps with
paint telemetry, controlled frames and full-window checks. The table is not a
fixed-denominator count of repairs.

Each previously listed difference has the following final disposition. Commit
IDs below identify changes in the merged phase-B history; the pan and chronology
repairs already present at its baseline are distinguished explicitly.

| Previous row ID | Final result and change |
| --- | --- |
| `pan-visible` | Same: 31 / 31 markers. The earlier held-pan repair was already in the phase-B baseline; the graph remains visible. |
| `pan-distance` | Same: [80, 32] / [80, 32] px. Earlier held-pan repair already in the baseline prevents inertia during a held drag. |
| `pan-inertia` | Same: [61.64, 24.65] / [61.69, 24.68] px, within 8 px. Phase B `6d67146` uses the observed 20% velocity filter; paired input timestamps were aligned to 16 ms intervals, retaining frames between moves. |
| `closed-chip` | Same: true / true. `1b13924` explicitly shows the guarded chip container and children on both close paths. |
| `anchor-wheel-out` | Same: 79.7545 / 79.7545 px, within 0.1 px. `6d67146` uses the centre when target zoom is below displayed zoom, otherwise the pointer, including Ctrl-wheel. |
| `replay` | Same: identical IDs at 250 and 1000 ms. Timestamp chronology was aligned before the baseline; `546b311` samples reveal progress at 30 Hz rather than each display frame. |
| `replay-reduced` | Same: identical IDs at 250 and 1000 ms. Same chronology and 30 Hz reveal change; completion time remains a separate difference below. |
| `label-1` | Same: visible on both sides; reference ink 47×13 px, impro 46×15 px, within 3 px. `48aac24`, finalized by `a57e98f`, removes zoom-history dependence by refreshing exact settled masks with bounded warm-up. |
| `label-alpha-0.7` | Same: reference 0.753, impro 0.741, within 0.15. The same mask-cache correction fixes raster contrast without changing nominal font/fade constants. |
| `layout-clusters` | Still different: independent layout geometry; accepted with sweep evidence below. |
| `buffer-200` | Still different: accepted widget API boundary difference; full-window debounce matches below. |
| `duration-replay-reduced` | Still different: OPEN ISSUE; the reference completes sooner in the final sample, with variation between archived runs. |

## Remaining difference: independent cluster separation

Reproduction row: **`layout-clusters`**, scenario **`default`**.

Reference: **321.21 world units** from the isolated-cluster centroid to the hub.
impro: **433.60 world units**. Tolerance: **80 world units; independent layouts**.
Measured verdict: **different** (112.39 units apart).

Disposition: **accepted divergence**. The charter requires an independent native
layout; exact world positions are not a shared contract. The generated sweep
shows the separation difference changes direction with fixture size rather than
systematically spreading all native clusters farther apart:

| Fixture | Nodes | Reference / impro settle (ms) | Reference / impro nearest spacing | Reference / impro cluster separation |
| --- | ---: | --- | --- | --- |
| Hub/spokes + ring | 60 | 4755 / 4065 | 84.12 / 80.51 | 534.84 / 534.58 |
| Chain | 60 | 4562 / 4065 | 95.88 / 100.27 | Single component |
| Three islands | 90 | 4558 / 4067 | 94.42 / 98.40 | 188.54 / 301.93 |
| Thirty islands | 3000 | 5052 / 4066 | Quadratic diagnostic skipped | 960.74 / 767.40 |

Spacing and separation are world units. Every fixture on both sides has zero
maximum world-position displacement in its fresh-process repeat (eight repeat
comparisons). These are additional fixture measurements, not replacements for
the `default` row's tolerance. Phase B retained the force/layout constants.

## Remaining difference: reduced-motion replay completion

Reproduction row: **`duration-replay-reduced`**, scenario **`replay-reduced`**.

Reference: **11527 ms**. impro: **13619 ms**. Tolerance: **1500 ms scheduling
allowance**. Measured verdict: **different**. impro takes **2092 ms longer**;
the shorter final sample belongs to the reference.

Disposition: **OPEN ISSUE**. This measures reveal plus final layout cooling,
not just reveal speed. The final ordinary replay is 13814 ms reference versus
13616 ms impro (same). Both final replay plans contain 82 items at 8 items/s,
with matching item indices/IDs; `replay` and `replay-reduced` also match at both
sample times. Thus the remaining row does not establish a reveal-order or
reveal-rate defect.

The phase-B report's first full audit records approximately 13.79 / 13.85 s on
the reference for ordinary / reduced-motion replay, unlike the final reference
reduced-motion sample of 11.527 s. Phase A likewise recorded 11.483 s for the
reference while describing an earlier reference sample near 13.8 s. This is
evidence of reference completion-time variation; impro's final ordinary/reduced
times are almost identical. It supports a run-dependent cooling/timing
explanation, but does not prove that the entire final 2092 ms gap is noise or
exclude a real reduced-motion cooling difference. The archive lacks repeated,
controlled final reduced-motion completion measurements that resolve that cause.

[docs/graph-view.md](../../docs/graph-view.md#drawing-and-motion) specifies reveal
progress sampled at 30 Hz and says “Reduced motion skips visual easing.” Its
Window integration and persistence section also says reduced motion removes
easing. Neither sentence promises a shorter reveal schedule or identical total
cooling time. Keep the measured difference open pending repeated completion and
cooling traces; the evidence does not support declaring this row fixed or
accepting a specific cause.

## Remaining difference: widget buffer sample at 200 ms

Reproduction row: **`buffer-200`**, scenario **`buffer`**.

Reference: **38 visible IDs**, including `?Buffer Target`. impro: **37 visible
IDs**, without `?Buffer Target`. All other IDs are identical. Tolerance:
**exact**. Measured verdict: **different**. The complete arrays remain in the
comparison JSON; `buffer-600` matches with the new target on both sides.

Disposition: **accepted divergence; not an application defect**. The reference
widget receives `set_text` directly, after its application normally waits to
forward an edit. The native widget owns the equivalent 400 ms debounce. The
widget-only 200 ms row therefore samples different API boundaries.

The full-window **`buffer-host-debounce`** row (scenario **`default`**) is **same**:
both report `at200: false`, `after: true`, `local_follow: true`; elapsed values
are **413.362725 ms reference / 400 ms impro**, within the **150 ms timer dispatch
allowance; flags exact**. The archived window regressions also confirm unchanged
disk text. This verifies the application-level 400 ms policy stated in
[docs/graph-view.md](../../docs/graph-view.md#window-integration-and-persistence).

## Coverage gap: native Fit command

Reproduction row: **`fit-camera`**, scenario **`default`**.

Reference value: **null**. impro paired value: **null**. Recorded tolerance:
**exact**, but no paired comparison is possible. Verdict: **unmeasurable**.

Disposition: **accepted coverage gap for an extra native command**. There is no
reference Fit entry point in either the graph widget or its window. The native
Fit method frames the existing layout, as documented in `docs/graph-view.md`;
null is absence of paired evidence, not a failed native command or proof of
parity. Reset camera is separately measured and passes. This is the only
unmeasurable row; loading status, paint metrics, cursor/velocity, editor debounce,
real settings writes and fresh-process persistence are now measured in the final
register.
