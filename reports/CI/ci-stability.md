# CI stability: Functional tests job on the hosted runner

STATUS: PARTIAL (round 1). Groups B and C cannot be confirmed without a run on the hosted runner.

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
