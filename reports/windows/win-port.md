# Windows workspace port

STATUS: DONE

## Changes

- `crates/impro-explorer/src/gitstatus.rs`: Unix poll/read logic remains behind `cfg(unix)`. Windows uses a reader thread, a bounded `sync_channel(1)`, 64 KiB chunks, and `recv_timeout` bounded by 100 ms and the original deadline. Output/record caps, cancellation, child kill/wait, partial-record handling and error outcomes are preserved. Git is launched through `impro_core::process::command` (CREATE_NO_WINDOW on Windows). Windows Git paths decode UTF-8 lossily, convert separators, and strip CRLF from rev-parse output. Canonical directory paths reconcile ordinary drive/UNC paths with Rust verbatim paths; snapshot keys retain the explorer root's original spelling.
- `crates/impro-explorer/src/util.rs`: Unix byte encoding is unchanged. Windows uses safe UTF-16 hex escaping (`encode_wide`/`from_wide`) for unpaired surrogates and an escaped reserved prefix. It needs no unchecked UTF-8 reconstruction; arbitrary decoder input remains memory-safe. Model encodings are local to the same process/platform. Added a shared Unicode/private-character round-trip test and a Windows surrogate test.
- `crates/impro-explorer/src/search.rs`: excluded directory names, including `.git`, compare without ASCII case on Windows. Unix matching is unchanged. Other path operations use `Path` components, not string separator splitting; directory symlinks remain leaves and search does not recurse through them.
- `crates/impro-editor/src/file.rs`: NONBLOCK and mode-bit preservation remain Unix-only. Windows saves retain std permissions, reject read-only destinations without changing them, and skip Unix directory fsync. `read_link` error 4390 (ERROR_NOT_A_REPARSE_POINT) is treated as an ordinary file. `tempfile::persist` uses MoveFileExW with REPLACE_EXISTING on Windows, not the no-replace helper; permissions are applied after persist because tempfile first clears temporary attributes. Existing CRLF policy is unchanged: normalize CRLF to LF while editing, restore CRLF when saving. Added portable CRLF and Windows read-only tests.
- `crates/impro-core/src/exec.rs` and the platform declaration in `lib.rs`: shared executable discovery. Unix uses access(X_OK). Windows requires a regular file and accepts an explicit path or a case-insensitive PATHEXT extension (default `.COM;.EXE;.BAT;.CMD`); bare command lookup appends PATHEXT entries on PATH. Tests cover injected extension lists, missing paths/directories, Unix execute permission and Windows explicit paths. To keep `platform.rs` untouched and expose the requested `platform::is_executable` API, the platform module declaration points to `exec.rs`, which re-exports the original directory module unchanged.
- `crates/impro-settings-ui/src/pages.rs` and `crates/impro/src/preferences.rs`: consume the shared executable helpers instead of Unix-only calls.
- `crates/impro/src/cli.rs`: uses `platform::expand_tilde`, including Windows backslash tilde syntax. Path expansion and resolution have an injectable seam for portable tests; Windows tests cover drive/UNC forms and real forward-slash paths. Before help/version/usage output, GetStdHandle checks stdout/stderr; AttachConsole(ATTACH_PARENT_PROCESS) runs only if either is missing or INVALID_HANDLE_VALUE. Any valid redirected stream is saved and restored with SetStdHandle, including mixed console/pipe cases. No console is allocated for an Explorer launch. Exit codes remain 0 for help/version and 2 for usage errors.
- `crates/impro/src/main.rs`: release Windows builds use the Windows GUI subsystem; debug builds retain their console.
- `crates/impro/src/app.rs`: Windows uses NON_UNIQUE with the existing application ID, so it does not depend on owning a D-Bus name or forwarding activation. GLib still probes for a session bus, then continues locally when none exists. See the [GLib registration implementation](https://github.com/GNOME/glib/blob/main/gio/gapplicationimpl-dbus.c).
- `crates/impro/src/style.rs`: the host's CSS registration hook adds Windows defaults: Segoe UI for UI, and Cascadia Mono, Consolas, Courier New, monospace for the editor. Existing user overrides are appended later and win. No impro-ui edit or font follow-up is needed.
- `crates/impro/{Cargo.toml,build.rs,windows/impro.rc,windows/impro.manifest}`: Windows console APIs and native resource embedding. `embed-resource` is pinned to 3.0.12 (declared MSRV 1.76; resolved TOML dependencies declare 1.85). Cargo generated the lockfile changes; no workspace dependency declaration changed. No other dependency was added directly.
- Portable test/example compilation: Unix guards on specific editor/explorer tests and XTest support; the graph fixture loader no longer warns about its Unix-only symlink target on Windows. The editor/Markdown performance examples and host WebKit examples have non-Unix stub mains. GTK test helpers and Markdown child tests set the X11 backend only on Unix. No Windows check-script changes were needed.
- `.gitattributes`: automatic text detection with LF on every text type, including shell/installer scripts and PowerShell/batch scripts. PNG/ICO/JPG/JPEG/GIF/ZIP/RGBA and the frozen single-line Markdown JSON fixture are binary. The latter has no line terminators and remains byte-exact.

## Verification

Every heavy command used the shared warm target directory, `CARGO_INCREMENTAL=0`, and the prefix:

```sh
flock ../.heavy.lock systemd-run --user --scope -q -p MemoryMax=3500M -p MemorySwapMax=0
```

No worktree target directory was created. GUI commands unset WAYLAND_DISPLAY and use Xvfb, a D-Bus session and GDK_BACKEND=x11.

| Command (after the heavy-command prefix above) | Result |
|---|---|
| `scripts/windows/check.sh --workspace --all-targets --keep-going` | Exit 101; errors exclusively in the unowned targets listed below. No owned-target errors. |
| `scripts/windows/check.sh --workspace --all-targets --exclude impro-terminal --exclude impro-gitviews` | Exit 0; every remaining workspace target, including all owned tests/examples and the host, compiles. The excluded crates' libraries remain checked as host dependencies. |
| `scripts/windows/check.sh --workspace --lib --bins` | Exit 0 for the whole workspace. |
| `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh --workspace --lib --bins -- -D warnings` | Exit 0, no warnings. The environment assignment precedes the heavy-command prefix. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Exit 0, Linux host, no warnings. |
| `env -u WAYLAND_DISPLAY GDK_BACKEND=x11 scripts/ci/run-tests.sh all` | Exit 0: 1309 Rust tests passed, 0 failed, 16 ignored; Node 16 passed, 0 failed. Unit: 216; strict integration: 1061; tolerated integration: 32; doctests: 0 passed, 2 ignored (included in the 16). |
| `cargo build -p impro` | Exit 0, Linux debug build. No resource compiler/linker step in the log. |
| `env -u WAYLAND_DISPLAY GDK_BACKEND=x11 bash scripts/ci/smoke.sh "$CARGO_TARGET_DIR/debug/impro"` | Exit 0: version, help, invalid-folder exit 2, launch exit 0, PNG first paint (19729 bytes), first paint 458.920 ms. The binary path uses the required shared cache rather than a worktree target directory. |

`cargo fmt --all --check` also exits 0. Windows checks use the unchanged sysroot recorded by the foundation task (61 packages, database digest starting `5f513c54a8e1`), and check.sh sets IMPRO_SKIP_WIN_RESOURCES=1.

The all-target failures are exactly:

- `impro-gitviews` examples: `git_perf` (6 errors), `webkit_host_qa` (33 errors), from Unix-only WebKitGTK/JavaScriptCore support and omitted stub webview methods.
- `impro-terminal` tests: `performance`, `test_panesplit`, `test_terminal_bar`, `test_terminal_panel`, `test_terminal_resize`, `test_terminal_startup` (3 errors each from the shared Unix process/shell helpers); `test_resizegate` and `test_terminal_dup` (7 errors each, including Unix stop/continue signals).
- `impro-terminal` example: `webkit_host_qa` (4 errors from shared Unix helpers and WebKitGTK).

The terminal library's known resizegate line is already gated on this base; the remaining resizegate errors are in its test target, not its library. None of these unowned targets was edited.

Additional resource API validation: temporarily made the allowed embed-resource build dependency unconditional and compiled the exact resource helper on the Linux host, then ran `scripts/windows/check.sh -p impro --lib --bins` with the same heavy prefix. Exit 0. Both files were restored exactly afterwards. This type-checks the resource helper's real embed-resource API and version definitions without claiming that windres or the native linker ran.

Before/after: the source diff against main adds six Linux unit tests and retains every existing Linux test; the final run has 1309 passed, 0 failed, 16 ignored, plus Node 16/16. That implies a 1303-test baseline for this checkout. The older foundation report records 1302 passed; a separate current-main baseline suite was not run, so that one-test difference from the historical report is unverified. All six new Linux tests appear as passed in the full-suite log. New Windows-only tests are compile-checked and remain unexecuted.

Line-ending checks: `git add --renormalize --dry-run .` exits 0. An isolated copy of the Git index was then actually renormalized; its `git write-tree` hash was identical before and after. All 91 non-LF entries from `git ls-files --eol` have binary attributes; every other tracked entry is LF. `git diff --check` is clean. The required forbidden-text scan of tracked files and commit metadata prints nothing. Broad path/contact scans find pre-existing synthetic test fixture paths/identities and decorator syntax; no machine-local paths or contact details were added.

## Resources and manifest

`build.rs` checks CARGO_CFG_TARGET_OS first, so a Linux target does no resource work. Captured Linux build-script output contains only:

```text
cargo:rerun-if-env-changed=IMPRO_SKIP_WIN_RESOURCES
```

With IMPRO_SKIP_WIN_RESOURCES=1, Windows build-script output lists the three rerun-if-changed inputs and the plain note `impro: Windows resources skipped for type checking`. It emits no cargo warning or resource linker argument. Native Windows builds call `embed_resource::compile_for` for the impro binary and require success. In MSYS2 UCRT64, installing GCC brings the binutils resource compiler; `windres` is found on the UCRT64 PATH. See [embed-resource's build API](https://docs.rs/embed-resource/3.0.12/embed_resource/).

The target-specific build dependency is selected for the build-script host. Therefore resource embedding is implemented for a native Windows host; a Linux-to-Windows check uses the explicit skip. A full cross-link with resources from a Linux host is unsupported by this configuration and fails with an explanatory message rather than silently omitting resources.

The numeric file/product version is major,minor,patch,0; the string version is CARGO_PKG_VERSION. CompanyName is omitted. Resource file:

```rc
#include <windows.h>

1 ICON "../../packaging/icons/windows/impro.ico"
1 RT_MANIFEST "windows/impro.manifest"

1 VERSIONINFO
FILEVERSION IMPRO_VERSION_NUM
PRODUCTVERSION IMPRO_VERSION_NUM
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
FILESUBTYPE 0
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "impro\0"
            VALUE "FileVersion", IMPRO_VERSION "\0"
            VALUE "InternalName", "impro\0"
            VALUE "LegalCopyright", "MIT License\0"
            VALUE "OriginalFilename", "impro.exe\0"
            VALUE "ProductName", "impro\0"
            VALUE "ProductVersion", IMPRO_VERSION "\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
```

Manifest:

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="dev.impro.impro" version="1.0.0.0" processorArchitecture="amd64"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
</assembly>
```

- `assembly` identifies the manifest schema; `assemblyIdentity` supplies the stable executable identity and an independent side-by-side assembly version.
- `asInvoker` uses the launching user's privileges; `uiAccess=false` requests no elevated input access.
- `supportedOS` uses the shared Windows 10/11 compatibility GUID.
- `activeCodePage` selects UTF-8 on supported Windows versions; `longPathAware` opts into the OS long-path policy. Filesystem/library limits still need runtime checking.
- DPI declarations are deliberately absent: [GTK 3.24's Win32 backend](https://github.com/GNOME/gtk/blob/gtk-3-24/gdk/win32/gdkdisplay-win32.c) calls SetProcessDpiAwareness(PROCESS_SYSTEM_DPI_AWARE), with an older SetProcessDPIAware fallback, and handles already-configured awareness. This keeps DPI policy with GTK rather than requesting a conflicting per-monitor-v2 policy. Multi-monitor fractional scaling is unverified.

Manifest fields were checked against [Microsoft's manifest reference](https://learn.microsoft.com/en-us/windows/win32/sbscs/application-manifests).

## Linux-only coverage

Existing Unix test bodies are retained:

- Editor: mode-preserving CRLF integration test, FIFO rejection, symlink atomic save, raw Unix path signals, and symlink Markdown project boundaries. The new portable CRLF test covers Windows too.
- Explorer: raw byte-name integration target; permission/symlink-cycle and symlink-selection tests; search's symlink fixture test; shell-script Git timeout/cancellation/environment tests; real XTest divider dragging. The XTest struct and its implementation are compiled only on Unix.
- Performance examples: editor_perf and livemd_perf require POSIX CPU timers or Linux process files; host toolchain_spike and window_qa (including parity/scenes) require WebKitGTK/JavaScriptCore. They keep their Linux bodies and expose empty non-Unix mains. startup_floor and portable demos still compile normally.
- Existing graph symlink fixtures remain Unix-only; no new Windows symlink test requires elevated privileges or Developer Mode.

## Open issues and Windows runner priorities

Windows runtime, native resource compilation/linking, DLL resolution and final PE inspection are UNVERIFIED here; the local loop supplies headers/pkg-config metadata and does no Windows linking or execution.

1. Native release build first: confirm windres is found, both version macros compile, icon and RT_MANIFEST land in the executable, Explorer shows the version/icon, and no console window appears on GUI launch. Check MSRV 1.88 on the native dependency graph.
2. Console smoke tests in cmd/PowerShell: help/version and usage exit codes; capture both streams; capture only stdout or only stderr; launch without inherited console. Check that AttachConsole preserves redirected output.
3. Launch without dbus-daemon or a session address: verify quick first paint and no registration failure. GLib's synchronous session-bus probe is still present; elapsed startup on Windows has not been measured.
4. Open/save existing and new LF/CRLF files, read-only errors, ordinary-file read_link handling, symlink/junction saves, and cleanup after replacement failure. Test non-ASCII, drive-root, UNC and long paths, including shares with different permissions.
5. Git decorations from canonical/verbatim roots, mixed drive-letter casing, Unicode names, renames/deletions and worktrees; cancellation latency, time/output caps, Git missing or failing, and child/reader cleanup. The detached reader relies on the child's pipe closing after termination; inherited pipes held open by descendants may keep that reader alive until those descendants exit.
6. Font resolution and user font overrides; mixed-DPI monitor moves; GTK clipboard, drag/drop, native dialogs and screenshots. Portable test targets compile, but their GTK/platform pixel assertions have not run on Windows.

Additional audit observations for follow-up: Windows path identity can still differ when the same file is reached with alternate casing outside the Git collector; do not globally lowercase paths because Windows directories can enable case sensitivity. The core file-operation name validator is outside this task's ownership; it should eventually validate Windows separators and reserved device names. No unowned file was edited for these observations.
