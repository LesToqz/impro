# win-smoke-fix

STATUS: DONE (pending runner confirmation)

## What was wrong
`smoke.ps1` passed `/DIR=<path>` and `/LOG=<path>` to `Start-Process -ArgumentList` unquoted. `-ArgumentList` joins
array elements with spaces and adds no quoting, so the deliberate `install dir` (space) was truncated to
`...\install`; Inno exited 0 and installed elsewhere, hence "missing installed impro.exe".

## What changed
- `packaging/windows/smoke-args.ps1` (new): `ConvertTo-QuotedArgument` (CommandLineToArgvW rules) and
  `New-InnoArgumentList` (builds `/DIR="..."`, `/LOG="..."`, `/TASKS=...`; drops a trailing backslash inside `/DIR`).
- `packaging/windows/smoke.ps1`: dot-sources the helper; installer and uninstaller arguments come from it.
- `packaging/windows/test_smoke_args.ps1` (new): plain-assert tests of the helpers.
- `.github/workflows/windows.yml`: `check` job gets a "PowerShell script tests" step (parses every tracked `.ps1`, runs the
  args test); `test` job steps source `scripts/package/windows/use-git-for-windows.sh` first.
- `scripts/package/windows/use-git-for-windows.sh` (new): puts Git for Windows (`$PROGRAMFILES/Git/cmd`) ahead of the
  MSYS2 git and logs `command -v git`, `git --version`, `git config --show-scope --show-origin --list | head -20`.
- `packaging/windows/README.md`: one table row.

Other `.ps1` review: `Invoke-Capture` already uses `ProcessStartInfo.ArgumentList` (.NET quotes); `reg.exe query` keys,
`ISCC` `/D...` arguments and `where.exe` are called with `&` (PowerShell 7 quotes values with spaces itself), and
runner paths there contain no spaces anyway. No other change needed. Inno task names `desktopicon,contextmenu,addtopath`
match `impro.iss`. The uninstaller relaunches itself from a temp copy, so `-Wait` may return early; the script already
polls with `Wait-Gone`.

## Verified
- PowerShell 7.6.8 (official tarball, SHA-256 checked against `hashes.sha256`) under `~/.cache/impro-pwsh/`.
- Parser on all 4 tracked `.ps1` files (smoke.ps1, smoke-args.ps1, test_smoke_args.ps1, build-installer.ps1): 0 errors.
  The new workflow step body was also run locally: passes.
- `pwsh -NoProfile -File packaging/windows/test_smoke_args.ps1`: all passed (9 asserts).
- `python3 -m unittest discover -s packaging/windows -p 'test_*.py'`: OK (4 skipped).
- actionlint (with shellcheck): clean; `zizmor --offline --min-severity medium .github`: no findings; shellcheck on
  `scripts/package/windows/*.sh`: clean. No `uses:` added or changed.

## Unverified until the runner
Inno really parsing `/DIR="...install dir"`; PATH assertions; uninstaller cleanup timing; GUI launch/screenshot and
whether GUI-subsystem stdout reaches the pipe (only a warning if not); `$PROGRAMFILES/Git/cmd` existing on windows-2025
(the script fails loudly if not).

## First places to read for `smoke` / `smoke-standin`
1. The `[smoke] installing:` line and `install.log` in the `windows-smoke` / `windows-standin-smoke` artifact (check `Dir:`/`App directory`).
2. The `registry after install` group and the `tree` line (file counts).
3. The `crash evidence` group, then `launch-*.stdout/stderr.txt` in the artifact.
