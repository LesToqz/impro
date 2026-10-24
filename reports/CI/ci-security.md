# ci-security report

STATUS: PARTIAL

PARTIAL because (1) the `[bans] wildcards` check cannot be set to `deny` without a `Cargo.toml` change (see Open issues), and (2) the repository-wide `git grep -n '/home/'` gate is not empty because of files outside this task's ownership (see Open issues). Everything owned by this task is done and verified locally as far as possible.

## Files

`deny.toml`, `.github/workflows/security.yml`, `.github/dependabot.yml`, `reports/CI/ci-security.md`. No `.gitleaks.toml` (no findings, so no allow-list needed) and no `.github/zizmor.yml` (no findings, so no suppressions needed).

## Workflow `Security` (`security.yml`)

Triggers: `pull_request`, `push` to `main`, weekly `schedule` (`23 3 * * 1`, Monday 03:23 UTC), `workflow_dispatch`. Top-level `permissions: contents: read`. Concurrency group `workflow-ref`, cancelling only PR runs. Every checkout uses `persist-credentials: false`, every job has `timeout-minutes`, and every action is pinned by commit SHA.

| Job | What it does |
| --- | --- |
| `cargo-deny` | Installs `cargo-deny@0.20.2` through `taiki-e/install-action`, then runs `cargo deny --locked check advisories bans licenses sources`. |
| `gitleaks` | Full-history checkout (`fetch-depth: 0`). Downloads the official `gitleaks` 8.30.1 release tarball, verifies its SHA-256 against the release's checksums file, and runs `gitleaks git --redact` over the history. |
| `workflow-lint` | Installs `actionlint` 1.7.12 (checksum-verified against the release checksums file) and `zizmor` 1.31.1 (prebuilt binary, SHA-256 pinned in the workflow, because zizmor publishes no checksums file). Runs `actionlint`, then `zizmor --min-severity medium .github` (online audits enabled through the job's read-only `github.token`). |
| `codeql` | Matrix `rust`, `javascript-typescript`, `build-mode: none`, category `/language:<lang>`. Only this job has `security-events: write`. Code Scanning default setup must stay disabled in the repository settings, because this workflow is the advanced setup. |

### Pinned versions and SHAs

| Component | Version | Pin |
| --- | --- | --- |
| `actions/checkout` | v7.0.1 | `3d3c42e5aac5ba805825da76410c181273ba90b1` |
| `taiki-e/install-action` | v2.87.29 | `8cca0918f8471c33515202b73f56ab27f2bed283` |
| `github/codeql-action` (init, analyze) | v4.39.0 | `6f2ac81f5cd81fe351a197ba0b00d91d03cd2aee` |
| `cargo-deny` | 0.20.2 | via install-action |
| `gitleaks` | 8.30.1 | checksums file from the release |
| `actionlint` | 1.7.12 | checksums file from the release |
| `zizmor` | 1.31.1 | SHA-256 `d185f726…e138c99` |

Each action is the newest release at least 7 days old on 2026-10-24 (newer releases exist for `taiki-e/install-action` v2.87.30 and 31, and `github/codeql-action` v4.39.1). `actionlint`, `gitleaks` and `cargo-deny` are the latest releases. All three action SHAs were dereferenced through `gh api` (tag objects followed).

## `deny.toml`

Schema: cargo-deny 0.20.2. Graph restricted to `x86_64-unknown-linux-gnu`. `yanked = "deny"`, `unmaintained = "all"`. Sources: crates.io only, unknown registry/git denied. `multiple-versions = "warn"` (18 duplicate-crate warnings today, all from the gtk-rs/syn/thiserror transitive graph).

### Ignore list (final)

All four entries carry a reason and "review by 2027-04-24".

| Advisory | Crate | Why ignored |
| --- | --- | --- |
| RUSTSEC-2024-0429 | `glib 0.18.5` | Unsound `VariantStrIter`. The stack is pinned to gtk-rs 0.18 by design and the repo never iterates `glib::Variant`. |
| RUSTSEC-2024-0370 | `proc-macro-error 1.0.4` | Unmaintained; compile-time dependency of `glib-macros` and `gtk3-macros` (pinned stack). |
| RUSTSEC-2025-0141 | `bincode 1.3.3` | Unmaintained (no vulnerability); only via `syntect`, bundled syntax data. |
| RUSTSEC-2024-0320 | `yaml-rust 0.4.5` | Unmaintained (no vulnerability); only via `syntect`, bundled `.sublime-syntax` files. |

**Proof the gate catches them.** With the ignore list removed (`--config` pointing at a copy without `ignore`), `cargo deny check advisories` fails and reports exactly those four ids on those four crates. With the committed file, no "advisory not detected" warnings appear, so the list is complete and minimal. The committed run prints `advisories ok, bans ok, licenses ok, sources ok` and exits 0.

**Differences from the expected list in the brief.** The GTK3-binding "unmaintained" advisories (RUSTSEC-2024-0412/0413/0416 and family) are **not reported** by cargo-deny 0.20.2 against the current RustSec DB, even with `unmaintained = "all"`, so they are not in the list (deriving from real output, not guessing). Conversely, `proc-macro-error`, `bincode` and `yaml-rust` were not anticipated. The last two come from `syntect`, **outside the gtk-rs/webkit2gtk stack**; they are informational "unmaintained" notices, not vulnerabilities, with no safe upgrade offered, so I ignored them with documented reasons. If you prefer not to ignore anything outside the stack, drop those two entries and the gate goes red until `syntect` is replaced or updated.

### Licence allow-list

`MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `Unicode-3.0`. This is the minimum set that satisfies every crate in the graph: `Apache-2.0 WITH LLVM-exception` is needed by `target-lexicon` (build time only), `Unicode-3.0` by `unicode-ident` (`(MIT OR Apache-2.0) AND Unicode-3.0`). Dual-licensed crates (`MIT OR Unlicense`, `MIT OR Zlib OR Apache-2.0`, `0BSD OR MIT OR Apache-2.0`) are satisfied through MIT, so `Unlicense`, `Zlib` and `0BSD` are deliberately not allowed. `impro-gitviews` has no `license` field; a `[[licenses.clarify]]` entry in `deny.toml` declares it as `MIT` (the workspace licence) without editing `Cargo.toml`.

## gitleaks (v8.30.1, `--redact`, local run)

| Scope | Commits scanned | Findings |
| --- | --- | --- |
| Full history, all refs (includes local `ceo/**` branches) | 306 | 0 |
| `main` only | 293 | 0 |

No rule ids, files or commits to report; no allow-list needed.

## Local verification

- `actionlint` on `security.yml`: clean, exit 0.
- `zizmor --min-severity medium .github` with online audits: "No findings to report", exit 0. The first run flagged two medium `dependabot-cooldown` findings, fixed with `cooldown: default-days: 7`.
- `security.yml` and `dependabot.yml` both parse (actionlint and zizmor parsed them; `deny.toml` parsed by cargo-deny).
- `cargo deny --locked check advisories bans licenses sources`: exit 0.
- Forbidden-word grep: prints nothing. No absolute local paths in the files I added.

## Not verifiable offline

- CodeQL: nothing was run. On the first real run, check that `rust` with `build-mode: none` is accepted, that both categories upload, and that Code Scanning default setup is disabled (otherwise the upload is rejected).
- The actual runner: `taiki-e/install-action` pulling `cargo-deny@0.20.2`, the `gitleaks` and `actionlint` checksum steps (checksum-file line format was verified locally with the same `grep | sha256sum --check` logic), and cargo-deny's advisory DB fetch on the runner.
- zizmor ran with my own token locally; on the runner it uses `github.token`. A new zizmor audit introduced by an upgrade may turn the job red.
- Dependabot: `dependabot.yml` was only parsed, not processed by GitHub. The `ignore` list covers `atk atk-sys cairo-rs cairo-sys-rs gdk gdk-pixbuf gdk-pixbuf-sys gdk-sys gio gio-sys glib glib-macros glib-sys gobject-sys gtk gtk3-macros gtk-sys javascriptcore-rs javascriptcore-rs-sys pango pangocairo pangocairo-sys pango-sys soup3 soup3-sys webkit2gtk webkit2gtk-sys` (names taken from `Cargo.lock`; `soup3` and the macro crates are part of the same pinned family).

## Open issues

1. **Wildcard bans are `warn`, not `deny`.** The workspace crates do not set `publish = false`, so cargo-deny treats their `path` dependencies as wildcards on public crates, and `allow-wildcard-paths = true` has no effect (12 workspace crates error under `deny`). Verified on a scratch copy: adding `publish = false` to every crate makes `bans` pass with `wildcards = "deny"`. Needs a `Cargo.toml` change by its owner (for example `publish = false` in each crate), then flip `wildcards` to `"deny"` in `deny.toml`.
2. **`git grep -n '/home/'` is not empty on this branch**, from files I do not own: `scripts/graph-compare/{README.md,DIVERGENCES.md}`, `scripts/graph-lib-bench/{README.md,bench.sh}`, and about 20 `scripts/graph-lib-bench/results/*.md` files contain absolute local paths (some also contain `<home>/` and a scratch directory name). Two test fixtures under `crates/impro-gitviews/tests/fixtures/` contain harmless sample paths (`/home/dev/...`). These must be cleaned before the repository is published.
3. **`scripts/graph-lib-bench/`** is a standalone Cargo project with its own lockfile and is not covered by `cargo deny`, Dependabot (`directory: /` only) or CodeQL's Rust analysis of the workspace. Add a second Dependabot `cargo` entry and a cargo-deny run for it if it should be covered.
4. Dependabot proposes patch updates of the pinned stack; only minor and major bumps are blocked.
5. Review the ignore list by 2027-04-24, or earlier when `syntect` or the gtk-rs stack changes.
