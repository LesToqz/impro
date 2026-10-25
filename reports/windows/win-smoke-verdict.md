STATUS: DONE

## Root cause
1. `Test-ScreenshotProblems` ended with `, $problems.ToArray()` (unary comma). The caller wraps it in `@(...)`, so the
   result was an array holding one array: `Count` was always 1, even with zero problems, and `-join` printed
   `System.String[]`. A passing screenshot therefore failed with "cold launch: System.String[]".
2. `Measure-Object Length -Sum` on an empty list, or on items without `Length`, returns an object without a `Sum`
   property; reading `.Sum` under `Set-StrictMode -Version Latest` throws. It hit `Get-DirSnapshot` in the startup lab
   (empty directory) and `Show-Tree` in smoke.

## Fix
- `packaging/windows/lab-helpers.ps1`: `Test-ScreenshotProblems` now emits one string per problem (nothing when fine);
  new `Get-ByteSum` (empty, `$null`, and Length-less safe).
- `smoke.ps1` (`Show-Tree`) and `startup-lab.ps1` (`Get-DirSnapshot`) use `Get-ByteSum`; the lab also initialises
  `$before`/`$after` so the final `if ($before -and $after)` cannot throw under strict mode.
- `test_lab_helpers.ps1` (already run by the `check` job): cases for 0/1/2 problems, real function output for a missing
  file and a non-PNG, and `Get-ByteSum` for empty/null/no-Length/scalar/list.

## Evidence
- Before (strict mode repro): `n=0 Count=1 join=System.String[]`, same for n=1, 2; and
  `The property 'Sum' cannot be found on this object` for an empty list and for items without Length.
- Old `lab-helpers.ps1` with the new tests: 3 FAIL (problem is a string, message text, non-PNG message).
- After: `pwsh -NoProfile -File packaging/windows/test_lab_helpers.ps1` and `test_smoke_args.ps1`: all passed.
- Parser: 0 errors on all 7 tracked `.ps1`. `python3 -m unittest discover -s packaging/windows -p 'test_*.py'`: OK (4 skipped).
  actionlint clean, `zizmor --offline --min-severity medium .github`: no findings. Forbidden-word grep and `git diff --check` clean.

## Unverified
- Not run on a Windows runner: the real decode path (`Get-ImageStats`, System.Drawing) and the full lab matrix reaching
  `$GITHUB_STEP_SUMMARY` are only exercised by CI. Other `.Count`/`$null` uses were re-read and found safe (all wrapped in `@()`).
- No Rust/Linux code touched, so cargo gates were not re-run.
