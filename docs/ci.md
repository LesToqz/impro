# Continuous integration

This page describes the GitHub Actions setup of impro: what runs when, how to
reproduce it locally, and how to maintain it. The workflows are in
`.github/workflows/`; helper scripts are in `scripts/ci/`.

## Overview

| Workflow | File | Triggers |
| --- | --- | --- |
| CI | `ci.yml` | pull requests, pushes to `main`, manual dispatch |
| Security | `security.yml` | pull requests, pushes to `main`, weekly (Monday 03:23 UTC), manual dispatch |
| Nightly | `nightly.yml` | daily 01:41 UTC, manual dispatch |

All jobs run on `ubuntu-24.04` with read-only token permissions. Dependabot
(`.github/dependabot.yml`) opens weekly update pull requests for Cargo and GitHub
Actions. A composite action, `.github/actions/setup-linux-gui`, installs the GTK
development packages, the pinned toolchain and the Cargo cache for the jobs that build.

### Test categories and jobs

| Category | What | Job (workflow) |
| --- | --- | --- |
| Functional | Unit tests | `Functional tests` (CI), step `unit` |
| Functional | Integration tests | `Functional tests` (CI), step `integration` |
| Functional | Doc tests | `Functional tests` (CI), step `doc` |
| Functional | Git-page JavaScript tests (Node) | `Functional tests` (CI), step `js` |
| Functional | Installer test (`scripts/test-install.sh`) | `Smoke test` (CI) |
| Functional | End-to-end smoke of the release binary | `Smoke test` (CI) |
| Static checks | rustfmt, clippy (`-D warnings`), shellcheck, Python syntax | `Lint` (CI) |
| Security | Dependency advisories, bans, licences, sources | `cargo-deny` (Security) |
| Security | Secret scanning of the full history | `gitleaks` (Security) |
| Security | Static analysis of Rust and JavaScript | `codeql` (Security) |
| Security | Workflow linting (actionlint, zizmor) | `workflow-lint` (Security) |
| Security | Dependency and action updates | Dependabot |
| Performance | Budget gate on every pull request | `Performance budget` (CI) |
| Performance | Before/after comparison against an older commit | `Performance A/B` (Nightly) |
| Performance | Long-running graph view soak test | `Graph view soak` (Nightly) |
| Reliability | Three full repeated runs to find flaky tests | `Flake hunt` (Nightly) |
| Compatibility | Minimum supported Rust version, 1.88 | `MSRV check` (CI) |
| Compatibility | Build and test on stable and beta toolchains | `Toolchain canary` (Nightly) |
| Quality visibility | Line coverage, report only (never fails on a threshold) | `Coverage` (Nightly) |

The smoke job uses the binary built by `Release build`; the performance gate uses
the same artifact.

## Required checks

`CI success` (job `ci-success` in `ci.yml`) is the only status the CI workflow
needs to expose to branch protection. It depends on `lint`, `msrv`, `test`,
`build-release`, `smoke` and `perf-budget` and fails if any of them failed or was
cancelled.

Also require the Security workflow jobs: `cargo-deny`, `gitleaks`, `workflow-lint`
and both `codeql` matrix entries. The Nightly workflow is not a merge gate.

## Running everything locally

Install the build packages listed in the [README](../README.md#install-and-run)
plus the test tooling. GUI tests need a virtual display and a D-Bus session; the
scripts below wrap the commands in `xvfb-run -a dbus-run-session --`.

```sh
sudo apt install xvfb dbus dbus-x11 nodejs desktop-file-utils fonts-dejavu-core fonts-noto-core
```

| What | Command |
| --- | --- |
| Formatting | `cargo fmt --all -- --check` |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Unit tests | `bash scripts/ci/run-tests.sh unit` |
| Integration tests | `bash scripts/ci/run-tests.sh integration` |
| Doc tests | `bash scripts/ci/run-tests.sh doc` |
| Git-page JavaScript tests | `bash scripts/ci/run-tests.sh js` |
| All of the above tests | `bash scripts/ci/run-tests.sh all` |
| Smoke test | `cargo build --release --locked -p impro --bin impro`, then `bash scripts/ci/smoke.sh target/release/impro` |
| Installer test | `IMPRO_INSTALL_BINARY=target/release/impro bash scripts/test-install.sh` |
| Dependency policy | `cargo deny --locked check advisories bans licenses sources` |
| Workflow lint | `actionlint` and `zizmor --min-severity medium .github` |
| Secret scan | `gitleaks git --redact` |

Performance gate (needs the release binary and the QA examples):

```sh
cargo build --release --locked -p impro -p impro-graph --bin impro \
  --example window_qa --example startup_floor --example graph_bench
mkdir -p /tmp/impro-perf
xvfb-run -a dbus-run-session -- python3 scripts/ci/perf_gate.py \
  --bin-dir target/release --budgets scripts/ci/perf-budgets.json \
  --output /tmp/impro-perf/perf.json
```

`--bin-dir` must contain `impro` and `examples/{startup_floor,window_qa,graph_bench}`,
which is the layout of `target/release`. Use `--budget-scale F` to tighten or loosen
all budgets for an experiment. `cargo-deny`, `actionlint`, `zizmor` and `gitleaks`
are separate tools; install them from their releases or with `cargo install`
(`cargo-deny`).

## Test runner

`scripts/ci/run-tests.sh` runs `cargo test` by default. `IMPRO_CI_RUNNER=nextest`
selects [cargo-nextest](https://nexte.st) with the `ci` profile from
`.config/nextest.toml` (one retry, serial execution, JUnit output at
`target/nextest/ci/junit.xml`).

nextest is opt-in because it did not pass in parallel. It starts one process per
test, and with four concurrent GTK processes on one Xvfb display (no window
manager) focus, geometry and pixel-capture tests interfere. Measured with
nextest 0.9.149:

| Run | Wall time | Result |
| --- | --- | --- |
| nextest, 4 threads, three runs | 243-252 s | 20-21 tests failed after retry in each run; 17 failed in all three |
| nextest, serial (`ci` profile) | 831 s | green, no retries |
| `cargo test` (default) | 567-625 s | green three times in a row |

Every failing test passes alone and with `--test-threads 1`. `cargo test` runs
the tests of one binary on one GTK thread and the binaries one after another,
so it does not hit the problem. Making nextest parallel would need one display
and session bus per test process; this is not implemented.

Suite split: 198 unit tests (`--lib --bins`), 1092 integration tests
(`--test '*'`), 2 doc tests, 16 Node tests; 14 further `#[ignore]` benchmarks are
not run by default. The `test` job sets `G_DEBUG=fatal-criticals`, which turns
GTK criticals into failures.

## Performance gate

`scripts/ci/perf_gate.py` runs in the `Performance budget` job against the
release binaries of the same run. Budgets are in `scripts/ci/perf-budgets.json`.

| Metric | Measured by |
| --- | --- |
| `startup_median_ms` | median first paint of 10 fresh runs after one warm-up |
| `startup_ratio` | `impro` startup median divided by the median of an empty GTK window, measured in the same run; a coarse guard, because the empty-window floor varies between sessions |
| `key_to_paint_ms` | median keyboard-to-paint time from `window_qa performance` |
| `graph_draw_p50_ms`, `graph_layout_tick_ms` | `graph_bench`, 3,000-node graph |
| `idle_cpu_ticks` | CPU ticks over 10 s of idle time |
| `binary_size_mib` | size of the release binary |

Semantics:

- Each metric has a `warn` and a `fail` budget. Exceeding `warn` is reported in
  the job summary and never fails the job.
- A metric group that exceeds `fail` is **re-measured once**; only a second breach
  fails the gate (exit 1). A missing binary, missing display or unparseable output
  is a harness error (exit 2).
- The budgets are **provisional** (`"provisional": true` in the file): they come
  from one fast development machine and are deliberately loose.

Recalibrating after the first runs on the hosted runner: collect the values from
five `Performance budget` runs (the summary lists every value), set each baseline
to the observed median and `warn`/`fail` to about 1.75x and 3x of it, then remove
the `provisional` flag. The absolute startup `fail` is above the 400 ms product
ceiling on purpose, to tolerate slow runners; the ratio is the stricter check.

The nightly `Performance A/B` job compares two builds of the repository. The base
is the `base` input of the manual dispatch (a commit, tag, branch or `origin/`
branch). If it is empty, the base is the last commit of `origin/main` older than
seven days, or `HEAD~1` when `main` has no commit that old. Both sides are built in
separate target directories with LTO off (relative numbers only), then
`scripts/ci/ab_perf.py` compares them. It fails when workspace first paint or the
3,000-node graph draw is more than 20% slower; the other metrics are only reported.
If the base cannot be built, the job exits with an error.

## Security

| Job | Coverage |
| --- | --- |
| `cargo-deny` | `cargo deny --locked check advisories bans licenses sources` with `deny.toml`: RustSec advisories, yanked crates, licence allow-list, crates.io as the only source. Duplicate crate versions only warn. |
| `gitleaks` | `gitleaks git --redact` over the full history (release binary checksum-verified). |
| `workflow-lint` | `actionlint`, then `zizmor --min-severity medium .github` for workflow security issues. |
| `codeql` | CodeQL for `rust` and `javascript-typescript` with `build-mode: none`. Only this job has `security-events: write`. |

### Advisory ignores in `deny.toml`

The GTK 3 / WebKit2GTK stack (gtk-rs 0.18, `webkit2gtk` 2.0) is pinned by design,
so advisories in that stack cannot be resolved by upgrading. Policy for the
`ignore` list:

- Every entry has a reason and a review date (`review by YYYY-MM-DD`, currently
  2027-04-24). Entries without both are not accepted.
- Ignore only what cannot be fixed by a compatible upgrade: the pinned stack and
  informational "unmaintained" notices that have no safe update.
- To add one: run the check, copy the advisory ID, add `{ id = "RUSTSEC-…", reason = "… review by …" }`.
- To remove one: delete the entry and run the check; cargo-deny warns about an
  ignore that no longer matches anything, which means it can go.

### Dependabot

Cargo and GitHub Actions are checked weekly (Monday 05:00 UTC), at most five open
pull requests each, with a 7-day cooldown after a release. Cargo minor and patch
updates are grouped in one pull request; all Actions updates are grouped. For the
pinned GTK/WebKit crates only minor and major bumps are ignored, so patch updates
are still proposed.

### Updating pinned actions

Actions are pinned by full commit SHA with the version in a trailing comment
(`uses: owner/repo@<sha> # v1.2.3`).

1. Pick the newest release that is at least 7 days old.
2. Resolve the tag: `gh api repos/OWNER/REPO/git/ref/tags/TAG`. If `object.type` is
   `tag` (an annotated tag), dereference it with
   `gh api repos/OWNER/REPO/git/tags/<object.sha>` and use the commit SHA in `object.sha`.
3. Replace the SHA and keep the `# vX.Y.Z` comment current in every workflow that
   uses the action.

### Toolchain pin

The build toolchain is `RUST_VERSION` in the `env` block of both `ci.yml` and
`nightly.yml`. Bump both together. The MSRV (`MSRV`, 1.88) matches the workspace
manifest and the README.

## Repository settings

These cannot be set from files in the repository. Apply them once:

- Actions enabled for the repository.
- Default workflow token permissions: read-only.
- A branch protection rule or ruleset on `main` requiring `CI success` and the
  Security jobs listed above.
- Code scanning **default setup must stay off**: `security.yml` is the advanced
  setup, and uploads are rejected while default setup is enabled.
- Secret scanning and push protection on.
- Dependabot alerts on.

## First-run checklist

The workflows were validated statically (`actionlint`, `shellcheck`, `zizmor`) and
by running their commands locally, not on GitHub-hosted runners. On the first runs
check:

- **Packages and fonts.** Package names were verified on a newer Ubuntu release;
  on 24.04 they should match (`libgtk-3-0t64` has the `t64` suffix). Layout and
  pixel tests are font-sensitive, so a different `fonts-dejavu-core` /
  `fonts-noto-core` version may change results.
- **WebKit sandbox.** Ubuntu 24.04 restricts unprivileged user namespaces, which
  can break WebKit's bubblewrap sandbox in the Git-page tests. If it fails, setting
  `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1` for the test step is the usual
  workaround. It disables the sandbox, so use it only on throwaway CI runners that
  open trusted repository content; it has not been tested here.
- **Disk space.** Debug builds of 12 crates, the release build and the packages are
  large; about 14 GB is free on a runner. Not measured.
- **Timing-sensitive tests.** `table_widgets::test_two_hundred_rows_build_in_bounded_slices_and_type_fast`
  asserts a 4 ms slice and may fail on a loaded runner. The cargo runner does not retry.
- **Budgets.** Provisional; recalibrate as described above.
- **CodeQL.** Confirm that `rust` with `build-mode: none` is accepted, that both
  categories upload, and that default setup is off.
- **Nightly durations.** `perf-ab` builds twice from scratch without the Rust cache
  (first runs may take up to about 40 minutes; limit 75). Check that `soak`
  (600 s run, limit 30 min), `flake-hunt` (three full runs, limit 150 min) and
  `coverage` (limit 90 min) fit.
- **Cache.** The Rust cache is saved only on `main`; pull requests restore it.
- **Artifacts.** `release-bin` must contain `impro` and `examples/graph_bench`.
  The JUnit upload finds no file with the default cargo runner.

## Measured wall times

Wall times have **not been measured on a hosted runner yet**. Local runs on a
12-core machine with a warm build cache, for orientation only: `run-tests.sh all`
about 570 s, release build of the binary and QA examples 73 s, performance gate
about 38 s, smoke test under 1 s.

| Job | Cold cache | Warm cache |
| --- | --- | --- |
| Lint | | |
| MSRV check | | |
| Functional tests | | |
| Release build | | |
| Smoke test | | |
| Performance budget | | |
| Security (each job) | | |
| Nightly: Performance A/B | | |
| Nightly: Soak | | |
| Nightly: Flake hunt | | |
| Nightly: Coverage | | |
