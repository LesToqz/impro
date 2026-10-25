# win-docs

STATUS: DONE

## What changed
- New `docs/windows.md`, `docs/packaging.md`.
- `docs/ci.md`: Windows workflow, `package-linux.yml`, overview rows, local Windows type-check loop. Wall-time table untouched (no measured numbers in the reports).
- `README.md`: Install section, source-build heading, platform statements; `docs/ARCHITECTURE.md`: "Platform backends".

## How verified
- Script over backticked repo paths in the five docs: every path resolves except generated or relative ones (`dist/`, `target/...`, `emu/`, `windows/` under `impro-vte`); forbidden-word, local-path and `git diff --check` gates clean.

## Open issues / assumptions
- Cold 4-9 s / warm ~1 s first-launch numbers come from the task brief (runner measurements), not from a report.
- `IMPRO_TERM_DIAG` is not on main, so it is not documented. `IMPRO_GIT_DEVTOOLS` accepted values are not listed.
- Windows behaviour and the Windows/Linux package workflows are described as written in the reports; nothing was verified on Windows.
