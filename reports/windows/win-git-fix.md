# win-git-fix

STATUS: PARTIAL — the launcher is hardened and a one-loop diagnostic is in place, but the root cause of the Windows
`unknown-revision` failures is **not proven** (it can only be observed on the runner). Everything else is done.

## Hypotheses and how the diagnostic discriminates them

`cargo test -p impro-gitviews --test windows_git_diag -- --nocapture` prints, for a repository created by the test
fixture in the runner's temp directory:

| # | Hypothesis | Discriminating output |
|---|------------|-----------------------|
| 1 | The child inherits an invalid/closed stdin and the MSYS git runtime aborts | `handles:` line (`type=0` = invalid/absent, `1` disk, `2` console, `3` pipe) and variants **A vs B** (and **E vs F**): same env, only stdin differs (`inherit` vs `null`). A fails and B succeeds => hypothesis 1 |
| 2 | "dubious ownership" (`safe.directory`) because the product uses user/system git config | `rev-parse --git-dir, default config` vs `... isolated like the fixture` (stderr shows `detected dubious ownership` and the `safe.directory` advice), `git config --get-all safe.directory`, and variants **B vs D** (same stdin, product env vs fixture env that disables global/system config) |
| 3 | `^{commit}` / `--end-of-options` mishandled by MSYS2 git | `argument handling` block: the same repo with the product environment and null stdin, with and without `--end-of-options` / `^{commit}` |
| 4 | HOME / config differences | printed `HOME`, `USERPROFILE`, `GIT_*`, `MSYSTEM`, `git config --show-origin --list`, and the `config differences` block (product env plus only `GIT_CONFIG_NOSYSTEM`, only `GIT_CONFIG_GLOBAL=NUL`, only a `core.hooksPath` override) |
| 5 | `CREATE_NO_WINDOW` creation flag | variants **E/F** vs **A/B** (same stdin and env, flag added) |

Also printed: every `git.exe` that `CreateProcess` would find, in search order (MSYS2 `/usr/bin/git` vs Git for
Windows), and `git --version`.

Final assertion: `rev-parse --verify --end-of-options HEAD^{commit}` through the shared launcher
`impro_core::process::command` (null stdin, `CREATE_NO_WINDOW`, product environment) must succeed and print a 40-hex id.

### Runbook for the CEO

Run on the runner (MSYS2 UCRT64 shell, repo root):

```
cargo test -p impro-gitviews --test windows_git_diag -- --nocapture
```

Read the `== summary ==` line first: `A(inherit/product)=.. B(null/product)=.. C(inherit/fixture)=.. D(null/fixture)=..`.

| Outcome | Meaning | Next step |
|---------|---------|-----------|
| A=false, B=true, final assertion passes | Hypothesis 1: inherited stdin. The new default null stdin is the fix. Re-run `test_gitdiff`: all should pass | none |
| A=false, B=false, C/D=true; stderr of A/B mentions `dubious ownership` | Hypothesis 2: the repo owner differs from the runner user. Do **not** add `safe.directory=*`. Fix the fixture instead (create the repo as the current user, or have the test pass `-c safe.directory=<that one repo path>` only for its own fixture calls) | report back; product code stays strict |
| A=false, B=false, C/D=true, stderr is not about ownership | Config/HOME difference (hypotheses 2/4): see the `config differences` block to find which single setting flips the result | report the setting |
| everything false, argument block shows only the variants without `^{commit}` or `--end-of-options` succeed | Hypothesis 3: argument handling in this git build | report which form works; the product would need an alternative spelling |
| everything true, final assertion passes | The failure is specific to the test harness environment (e.g. repos made by a different step/owner) rather than to the launch; compare with the `test_gitdiff` run using `IMPRO_GIT_DEBUG=1` | see below |
| final assertion fails | The shared launcher itself is wrong on Windows; the F/E lines and stderr give the reason | report |

For the product tests themselves, `IMPRO_GIT_DEBUG=1 cargo test -p impro-gitviews --test test_gitdiff -- --nocapture`
prints one line per failed git run: `impro: git failed: [<args>] status=<code> stderr=<first 400 bytes>`; that names the exact
git error behind every `unknown-revision`.

## What changed

- `crates/impro-core/src/process.rs`: `command()` now defaults `stdin` to `Stdio::null()` on every platform
  (still `CREATE_NO_WINDOW` on Windows). Tests: default is the null device, override with a pipe works.
  All users were reviewed (`grep process::command`): `log.rs` git runner (never reads stdin), `log.rs` `/bin/kill` /
  `taskkill` watchdog (stdout/stderr null, no stdin use), `gitstatus.rs` x2 (already `stdin(Stdio::null())`). None relies on
  inherited stdin. The previously inherited stdin was the product's only difference to the fixture in stdio setup.
- `crates/impro-gitviews/src/log.rs`: `IMPRO_GIT_DEBUG=1` captures git's stderr (head of 400 bytes, the rest drained so
  git never blocks) and prints one diagnostic line per failed invocation (non-zero exit, spawn error). Unset or any other
  value: stderr stays `Stdio::null()`, behaviour unchanged. Windows only: the `rev-parse --show-toplevel` and branch
  outputs also strip a trailing `\r` (`trim_eol`); Unix behaviour is byte-identical.
- `crates/impro-gitviews/tests/windows_git_diag.rs` (new, `#![cfg(windows)]`): the diagnostic described above.
- Fixture portability (`tests/support/mod.rs`, `tests/test_gitdiff.rs`, `tests/test_gitlog.rs`): `NUL` and an empty
  directory replace `/dev/null` for `GIT_CONFIG_GLOBAL` and `core.hooksPath` on Windows (Unix unchanged);
  the "machine paths" test uses legal look-alikes on Windows (`"`->U+201C/U+201D, newline->U+240A, tab->U+2409,
  `:`->U+A789, `\`->U+FF3C; spaces, Cyrillic, `[1]` and the leading dash are kept) and the original names under `cfg(unix)`;
  the fake-`git` script tests (python/sh/`/proc`) are `cfg(unix)`. New test
  `test_git_debug_reports_failed_invocations_only_when_enabled` covers the diagnostics on Linux.
- Not changed: `impro-explorer/src/gitstatus.rs` (item 5 found no defect, see below).

## Static review of other git callers (item 5)

- `gitstatus.rs`: stdin/stdout/stderr already explicit; Windows `run` uses a channel reader; output paths are normalised
  (`git_path`, CRLF trimming) and compared after `canonicalize` on both sides (verbatim `\\?\` handled in
  `parse_porcelain`). No change needed.
- `-C <path>` with verbatim paths: `impro-gitviews` never canonicalizes the repository root before passing it to
  `git -C` (it passes the host's path unchanged). Its only `canonicalize` calls compare two canonicalized paths
  (open-file check), so verbatim prefixes cannot reach git from this crate. A host passing a `\\?\C:\...` root would
  rely on git accepting verbatim prefixes; that is unverified and is not stripped here (Unix-neutral, but
  speculative). The diagnostic does not test it; add a variant if the runner shows trouble.
- CRLF: git writes LF to pipes; the product parses `\n`-separated `-z`/format output. Only the two `trim_end_matches('\n')`
  sites were made `\r`-tolerant on Windows. `watch_paths` already uses `trim()`.
- Not verified without a Windows run: process-tree kill via `taskkill /F /T /PID`, and `mpsc` timeouts, are unchanged.

## How verified

- `scripts/windows/check.sh -p impro-core -p impro-gitviews -p impro-explorer --all-targets`: clean.
- `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh -p impro-core -p impro-gitviews -p impro-explorer --all-targets -- -D warnings`: clean.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` (Linux): clean.
- Linux: `env -u WAYLAND_DISPLAY scripts/ci/run-tests.sh all` (unit, integration, doc, js) exit 0, no failing target,
  including the new `test_git_debug_reports_failed_invocations_only_when_enabled` and the `impro-core` process tests;
  Node git-page tests 19/19. The known flaky `impro-markdown` timing test did not fail.

## Open issues / assumptions

- Root cause A cannot be reproduced on Linux; the default null stdin is the leading fix candidate, not proven. Run the
  runbook to confirm. If the cause is ownership, only the fixture may change, never git's safety settings.
- Windows test results for the rest of the `impro-gitviews` suite (sandbox fixtures, fault-injection) are unknown until the next runner run.
