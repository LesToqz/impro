# ci-core: pull-request gate

STATUS: DONE

`.github/workflows/ci.yml` (`name: CI`) is the pull-request gate. It runs on `pull_request`, on `push` to `main` and on `workflow_dispatch`. It could not run on GitHub, because nothing was pushed. Everything below was checked locally or statically (`actionlint`, `shellcheck`, YAML parse).

## Files

| File | Purpose |
| --- | --- |
| `.github/workflows/ci.yml` | The gate: six jobs, one section per job |
| `.github/actions/setup-linux-gui/action.yml` | Composite action: apt packages, toolchain, `CARGO_BUILD_JOBS`/`CARGO_INCREMENTAL`, cache, optional nextest |
| `.config/nextest.toml` | Profiles `default` and `ci` |
| `scripts/ci/run-tests.sh` | `unit\|integration\|doc\|js\|all`, used by the workflow and by humans |
| `scripts/ci/smoke.sh` | Release-binary smoke test |

## Jobs

| Job | Runs |
| --- | --- |
| `lint` | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `shellcheck scripts/ci/*.sh` (blocking); shellcheck of the other `scripts/**/*.sh` and `packaging/install.sh` (`continue-on-error`); Python `ast.parse` of every tracked `*.py` |
| `msrv` | `cargo check --workspace --all-targets --locked` on toolchain `MSRV` (1.88) |
| `test` | `scripts/ci/run-tests.sh` `unit`, `integration`, `doc`, `js` as separate steps; uploads `target/nextest/ci/junit.xml` if present |
| `build-release` | `cargo build --release --locked -p impro --example window_qa --example startup_floor`; uploads the three binaries as `release-bin` |
| `smoke` | downloads `release-bin`; `scripts/ci/smoke.sh`; `scripts/test-install.sh` with the real binary; `desktop-file-validate`; uploads the first-paint PNG |
| `ci-success` | `needs` all of the above (one per line, with the placeholder comment for the perf job); `if: always()`; fails unless every result is `success` (`toJSON(needs)` passed via an env var) |

`smoke` checks out only and installs runtime libraries (`libgtk-3-0t64 libvte-2.91-0 libwebkit2gtk-4.1-0`), so it needs no toolchain. `smoke.sh` honours `IMPRO_SMOKE_OUTPUT_DIR` to keep the PNG for upload.

## Spike: nextest 0.9.149 versus cargo test

Prebuilt `x86_64-unknown-linux-gnu` release. The sha256 matched the published checksum file. Each run is `run-tests.sh all` with the first version of the `ci` profile (`retries = 1`, `test-threads = 4`). The `all` run stops after `integration` fails, so doc and js did not run in these three runs.

| Run | Wall | unit | integration | Passed after retry (flaky) | Failed after retry |
| --- | --- | --- | --- | --- | --- |
| nextest 1 (4 threads) | 252 s | 198 pass | 1072 pass, 14 skipped | 6 | 20 |
| nextest 2 (4 threads) | 245 s | 198 pass | 1071 pass, 14 skipped | 7 | 21 |
| nextest 3 (4 threads) | 243 s | 198 pass | 1072 pass, 14 skipped | 2 | 20 |
| `scripts/test.sh --no-fail-fast` (cargo) | 563 s | all pass (includes doc tests) | | | |

**Rule outcome: nextest was red 3 times in a row, so the default runner is `cargo`.** This follows the selection rule used for this work: nextest is the default only if it is green three times in a row. `IMPRO_CI_RUNNER=nextest` stays available.

First failure, and the cause:

```text
impro-settings-ui::test_settings_dialog test_card_radius_and_row_metrics
  assertion `left == right` failed   left: [0, 0, 0]   right: [24, 33, 38]
impro-markdown::test_seltoolbar test_first_line_uses_space_below_and_buttons_are_compact
  assertion `left == right` failed   left: Top   right: Bottom
```

- 17 tests failed in all three runs. Each passes alone under cargo and nextest, and passes with `--test-threads 1`.
- The cause is four concurrent GTK test processes (nextest runs one process per test) on one Xvfb with no window manager. Focus, geometry and pixel-capture tests interfere with each other.
- cargo does not hit this because it runs the tests of one binary on one GTK thread and runs binaries one at a time.
- I added no `[test-groups]`. The `ci` profile uses `test-threads = 1` instead. This differs from the brief's `test-threads = 4`, because 4 is known to fail.
- Serial nextest `all` (`IMPRO_CI_RUNNER=nextest`, `test-threads = 1`): green, 0 retries, 831 s.

Tests that needed a retry under nextest with 4 threads (not needed with cargo or serial nextest):

- `table_widgets::test_two_hundred_rows_build_in_bounded_slices_and_type_fast` (the 4 ms slice assertion) failed once in run 1 and passed on retry.
- `test_slashmenu::test_every_command_is_one_undo_and_keeps_text` was flaky in all 3 runs.
- The remaining flaky tests, by run:
  - run 1: `test_markdown_blocks::test_divider_and_multiline_paste`, `test_search::test_entry_geometry_native_clear_icon_and_focus_ring`, `test_search_wiring::test_typing_shows_results_and_escape_restores_tree`, `test_slashmenu::test_trigger_rules`
  - run 2: `test_markdown_window::test_selection_toolbar_window_shortcut_precedes_the_explorer_accelerator`, `test_markdown_window::test_selection_toolbar_setting_loads_and_notes_toggle_applies_live`, and others in the same pixel and focus group
  - run 3: one other test in the same group
- Slowest single test: `impro::test_graph_click menus_markers_panel_and_modes_leave_the_graph_in_view`, 52.8 s under 4-way contention. The `ci` profile uses `slow-timeout = { period = "60s", terminate-after = 3 }`.

## Default runner (cargo): three green runs

`bash scripts/ci/run-tests.sh all` (cargo, `G_DEBUG=fatal-criticals`), exit 0 each time: 577 s, 625 s, 567 s. A fourth run with the final script (after the `--test '*'` fix below) also passed, in 570 s. There were no retries (cargo has none).

## Suite split

| Count | Tests |
| --- | --- |
| `unit` (`kind(lib) \| kind(bin)`) | 198 |
| `integration` (`kind(test)`) | 1092 |
| `cargo nextest list --workspace` | 1290 |

198 + 1092 = 1290. The 14 `#[ignore]` benchmarks are not in the nextest list. Under cargo the integration step lists 1106 (1092 + 14 ignored). The cargo runner uses `--lib --bins` and `--test '*'`, so no target runs twice. `--tests` would also re-run the 198 unit tests, and I fixed that. `doc` runs 2 doc tests and `js` runs 16 Node tests.

## Local timings (informational, warm `target/`, 3 jobs, 12 CPUs)

| Command | Wall |
| --- | --- |
| `cargo fmt --check` | 1 s |
| `cargo clippy --all-targets -D warnings` | 26 s |
| MSRV `cargo +1.88.0 check --workspace --all-targets --locked` (separate target dir) | 50 s |
| `run-tests.sh all` (cargo) | about 570 s |
| `cargo build --release ... --example window_qa --example startup_floor` | 73 s |
| `smoke.sh` | 0.7 s |
| `test-install.sh` (real binary) | 0.2 s |

A cold runner will be slower. The first run on the runner populates the cache only on `main`.

Check results:

- fmt, clippy `-D warnings`, `actionlint` and `shellcheck` on `scripts/ci/*.sh` are clean.
- All tracked `*.py` files parse.
- **MSRV 1.88.0: passes.** There is no lockfile finding.
- `smoke.sh` passes. It printed `first-paint-ms=338.149`, with a PNG of 8,278 bytes.
- `desktop-file-validate` exits 0 but prints one hint, suggesting the `Utility` category alongside `TextEditor`.

## Pinned actions

The newest release at least 7 days old was taken; the dates are as of 2026-10-24.

| Action | Version | SHA |
| --- | --- | --- |
| `actions/checkout` | v7.0.1 | `3d3c42e5aac5ba805825da76410c181273ba90b1` |
| `actions/upload-artifact` | v7.0.2 | `cf430e030ddbb5b0abf93d22962f4752f3646cd9` |
| `actions/download-artifact` | v8.0.2 | `9000827ccba6bdab643e8b6fd33ac0654aef8333` |
| `Swatinem/rust-cache` | v2.9.2 | `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` |
| `taiki-e/install-action` | v2.87.29 | `8cca0918f8471c33515202b73f56ab27f2bed283` |

`rust-cache` v2.9.3 and `install-action` v2.87.30 and later are newer than the 7-day threshold, so they were not used. `install-action` installs `cargo-nextest@0.9.149`.

## shellcheck findings in the other scripts (report only, not edited)

- `scripts/graph-lib-bench/bench-gui.sh`:
  - line 6: SC2164 (`cd` without `|| exit`)
  - line 23: SC2086 (unquoted `$size`)
  - line 26: SC2086 (unquoted `$size`)
- `scripts/graph-lib-bench/bench.sh`:
  - line 7: SC2164 (`cd` without `|| exit`)
- All other scripts (`scripts/test.sh`, `scripts/test-install.sh`, the other `run.sh`, `compare.sh`, `timing.sh`, `packaging/install.sh`) are clean.

## Risks on the real runner

- **Package names.** I verified the package names with `apt-cache policy` on this machine, which is Ubuntu 26.04, not 24.04. On 24.04 they should be the same, with the `t64` suffix on `libgtk-3-0t64`, but that is unverified.
- **Fonts.** `fonts-dejavu-core fonts-noto-core` are installed, but the exact versions on 24.04 may differ from here, and the pixel and layout tests are font-sensitive.
- **Disk.** Debug test builds of 12 crates plus the release build and apt packages are large. The runner has about 14 GB free, which should suffice, but I have not measured it.
- **WebKit sandbox.** The tests have not run on a runner with a restricted user-namespace policy. Ubuntu 24.04 restricts unprivileged user namespaces, and WebKit's bubblewrap sandbox may fail there. If so, the fix is a workflow setting that I have not tested. The environment variable `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1` is the usual escape hatch.
- **Timing tests.** `table_widgets::test_two_hundred_rows_build_in_bounded_slices_and_type_fast` asserts a 4 ms slice and may fail on a loaded runner. The cargo runner has no retry for it; the nextest `ci` profile has `retries = 1`.
- **Test job runtime.** The cargo runner takes about 10 minutes warm, so the cold-cache run will be longer. The job's `timeout-minutes` is 60.
- **JUnit artifact.** The default cargo runner writes no `junit.xml`, so the upload finds no file (`if-no-files-found: ignore`). nextest is installed in the `test` job but unused by default. Only the nextest runner (`IMPRO_CI_RUNNER=nextest`) writes the JUnit file.
- **Cache saving.** The cache is saved only on `main` (`save-if`), so pull requests restore but never save.

## Open issues

- A faster, parallel nextest setup would need one Xvfb (and session bus) per test process, for example a nextest wrapper script. I did not build one; it is outside the brief.
- `RUST_VERSION` 1.99.1 is the toolchain installed on this machine and is pinned deliberately.
