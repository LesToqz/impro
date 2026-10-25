# win-foundation report

STATUS: PARTIAL

One item is blocked by the ownership rules: `crates/impro-terminal/src/resizegate.rs` needs a
one-line `#[cfg(unix)]` (patch below). Everything else is done. With that one line applied, acceptance
item 1 passes (verified on a temporary uncommitted edit, then reverted); without it,
`scripts/windows/check.sh -p impro-terminal` fails with exactly one error.

## What changed

### (A) Local Windows type-check loop — `scripts/windows/`
- `fetch-sysroot.sh` — downloads the MSYS2 UCRT64 database, resolves the dependency closure of
  gtk3, gdk-pixbuf2, cairo, pango, glib2, librsvg, adwaita-icon-theme, harfbuzz and atk,
  verifies every package against `%SHA256SUM%`, and extracts only `ucrt64/include` and the
  `pkgconfig` dirs (`--full` extracts everything). Sysroot lives in
  `~/.cache/impro-win-sysroot/<db-sha256-prefix>/` (override the base with `IMPRO_WIN_SYSROOT`),
  `current` points at the one in use. Idempotent, resumable (`curl -C -`, per-package done
  markers), no network once a sysroot is ready (`--update` refreshes it). Prints db hash, package count and disk use.
- `msys2_resolve.py` — stdlib-only resolver (`%DEPENDS%` with `>= <= = < >`, `%PROVIDES%`, pacman-style vercmp).
- `check.sh [cargo check args]` — adds the `x86_64-pc-windows-gnu` rust-std if missing, ensures the
  sysroot, exports `PKG_CONFIG_ALLOW_CROSS=1`, `PKG_CONFIG_SYSROOT_DIR`, `PKG_CONFIG_PATH`,
  `PKG_CONFIG_LIBDIR` (so a host GTK can never satisfy a probe) and `IMPRO_SKIP_WIN_RESOURCES=1`,
  then runs `cargo check --target x86_64-pc-windows-gnu "$@"` in the worktree's normal `target/`.
  `IMPRO_WIN_CARGO_CMD=clippy` swaps the subcommand (used below for Windows-target clippy).
- `IMPRO_SKIP_WIN_RESOURCES=1` is exported by `check.sh`; a later task's `build.rs` that needs a
  Windows resource compiler / icon embedding must skip that step when it is set.
- All scripts: `set -euo pipefail`, usage text, header comment, shellcheck clean (v0.11.0).

### (B) Base crates for Windows
- `impro-core`
  - `fsops.rs`: `rename_no_replace` on Windows = `MoveFileExW(src, dst, 0)` (no
    `MOVEFILE_REPLACE_EXISTING`, no `COPY_ALLOWED`); an existing target gives
    `ERROR_ALREADY_EXISTS`/`ERROR_FILE_EXISTS`, which std maps to `ErrorKind::AlreadyExists`, so the
    callers' error mapping is unchanged. Root guard is now `path.parent().is_none()` (`/`, `C:\`, UNC root).
    `trash()` on Windows moves to the **Recycle Bin** with `SHFileOperationW(FO_DELETE,
    FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_WANTNUKEWARNING)`: if the shell would delete
    permanently (too big, no bin) it asks instead of doing it silently, and a declined/failed operation
    is `TrashUnavailable`. The freedesktop `trash_into` (uid/dev checks, 0700/0600) stays Unix-only; the
    uid ownership check has no Windows counterpart and is not needed because the Recycle Bin is per-user.
  - `local_date`: libc `localtime_r` + `strftime` replaced by `LocalTime` (fields from `localtime_r` on Unix,
    `GetLocalTime` on Windows) and a small formatter for `%Y %m %d %H %M %S %F %T %%`. A Unix-only test compares it
    byte-for-byte with libc `strftime` for the formats in use.
  - new `platform.rs`: `home_dir()` (Unix `$HOME` exactly as before; Windows `%USERPROFILE%`, then `$HOME`),
    `config_root()` (Unix `XDG_CONFIG_HOME` else `~/.config`; Windows `%APPDATA%` else
    `%USERPROFILE%\AppData\Roaming`), `expand_tilde()`. Both OS rules are unit-tested on any host through an injected env getter.
    `settings::settings_path()` uses `config_root()`; the 0600 permission hardening stays `cfg(unix)`.
  - new `process.rs`: `command(program)` = `Command::new`, plus `CREATE_NO_WINDOW` on Windows.
  - `windows-sys` 0.61 (already in `Cargo.lock`) under `[target.'cfg(windows)'.dependencies]`, features
    `Win32_Foundation, Win32_Storage_FileSystem, Win32_System_SystemInformation, Win32_UI_Shell`;
    declared once in the root `[workspace.dependencies]` (root `Cargo.toml` edit, as allowed). `Cargo.lock` changed
    only by cargo (one `windows-sys` line under impro-core).
- `impro-ui`: nothing Unix-specific was found (the `/usr/share/icons/...` strings in `icon_sources.rs` are provenance labels of the generated icons); compiles for Windows unchanged.
- `impro-gitviews`: every `Command::new` in `src/` now uses `impro_core::process::command`. The git watchdog
  kill uses `/bin/kill -KILL` on Unix (unchanged) and `taskkill /F /T /PID` on Windows. `webkit2gtk` and
  `javascriptcore-rs` moved to `[target.'cfg(unix)'.dependencies]`. `GraphSettings` and `PaintTimings` moved
  (verbatim) from `gitviews.rs` into a shared `types.rs` and are re-exported, so the WebKit host and the Windows host share them.
  Test support/targets that use Unix-only APIs are `cfg(unix)`-gated so the pure-logic test targets compile for Windows.

### (C) Seams
- `impro-vte`: `src/lib.rs` re-exports; the existing binding moved to `src/unix.rs` (`cfg(unix)`); shared enums/bitflags
  (`Format`, `CursorShape`, `CursorBlinkMode`, `PtyFlags`) moved to `src/types.rs`; stub in `src/windows/mod.rs`
  (a `gtk::Box` GObject subclass with the notice label, the five signals, `rewrap-on-resize` and
  `current-directory-uri` properties, round-tripping settings, `spawn_async*` -> `Err(NotSupported)` from an idle handler).
  `impro-vte-sys` is a `cfg(unix)` dependency; its `build.rs` returns before probing when `CARGO_CFG_UNIX` is unset and its `lib.rs` is `#![cfg(unix)]`.
  Feature `stub-backend` compiles the stub on any platform as `impro_vte::stub` (never re-exported on Unix) so it is testable on Linux.
- `impro-gitviews`: `src/gitviews_windows.rs` has the same public items as `gitviews.rs` (`GitGraphView`, `CommitView`,
  `GraphSettings`, `PaintTimings`, `SyntaxAdapter`) and shows "Git views are not available in this Windows build yet." It is `impro_gitviews::gitviews` on Windows and `impro_gitviews::stub` under `stub-backend`. Validation, clamping, settings load and `root_changed` behave like the WebKit host; page/commit/file/error callbacks never fire.
- `impro-terminal` (`panel.rs`, `startup.rs`, manifest): `libc::kill(SIGHUP)` -> `hangup_pid` (Windows: documented no-op, nothing is ever spawned); `libc::access(X_OK)` -> `is_searchable` (Windows: `read_dir`);
  `PermissionsExt` shell check -> per-OS `pick_shell` (Windows: `SHELL`, then `ComSpec`, else `cmd.exe`); `glib::Pid` handle -> `pid_number`;
  HOME -> `impro_core::platform::home_dir()`; `/` -> `fs_root()` (`%SystemDrive%\`). `webkit2gtk` dev-dependency is `cfg(unix)`.
- `crates/impro/Cargo.toml`: `webkit2gtk` (dependency) and `javascriptcore-rs` (dev-dependency) under `cfg(unix)`. `impro-vte` stays unconditional (it now exists on both).

## BLOCKED item: `crates/impro-terminal/src/resizegate.rs` (not owned)
`glib::source::unix_fd_add_local` does not exist on Windows (line ~298). The call sits inside `if let Some(pty) = terminal.pty()`, which is always `None` on the stub, so a gate changes nothing on Linux. Needed patch (apply with `git apply`):

```diff
--- a/crates/impro-terminal/src/resizegate.rs
+++ b/crates/impro-terminal/src/resizegate.rs
@@ -293,6 +293,7 @@ impl ResizeGate {
                     g.answered(t);
                 }
             }));
+            #[cfg(unix)]
             if let Some(pty) = terminal.pty() {
                 let weak = Rc::downgrade(&answer);
                 let source = glib::source::unix_fd_add_local(
```

## Signature differences: vte stub vs Unix wrapper
| Item | Unix | Windows stub |
|---|---|---|
| `set_size`, `row_count`, `column_count`, `char_width`, `char_height`, `scrollback_lines`, `set_scrollback_lines`, `cursor_position`, `text_range_format` | `libc::c_long` | `i64` (`stub::CLong`); equals `c_long` on 64-bit Linux, so callers are source-compatible |
| `Pty::fd()` | `std::os::fd::RawFd` | `i32`, always `-1` (`Terminal::pty()` is always `None`) |
| `Terminal::vadjustment()` / `hadjustment()` | `gtk::Scrollable` trait methods | inherent methods returning `Option<gtk::Adjustment>` (a `Box` subclass cannot implement `Scrollable` in gtk-rs 0.18); `Terminal` does not `impl IsA<gtk::Scrollable>` |
| `Terminal` type | `Widget` subclass | `gtk::Box` subclass (`Terminal: IsA<gtk::Container>`) |
| `impro_vte::ffi` | re-export of `impro-vte-sys` | absent |
| `spawn_async*` | real spawn | callback gets `Err(IOErrorEnum::NotSupported)` from an idle handler |
| `Pty` | GObject wrapper | plain struct, never instantiated; `size()` returns `Err`, `unread_bytes()` 0, `canonical_or_echo()` false |
Not identical but source-compatible: gitviews `webview()` (returns `webkit2gtk::WebView` on Unix) is **omitted** on Windows; only examples/tests use it.

## How verified (exact commands and results)
Machine: Linux host, `x86_64-pc-windows-gnu` rust-std, no Windows/wine/mingw.
1. Fresh cache: `rm -rf ~/.cache/impro-win-sysroot; scripts/windows/check.sh -p impro-core -p impro-ui -p impro-vte -p impro-gitviews -p impro-terminal` (with the resizegate patch applied temporarily) -> exit 0, wall 94 s (80 s of it fetching 61 packages, ~108 MB download, 25 MB on disk). Warm and offline: exit 0, 0.17 s. Without the patch: 1 error (resizegate.rs, see above).
2. Windows-target lints: `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh -p impro-core -p impro-ui -p impro-vte -p impro-gitviews [-p impro-terminal with patch] -- -D warnings` -> clean.
3. Windows test targets: `scripts/windows/check.sh --tests -p impro-core`, `-p impro-ui`, `-p impro-vte`, `-p impro-gitviews` all compile (Unix-only tests are `cfg(unix)`); `impro-terminal` tests were not tried.
4. Linux: `cargo fmt --all --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `cargo build --workspace` ok; `cargo test -p impro --no-run` ok;
   `clippy -p impro-vte -p impro-gitviews --features ...stub-backend --all-targets -- -D warnings` ok.
5. Linux tests, `scripts/ci/run-tests.sh all` (xvfb + dbus, once on the final code before the last small commit): exit 0; 1302 Rust tests passed, 0 failed, 16 ignored; Node git-page tests 16/16 pass.
   Afterwards `cargo test -p impro-core -p impro-gitviews -p impro-vte -p impro-ui` again: 152 passed, 0 failed.
   Before/after (`#[test]` counts at merge-base vs HEAD): impro-core 47 -> 59 (+12 new unit tests: platform, process, fsops formatter);
   impro-ui 50 -> 50; impro-terminal 149 -> 149; impro-vte 2 -> 3 and impro-gitviews 42 -> 43 (the new `tests/stub.rs`, run only with `--features stub-backend`; both pass: `cargo test -p impro-vte -p impro-gitviews --features impro-vte/stub-backend,impro-gitviews/stub-backend --test stub`).
   I did not run a separate baseline on `main`; the unchanged-count claim comes from the test-attribute counts, and the suite is green.
6. Gates: forbidden-word grep and log grep print nothing; no local paths/e-mails in tracked files; `git diff --check` clean; shellcheck clean.

## Worklist for the next task (Windows `check.sh --keep-going`)
Windows type-check results of the remaining crates (compiler-verified):
| Crate | Result | Errors by file |
|---|---|---|
| impro-markdown, impro-chrome, impro-graph | compile | 0 |
| impro-explorer | 13 errors | `src/gitstatus.rs` 9 (`os::unix::{OsStrExt, AsRawFd}`, `OsStr::from_bytes` x3, `ChildStdout::as_raw_fd`, `libc::poll`/`pollfd`/`POLLIN`: the 100 ms-poll read loop of `git status`), `src/util.rs` 4 (`OsStrExt::as_bytes`, `OsString::from_vec`: byte<->path conversions) |
| impro-editor | 6 errors | `src/file.rs` 6 (`OpenOptionsExt::custom_flags(libc::O_NONBLOCK)`, `PermissionsExt::mode/from_mode`, `OsStrExt::as_bytes`) |
| impro-settings-ui | 3 errors | `src/pages.rs` 3 (`libc::access(X_OK)` + `OsStrExt`; same executable-shell check as `pick_shell`) |
| impro (host) | not checkable yet: its dependencies above fail first | static scan (not compiler-verified): `src/preferences.rs:235` `PermissionsExt` (shell executable check); `src/cli.rs:47,51` reads only `HOME` (use `impro_core::platform::expand_tilde`); its `examples/` (`window_qa`, `toolchain_spike`, `parity/scenes`) and `impro-gitviews` examples/`tests/common`, `impro-terminal/examples/webkit_host_qa.rs` use `webkit2gtk`/`webview()` and cannot build on Windows (use `--lib --bins`, not `--all-targets`) |
Also plan for: `impro-terminal/src/resizegate.rs` (patch above), tests of other crates (untried), and `libc` unused on Windows in impro-core/impro-vte.

## Unverified at runtime (Windows)
Nothing here ran on Windows. Specifically unverified: `MoveFileExW` no-replace semantics for directories and across volumes; `SHFileOperationW` (path form, thread/COM requirements when called from the worker, the nuke-warning dialog); `GetLocalTime` formatting; `CREATE_NO_WINDOW` behaviour; `taskkill` as the git watchdog; `%APPDATA%` fallbacks; the stub widgets' layout (notice centring) and the idle-callback timing; `Pid` handle truncation in `pid_number` (unused until a real spawn exists). The sysroot supplies headers and `.pc` files only: no linking, so link-time issues (missing import libs, DLL names) are not covered.

## Sysroot used
MSYS2 UCRT64 repo database `ucrt64.db`, SHA-256 `5f513c54a8e1b86c8014ef941fabf596664f848d624c2e2fbd3b86a7a61505c9` (repo last-modified 2026-10-24), 61 packages, all prefixed `mingw-w64-ucrt-x86_64-`, version-release as downloaded:
adwaita-icon-theme-51.0-1 adwaita-icon-theme-legacy-46.2-1 atk-2.62.0.1-1 brotli-1.2.1-1 bzip2-1.0.8-4 cairo-1.18.6-2 cc-libs-16.2.0-6 egl-headers-1.5.r284.3ae2b7c-1 expat-2.9.0-1 fontconfig-2.18.3-1 freetype-2.15.0-1 fribidi-1.0.17-1 gdk-pixbuf2-2.44.8-1 gettext-runtime-1.0-1 giflib-6.1.3-1 gles-headers-3.2.r1065.7fc154c-1 glib2-2.90.1-1 graphite2-1.3.15-1 gtk-update-icon-cache-3.24.52-1 gtk3-3.24.52-1 harfbuzz-14.6.0-1 hicolor-icon-theme-0.18-1 jbigkit-2.1-6 json-glib-1.10.8-2 lerc-4.1.1-1 libb2-0.98.1-3 libdatrie-0.2.14-1 libdeflate-1.26-1 libepoxy-1.5.10-7 libffi-3.8.0-1 libgcc-16.2.0-6 libiconv-1.19-1 libjpeg-turbo-3.2.0-1 libpng-1.6.59-2 librsvg-2.63.3-1 libstdc++-16.2.0-6 libsystre-1.0.2-3 libthai-0.1.30-1 libtiff-4.7.2-2 libtre-0.9.0-2 libwebp-1.6.0-1 libwinpthread-14.0.0.r532.gdbbc8e99d-1 libxml2-2.15.4-3 lzo2-2.10-3 mpdecimal-4.0.1-3 ncurses-6.6-4 openssl-3.6.5-1 pango-1.58.2-1 pcre2-10.49-1 pixman-0.46.4-3 python-3.14.9-1 python-packaging-26.3-1 shared-mime-info-2.5.1-1 sqlite3-3.54.0-1 tcl-8.6.18-1 tk-8.6.18-1 tzdata-2026e-1 wineditline-2.208-1 xz-5.8.4-1 zlib-1.3.2-3 zstd-1.5.7-2

## Open issues / assumptions
- BLOCKED item above (needs the `resizegate.rs` patch, i.e. a file outside this task's ownership).
- The brief's `../impro-wt/.heavy.lock` path does not exist from this worktree; I used `../.heavy.lock` (same directory as the worktree's parent).
- The Windows `trash()` uses the Recycle Bin (a decision; the brief left it open). If a later task prefers a private trash directory, `fsops::trash` is the only place to change.
- `stub-backend` features are not exercised by `scripts/ci/run-tests.sh`; a CI task should add `cargo test -p impro-vte -p impro-gitviews --features impro-vte/stub-backend,impro-gitviews/stub-backend --test stub`.
- Windows `pick_shell` prefers `SHELL`, then `ComSpec`; the real terminal task should decide (PowerShell, user choice).
