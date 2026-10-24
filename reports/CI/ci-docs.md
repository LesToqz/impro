# ci-docs report

STATUS: DONE

## Sections written
- `docs/ci.md` (new): overview with workflow and category-to-job maps, required checks, local commands, test runner, performance gate, security (ignore policy, Dependabot, action and toolchain pins), repository settings, first-run checklist, wall-time table (empty cells).
- `README.md`: CI badge under the title, "Continuous integration" section after "Tests and limitations", "Licence" section at the end. Existing tables and text untouched; "Project layout" left alone (no row for non-crate paths without editing the table).

## Verified against the files
- Triggers, cron times, job names, `timeout-minutes`, `RUST_VERSION` in both workflows, `MSRV`, `ci-success` dependencies, security job names, Dependabot schedule, cooldown and grouping, `IMPRO_SOAK_SECONDS`, `perf_gate.py` usage, `run-tests.sh` modes and `IMPRO_CI_RUNNER`, nextest `ci` profile.
- No mismatch found between the reports and the files. The reports were trusted for measured numbers (nextest spike, suite split, calibration, wall times) and for facts that cannot be checked offline.
- The `ci-success` dependency list in `docs/ci.md` was taken from the report and the diff in it. The local build command for the performance gate matches the `build-release` job in the report; I did not re-open every line of that job.

## Checks
- The four hygiene greps print nothing; relative links resolve (`../README.md`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, `docs/ci.md`); table rows have matching column counts.

## Open questions
- Nothing here has run on a hosted runner. The WebKit sandbox workaround is untested.
- `nodejs` in the local package list is an assumption for the `js` mode; hosted runners ship Node.
