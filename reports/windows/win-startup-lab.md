STATUS: DONE (CI-side changes verified statically; every runtime outcome is UNVERIFIED until the runner)

## What changed
- `.github/workflows/windows.yml`
  - `test` job split into five independent report-only steps (`continue-on-error: true`, each ends with `RESULT <name>: PASS|FAIL`, also appended to the step summary): pure logic (`impro-core`; `impro-gitviews --lib --test test_gitlog --test test_gitdiff --test windows_git_diag -- --nocapture`), `impro-graph --lib`, GUI targets of the whole workspace (`--no-fail-fast`, GTK-init limitation noted in a comment), WebView2 host test, ConPTY tests (`impro-vte --test windows_conpty`, `impro-terminal --test windows_panel`). Every step sources `use-git-for-windows.sh` first (prints the git in use); steps that spawn node also source `use-node.sh`.
  - `smoke-standin`: new first step "Startup lab" (before any other launch). `smoke`: "Startup lab (report only)" after the required steps. `check` job also runs `test_lab_helpers.ps1`.
- `scripts/package/windows/use-node.sh` (new): finds `node.exe` via `cmd.exe //c "where node.exe"`, then `%PROGRAMFILES%\nodejs`, then `$RUNNER_TOOL_CACHE/node/*/x64`; prepends its dir as an MSYS path, logs `node --version`, fails with `::error::` when absent.
- `packaging/windows/startup-lab.ps1` (new), `lab-helpers.ps1` (new, pure helpers), `test_lab_helpers.ps1` (new), `smoke.ps1`, `README.md` ("Startup lab" section).
- `crates/impro-ui/examples/packaging_standin.rs`: with `IMPRO_QA_TIMING` prints `app-gtk-startup-ms=` (process start to `gtk::init` done) and `STANDIN-ENV` lines echoing the four lab variables.

## smoke.ps1
- Launches the product twice (`cold`, `warm`), each a fresh process; logs `first-paint-ms` / `app-gtk-startup-ms` / exit for both, saves `first-paint-<mode>-cold|warm.png` and `launch-<mode>-cold|warm.*.txt`.
- Both launches finish and are logged before anything fails. The screenshot verdict is the cold one; the warm one is judged only when the cold one fails, and both states go into the failure message.
- Blank check is now decode-based (System.Drawing, `LockBits`, C# pixel loop): fail when < 1 % of pixels differ from the top-left pixel; numbers always logged; the 5000-byte floor is only a `::warning::` hint.

## Experiment matrix (startup-lab.ps1, real user profile except row 8)
1 default (cold, first launch on the machine) / 2 default (warm) / 3 `PANGOCAIRO_BACKEND=win32` / 4 `DBUS_SESSION_BUS_ADDRESS=disabled:` / 5 `GSETTINGS_BACKEND=memory` / 6 `FC_DEBUG=1` (first 40 lines kept, printed in a log group and saved) / 7 cache listings before and after row 1 (`%LOCALAPPDATA%`, `%LOCALAPPDATA%\fontconfig`, `%TEMP%`, bundle `var\cache`, `etc\fonts`, `fc-cache -v` only if `fc-cache.exe` is in the bundle; else growth of LOCALAPPDATA/TEMP) plus a diff / 8 extra: default with fresh HOME/APPDATA/LOCALAPPDATA (what `smoke.ps1` does per run) / 9 extra: default again as a drift check.

## Reading the `$GITHUB_STEP_SUMMARY` table
Columns: run, `first-paint-ms`, `app-gtk-startup-ms`, exit (n/a = line not printed); wall time per launch follows the table. Compare variants with row 2, not row 1.
- Row 1 >> row 2 and `app-gtk-startup-ms` carries the gap: one-time work before/inside GTK init (fontconfig, D-Bus, schemas). Gap only in `first-paint-ms`: themes/rendering.
- Row 3 as fast as row 2 while row 1 is slow is not enough; the variant must bring a COLD start down. This table only has variants after the warm run, so a variant tells you the cost it removes from the warm path. To prove a fix for the cold path, use rows 6/7/8: cache directories grow in row 7 => fontconfig builds its cache on first run; fix in the product: prebuild the cache at install time (run `fc-cache` in the installer `[Run]` or ship the cache in the bundle `var\cache\fontconfig` and point `FONTCONFIG_FILE`/`FONTCONFIG_PATH`), or set `PANGOCAIRO_BACKEND=win32` in the product if row 3 is clearly faster on the warm path as well.
- Row 4 faster than row 2 (and stderr no longer has the `win32 session dbus binary not found` warning): set `DBUS_SESSION_BUS_ADDRESS=disabled:` early in the product's `main` (Windows only).
- Row 5 faster: use the memory backend or bundle a keyfile/`GSETTINGS_BACKEND` setting.
- Row 8 slow again while row 2 is fast: a per-profile cache (under LOCALAPPDATA/APPDATA) is the cost, which also means `smoke.ps1` numbers always look cold.
- Row 9 far from row 2: the machine drifted; discount small differences.

## How verified
- `pwsh` 7.6.8 parser over every `.ps1` (tracked + new): 0 errors. `test_smoke_args.ps1`: all passed. `test_lab_helpers.ps1`: all passed (timing parser, table, Limit-Lines, C# pixel diff incl. stride padding).
- `python3 -m unittest discover -s packaging/windows -p 'test_*.py'`: OK (4 skipped, pre-existing).
- actionlint: clean. `zizmor --offline --min-severity medium .github`: no findings. shellcheck 0.11 on `scripts/package/windows/*.sh`: clean. `use-node.sh` exercised locally with a fake `cmd.exe`/`cygpath` (found and not-found paths).
- `scripts/windows/check.sh -p impro-ui --examples` and the clippy variant (`-D warnings`): clean. Linux `cargo clippy -p impro-ui --all-targets -- -D warnings`, `cargo fmt --all --check`, forbidden-word grep, `git diff --check`: clean.

## UNVERIFIED until the runner
- Everything that runs on Windows: `startup-lab.ps1` end to end (install, launches, snapshots, `Compare-Object` output), the `System.Drawing` decode path (`Get-ImageStats`; only the pixel loop was tested here), `use-node.sh` against the real image, the GITHUB_STEP_SUMMARY writes from the msys2 shell, and the new test-step structure.
- Whether the product prints `app-gtk-startup-ms` (only the stand-in does; the product rows show `n/a` otherwise). The bundle may not ship `fc-cache.exe`.

## Open issues / assumptions
- The product lab step runs after the required smoke steps, so its "cold" row is not machine-cold; the product's true cold number is the first launch of `smoke.ps1` (installer mode).
- In `smoke-standin` the lab installs/uninstalls its own copy before `smoke.ps1` runs; an uninstalled app dir leaves any fontconfig cache in the profile, so later smoke launches there are no longer machine-cold (by design: the lab goes first).
