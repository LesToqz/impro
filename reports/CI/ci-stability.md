# CI stability: Functional tests job on the hosted runner

STATUS: PARTIAL (round 2). Stays PARTIAL until a green run on the hosted runner is confirmed.

## Round 1

### Failing targets

| # | Target | Group | Change made | Still unknown |
|---|---|---|---|---|
| 1 | `impro --test test_gitgraph_wiring` | A, example binary missing | `run-tests.sh integration` now builds examples first | none expected |
| 2 | `impro-gitviews --test test_gitviews` | A | same | none expected |
| 3 | `impro-terminal --test test_panesplit` | A | same | none expected |
| 4 | `impro-explorer --test test_icons` | B, icon rendering | icon theme, SVG pixbuf loader, schemas added to the package lists | whether the runner then resolves icons |
| 5 | `impro-ui --test test_icons` | B | same | same |
| 6 | `impro-terminal --test test_terminal_bar` | B | same | same |
| 7 | `impro-terminal --test test_resizegate` | C, SIGTRAP after a GLib-GObject critical | packages added; diagnose workflow added | root cause (missing package vs. older GTK 3.24.41 / VTE 0.76) |
| 8 | `impro-terminal --test test_terminal_startup` | C | same | same |
| 9 | `impro-chrome --test test_prompt` | C, SIGSEGV | same | cause of the segfault |

### Group A (script)

`scripts/ci/run-tests.sh` now runs `cargo build --examples --workspace --locked` at the start of `integration` (and therefore `all`). It is a separate step, so it applies to both the cargo and the nextest runner. The tests look for the binaries under `target/debug/examples/`, the same profile.

Verification from a state without the example binaries (`target/debug/examples/webkit_host_qa*` and `window_qa*` deleted):

- Before: `scripts/ci/run-tests.sh integration -p impro --test test_gitgraph_wiring ...` gave 13 `FAILED` tests, each `test_gitgraph_wiring.rs:16:10: called Result::unwrap() on an Err value: Os { code: 2, kind: NotFound ... }`.
- After (examples deleted again first): `test_gitgraph_wiring` `13 passed; 0 failed; 1 ignored`, `test_gitviews` `5 passed; 0 failed`, `test_panesplit` `14 passed; 0 failed`.
- Because `--workspace` in the script takes precedence over `-p`, that run executed the whole integration suite; every target passed locally, including `test_icons`, `test_resizegate`, `test_terminal_startup`, `test_prompt` and `test_terminal_bar` (the development machine has the icon themes and newer libraries).
- `bash scripts/ci/run-tests.sh unit`: exit 0, 14 targets `ok`.

### Groups B and C (packages)

Added to the `gui` package list of `.github/actions/setup-linux-gui/action.yml` and to the runtime lists of the `smoke` and `perf-budget` jobs in `.github/workflows/ci.yml` (`--no-install-recommends` kept):

```
librsvg2-common adwaita-icon-theme yaru-theme-icon
gsettings-desktop-schemas at-spi2-core shared-mime-info
```

All six names exist (`apt-cache policy`). The runner is Ubuntu 24.04, where `yaru-theme-icon` is in `universe`; the runner image enables it, but this was not checked there.

### Diagnose workflow (temporary)

`.github/workflows/ci-diagnose.yml`, triggered by `workflow_dispatch` or by a pull request touching only that file. It must be deleted once the failures are understood. It prints to the log only:

1. Versions of GTK, VTE, GLib, Pango, Cairo, gdk-pixbuf, librsvg, icon themes, AT-SPI, schemas, WebKitGTK; registered SVG pixbuf loader; `/usr/share/icons`; font count.
2. Builds the three crashing test binaries and the examples.
3. Runs each crashing binary under `xvfb-run -a dbus-run-session --` three ways (`G_DEBUG=fatal-criticals`; `G_DEBUG` unset with `--nocapture`; `--test-threads 1`), printing exit codes.
4. For binaries that still crash serially: installs `gdb` and prints `bt` and `thread apply all bt 12` for the first failing test, with `G_DEBUG=fatal-criticals`, plus a variant with `G_SLICE=always-malloc`.
5. Re-runs the icon tests with `--nocapture`.

Expected to show: (a) whether the new packages fix groups B and C; (b) if a crash remains, whether it depends on the fatal-criticals setting, on test parallelism, or on memory handling; (c) the call stack at the failing critical or segfault.

### Open questions

- If the crashes persist with the packages installed, are they caused by the older GTK/VTE (a test or product issue), in which case a test fix is needed (not in this round's scope)?
- If criticals are harmless when not fatal, should `G_DEBUG=fatal-criticals` be kept for the hosted runner?
- The AT-SPI bus warning is expected to remain; `at-spi2-core` provides the bus launcher but the tests do not start it.

## Round 2

### Runner evidence (from the first pull-request run)

- Groups A and B and the `impro-chrome test_prompt` SIGSEGV are fixed by round 1. Two targets still failed: `impro-terminal --test test_resizegate` and `--test test_terminal_startup`.
- With `G_DEBUG` unset both pass on the runner (20/20 and 12/12). They abort only under `G_DEBUG=fatal-criticals`, on exactly this pair of GLib messages emitted during teardown: `invalid (NULL) pointer instance` and `g_signal_handlers_disconnect_matched: assertion 'G_TYPE_CHECK_INSTANCE (instance)' failed`.
- gdb: the last reference to an `impro_vte::Terminal` is dropped in `impro_terminal::resizegate::stop_waiting` (from `ResizeGate::unrealize`); libvte's own dispose code (VTE 0.76.0) then calls `g_signal_handlers_disconnect_matched` with a NULL instance. A library-side teardown critical of the older VTE (development machine: 0.84, which does not emit it). Not caused by parallelism (`--test-threads 1` crashes the same way).

### Changes

- `scripts/ci/run-tests.sh` (cargo runner): the two targets are declared once at the top (`tolerated_pkg`, `tolerated_targets`). `integration` runs (a) every other target with `G_DEBUG=fatal-criticals` as before (`--workspace --exclude impro-terminal --test '*'`, plus `-p impro-terminal --test <name>` for each other terminal target) and (b) the two tolerated targets with `G_DEBUG` unset, output captured. After (b) every line containing `CRITICAL` that is not one of the two known messages (matched on message text, not pid or time) is listed and fails the script. Each part runs even if another failed; exit status is non-zero if any part failed. The nextest mode is unchanged and does not apply the tolerance (stated in a script comment and in the docs).
- `.github/workflows/ci-diagnose.yml` removed.
- `docs/ci.md`: new subsection "Known runner differences".
- No product code or test files changed.

### Filter self-test (`scripts/ci/run-tests.sh --self-test-critical-filter`)

```
self-test (i) known messages: accepted (expected)
self-test (ii) with unrelated critical: rejected (expected), offending line: (x:1): Gtk-CRITICAL **: 00:00:00.000: gtk_widget_show: assertion 'GTK_IS_WIDGET (widget)' failed
self-test (iii) empty output: accepted (expected)
```

(i) used two known messages with different pids and times; exit status of the self-test is 0.

### Local run of the split

A temporary copy of the script with the strict workspace part limited to `-p impro-explorer` (so only that crate and `impro-terminal` ran), executed through `integration`:

- Printed `integration targets: 90 in total, 2 tolerated, 88 strict`.
- Strict part: all `impro-explorer` and the six other `impro-terminal` targets ran. Tolerated part: `test_resizegate` 20 passed, `test_terminal_startup` 12 passed, no CRITICAL lines (VTE 0.84 locally).
- One strict-part failure: `test_panesplit::test_real_webkit_resize_keeps_page_state_and_pause_resume` ("GTK operation timed out") while the machine was loaded; rerun alone, `test_panesplit` gives 14 passed. Looks like a load-sensitive timeout; not related to the change.

### Target counts

`cargo metadata` lists 90 integration test targets in total (8 in `impro-terminal`). Part (b) runs 2; part (a) runs 88 (82 outside `impro-terminal`, 6 inside), so (a)+(b) = 90, the same set as before. The script prints these numbers at the start of `integration`. The full suite was not run here.

### Open issues

- Confirm a green run on the hosted runner, including that the two tolerated targets print no other CRITICAL.
- Revisit the tolerance when the runner image has a newer libvte, or when the product avoids dropping a terminal during unrealize.
