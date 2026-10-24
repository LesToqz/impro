# CI performance gate and nightly workflow

STATUS: DONE

## What changed

- `scripts/ci/perf_gate.py`, `scripts/ci/perf-budgets.json`: the budget gate.
- `scripts/ci/ab_perf.py`: before/after comparison used by the nightly job.
- `.github/workflows/nightly.yml`: `perf-ab`, `soak`, `flake-hunt`, `toolchain-canary` (stable, beta), `coverage`.
- `.github/workflows/ci.yml`: new `perf-budget` job, `ci-success` needs entry, `build-release` command and artifact list (diff below).

**Bug found in the existing `build-release` job.** `cargo build --release -p impro --example ...` builds only the examples, not the `impro` binary. Locally, `target/release/impro` stayed the stale warm-cache file (759 KB) until `--bin impro` was added; the real binary is 12.19 MB. Without `--bin impro` the artifact upload would fail on a clean runner or ship a stale file. The new command passes `--bin impro`.

## Metrics chosen

| metric | source | why |
| --- | --- | --- |
| `startup_median_ms` (p90 reported) | 10 fresh `impro` runs after one warm-up, `first-paint-ms` | the charter figure (ceiling 400 ms) |
| `startup_ratio` | impro median / `startup_floor` median, interleaved in the same run | independent of machine speed; the strict startup check |
| `key_to_paint_ms` | `window_qa performance` JSON, median of open/switch/toggle | machine-readable JSON, no text scraping; already built |
| `graph_draw_p50_ms`, `graph_layout_tick_ms` | `graph_bench`, 3,000-node section | pure CPU, stable (spread under 2%), parsed with anchored regexes |
| `idle_cpu_ticks` | `/proc/<pid>/stat` utime+stime over 10 s, 3 s after start | warn above 5, fail above 30 |
| `binary_size_mib` | size of `impro` | warn +8%, fail +15% |

Only `graph_bench` was added to `build-release` (it is gated). `window_qa` and `startup_floor` were already there.

Rejected: `editor_perf` (many free-form `key=value` lines of different shapes rather than one stable figure per run), explorer `perf` (results split between stdout and stderr in free-form text) and `git_perf` (only skimmed: free-form output, and Git scenarios depend on repository fixtures). I judged none of them reliably parseable without changing the crates, which is out of scope. `window_qa` also reports `idle_ticks`; it is kept as a detail only, because the separate idle measurement follows the brief.

Rules implemented: a `fail` breach re-measures the whole metric group once and only a second breach fails (exit 1); `warn` never fails; a missing binary, missing display or unparseable output exits 2. `--budget-scale F` multiplies warn and fail.

## Calibration (5 local runs, release build, Xvfb, nothing else running)

| metric | run 1 | run 2 | run 3 | run 4 | run 5 |
| --- | ---: | ---: | ---: | ---: | ---: |
| startup median (ms) | 221.45 | 220.13 | 218.49 | 219.06 | 219.36 |
| startup p90 (ms) | 224.85 | 224.99 | 222.19 | 221.81 | 222.48 |
| empty-window median (ms) | 93.55 | 95.50 | 94.40 | 94.63 | 94.79 |
| startup ratio | 2.37 | 2.30 | 2.31 | 2.31 | 2.31 |
| key to paint (ms) | 15.79 | 15.51 | 16.35 | 15.82 | 16.63 |
| graph draw p50 (ms) | 1.81 | 1.82 | 1.81 | 1.80 | 1.83 |
| layout tick (ms) | 1.71 | 1.69 | 1.73 | 1.72 | 1.71 |
| idle ticks | 0 | 0 | 0 | 0 | 0 |
| binary (MiB) | 11.63 | 11.63 | 11.63 | 11.63 | 11.63 |

Budgets (`provisional: true`):

| metric | baseline | warn | fail |
| --- | ---: | ---: | ---: |
| startup_median_ms | 220 | 385 | 660 |
| startup_ratio | 2.31 | 4.0 | 7.0 |
| key_to_paint_ms | 15.8 | 28 | 47 |
| graph_draw_p50_ms | 1.81 | 3.2 | 5.4 |
| graph_layout_tick_ms | 1.71 | 3.0 | 5.1 |
| idle_cpu_ticks | 1 | 5 | 30 |
| binary_size_mib | 11.63 | 12.56 | 13.37 |

The idle baseline is 1 (the README figure), not the observed 0, so a ratio is never taken against zero.

## Acceptance results

- Five gate runs against the committed budgets, all exit 0. Startup medians (ms) / ratios: 220 / 2.31, 219 / 2.34, 218 / 2.32, 216 / 2.29, 219 / 2.33.
- `--budget-scale 0.2`: exit 1, every group re-measured once, then failed.
- Missing binary directory: exit 2. Directory without binaries: exit 2.
- Local run time of the gate: 37-38 s (warm-up, 2x10 startup runs, a 15 s `window_qa performance` run, `graph_bench`, 13 s idle window). Expect 1.5-3x on the runner, so about 1-2 minutes, plus a possible second pass.
- `ab_perf.py`, same build on both sides (`--runs 10 --frame-rounds 2`): exit 0 in 88 s, ratios 0.99-1.02. With the empty-window example standing in for the old `impro`: workspace startup 93 ms -> 219 ms (2.34x), exit 1. With a base directory lacking binaries: exit 2.
- `actionlint` on `ci.yml` and `nightly.yml`: nothing in `nightly.yml` or in my `ci.yml` changes. It reports two shellcheck warnings (SC2046) in the pre-existing `lint` job (lines 49 and 51), which I did not touch. `zizmor --offline --min-severity medium .github`: no findings. `py_compile` clean. No shell scripts were added.
- The four hygiene greps print nothing. `git diff --stat main...HEAD` lists only owned files.

## A/B base

`workflow_dispatch` input `base` accepts a commit, tag, branch or `origin/` branch. If empty: `git rev-list -n 1 --before='7 days ago' origin/main`, falling back to `HEAD~1` when `main` has no commit that old. The base is exported with `git archive` and built in its own target directory, like head, with `CARGO_PROFILE_RELEASE_LTO=off` and `CODEGEN_UNITS=16` on both sides (relative numbers only). If the base cannot be built the job writes a note to the summary and exits 2. `ab_perf.py` gates (exit 1) on workspace first paint and the 3,000-node graph draw being more than 20% slower; the empty window, chrome, layout tick and key-to-paint are reported only. It reuses `startup_bench.py` unchanged (it stages `startup_floor` beside `impro` in a temporary directory to fit the script's before-layout) and `perf_gate.Gate` for the frame metrics.

## Not verifiable offline / look at in the first real runs

- Runner speed: the budgets come from a fast development machine. After the first five `perf-budget` runs, set baselines to the observed medians and warn/fail to about 1.75x/3x, then drop `provisional`. The summary prints every value. The absolute startup fail of 660 ms is above the charter ceiling of 400 ms on purpose, to tolerate slow runners; the ratio is the strict check.
- `perf-budget`: that the `release-bin` artifact now contains `impro` and `examples/graph_bench`, and that the idle process on the runner stays near 0-2 ticks.
- `perf-ab`: two cold builds with LTO off, no dependency cache (separate target directories), so the first runs may approach 40 minutes (job limit 75). `CARGO_PROFILE_RELEASE_LTO=off` was not exercised locally.
- `soak`: whether the 600 s run plus the release test build fits in 30 minutes; `--include-ignored` works whether or not the test is marked ignored.
- `flake-hunt`: log parsing is by `test ... FAILED` and Node `not ok N - name` lines; build failures show as "failed without a recognised test name". Three full runs may need the 150-minute limit.
- `coverage`: `cargo-llvm-cov` with the GTK tests under Xvfb; the report steps run even if tests fail.
- `toolchain-canary`: new lints on `stable` or `beta` will show as red runs.

## Open issues

- `ci.yml` and `nightly.yml` both pin `RUST_VERSION`; they must be bumped together.
- The nightly `perf-ab` job does not benefit from the Rust cache because it uses custom target directories.

## `ci.yml` diff

```diff
diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml
index e68e28f..5aad1c3 100644
--- a/.github/workflows/ci.yml
+++ b/.github/workflows/ci.yml
@@ -121,7 +121,9 @@ jobs:
           nextest: "false"
           cache-key: release
       - name: Build release binary and QA examples
-        run: cargo build --release --locked -p impro --example window_qa --example startup_floor
+        run: >-
+          cargo build --release --locked -p impro -p impro-graph
+          --bin impro --example window_qa --example startup_floor --example graph_bench
       - name: Upload binaries
         uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2
         with:
@@ -130,6 +132,7 @@ jobs:
             target/release/impro
             target/release/examples/window_qa
             target/release/examples/startup_floor
+            target/release/examples/graph_bench
           retention-days: 3
           if-no-files-found: error
 
@@ -173,6 +176,51 @@ jobs:
           retention-days: 3
           if-no-files-found: ignore
 
+  # -------------------------------------------------------- perf-budget
+  # Gross-regression gate: startup (absolute and relative to an empty GTK
+  # window), frame timings, idle CPU and binary size. See scripts/ci/perf_gate.py.
+  perf-budget:
+    name: Performance budget
+    needs: build-release
+    runs-on: ubuntu-24.04
+    timeout-minutes: 25
+    steps:
+      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
+        with:
+          persist-credentials: false
+      - name: Install runtime packages
+        run: |
+          sudo apt-get update
+          sudo apt-get install -y --no-install-recommends \
+            libgtk-3-0t64 libvte-2.91-0 libwebkit2gtk-4.1-0 \
+            xvfb dbus dbus-x11 desktop-file-utils \
+            fonts-dejavu-core fonts-noto-core
+      - uses: actions/download-artifact@9000827ccba6bdab643e8b6fd33ac0654aef8333 # v8.0.2
+        with:
+          name: release-bin
+          path: ${{ runner.temp }}/release-bin
+      - name: Make binaries executable
+        run: chmod +x "$RUNNER_TEMP"/release-bin/impro "$RUNNER_TEMP"/release-bin/examples/*
+      - name: Performance budgets
+        env:
+          GDK_BACKEND: x11
+          GIO_USE_VFS: local
+        run: >-
+          xvfb-run -a dbus-run-session --
+          python3 scripts/ci/perf_gate.py
+          --bin-dir "$RUNNER_TEMP/release-bin"
+          --budgets scripts/ci/perf-budgets.json
+          --output "$RUNNER_TEMP/perf/perf.json"
+          --summary "$GITHUB_STEP_SUMMARY"
+      - name: Upload performance results
+        if: always()
+        uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2
+        with:
+          name: perf-results
+          path: ${{ runner.temp }}/perf/perf.json
+          retention-days: 14
+          if-no-files-found: ignore
+
   # --------------------------------------------------------- ci-success
   # The single status that branch protection requires.
   ci-success:
@@ -183,7 +231,7 @@ jobs:
       - test
       - build-release
       - smoke
-      # the perf-budget job is added here by the performance task
+      - perf-budget
     if: always()
     runs-on: ubuntu-24.04
     timeout-minutes: 5
```
