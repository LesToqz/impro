# win-web-fix

STATUS: DONE

All Linux and Windows-target gates pass. The WebView2 behaviour itself can only be
proven on the Windows runner: this machine has no Windows runtime, so the cause below is
established by reading our own code against the WebView2 contract (elimination), not by
observing the runtime. The new diagnostics exist so the next runner run confirms or
refutes it in one read.

## Analysis: what `NavigateToString` does and why the page was refused

The `webview2-com-sys` registry source is bindings only (no documentation, no SDK
headers on this machine), so the contract below is Microsoft's reference for
`ICoreWebView2::NavigateToString` and "Navigation events for WebView2 apps":

- `NavigateToString(html)` takes at most 2 MiB (ours is ~160 KB) and loads the string as
  a document whose origin is `about:blank`. It goes through the normal navigation
  pipeline, so the same events fire as for any navigation, in this order:
  `NavigationStarting` -> `SourceChanged` -> `ContentLoading` -> `HistoryChanged` ->
  `DOMContentLoaded` -> `NavigationCompleted`. Everything is raised on the UI thread
  asynchronously after the call returns `S_OK`.
- The navigation target the runtime reports in `NavigationStarting.Uri` is a generated
  `data:text/html;charset=utf-8;base64,...` URI, not `about:blank` ("about:blank" is the
  *origin* of the resulting page). This is the runtime's observable behaviour, not
  something the bindings pin down, so the fix does not depend on the exact spelling (see
  policy below).
- A navigation cancelled in `NavigationStarting` still ends with a
  `NavigationCompleted` event: `IsSuccess=false`, `WebErrorStatus=14`
  (`OPERATION_CANCELED`), with the same `NavigationId`.

### Which hypothesis is the cause

The notice seen on run #4, "WebView2 could not load the embedded Git page.", has exactly
one producer in the old host: the `NavigationCompleted` handler with `IsSuccess=false`
while the page was not ready. Each other hypothesis maps to a different, distinguishable
outcome, which is how they are ranked:

1. **Our own `NavigationStarting` policy (cause, by elimination).** The old handler
   allowed a navigation only if a one-shot flag was set *and* `Uri == "about:blank"`
   exactly. The URI of the `NavigateToString` navigation is a `data:` URI, so the
   document itself was cancelled, which completes as `IsSuccess=false` / `OPERATION_CANCELED`
   and lands in precisely the observed notice. Nothing else in the code reaches that
   message. The flag was also consumed by *any* first navigation event, so a stray
   earlier event would have cancelled the real document as well.
2. **`WebResourceRequested` filter `*` answering 403 to everything (latent bug, fixed
   too).** The old handler set a 403 response for every request that reached it, with no
   URI check. If the runtime raises that event for the document request, the document
   would get a 403 body. An HTTP error status is still `IsSuccess=true`, so this could not
   produce the observed notice; it would have surfaced next as a 60 s "did not finish"
   timeout. It is fixed regardless (below).
3. Shim/script failure: cannot change `IsSuccess` of the navigation; would show as a
   `page error:` line and a missing `ready`.
4. A timer firing: the only timer is the 60 s creation/ready timer, whose notice text is
   different ("did not finish creating the page within 60 seconds"); the run ended at
   24.7 s.
5. `NavigateToString` called too early: it is called from the
   `AddScriptToExecuteOnDocumentCreated` completion on the GTK/UI thread after the
   controller exists, and a failure would be an HRESULT notice ("document load"), not this one.
6. Slow first launch: same as 4, a slow start shows up as a late timeline, not as a failed
   navigation.

## Changes

- `src/windows_policy.rs` (portable, unit tested): `Navigations` bookkeeping with a
  pure verdict function.
  - One navigation is armed right before `NavigateToString`. While armed, a
    `data:text/html...` navigation (any spelling after the prefix, ASCII case-insensitive)
    is the document: it is allowed, its `NavigationId` is recorded, and the arming is
    consumed, so a second `data:` navigation (for example from page script) is cancelled.
    A plain `about:blank` while armed is allowed *without* consuming the arming (older
    runtime spelling, or a harmless initial blank page) and is never taken for the document.
    Redirected navigations, every other URI and everything when not armed are cancelled.
    Arming ends on `ready`, reload, or detach.
  - `NavigationCompleted` is classified by id: document success/failure, "cancelled by
    policy" (ignored, no notice), unrelated, or an unknown navigation failing with a
    non-cancel status. A cancelled script navigation can therefore no longer trigger the
    notice, and a real failure of the document always does.
  - `blocks_resource(uri)`: only `data:`, `about:` and `blob:` pass; the 403 answer
    applies to everything else. The `*` filter is kept, so nothing that could leave the
    process is let through, and the document request is never answered by the filter.
- `src/windows_diag.rs` (new, portable, unit tested): HRESULT formatting, URI
  formatting (120 chars; `data:` URIs print only their media type and length, never the
  html), `WebErrorStatus` and `ProcessFailedKind` names (0..=18 and 0..=9 as in the
  bindings), the `Diagnostics` timeline with phase marks, bounded to 300 lines.
- `src/gitviews_windows.rs`:
  - Every COM call that can fail is logged with its HRESULT (`step`, `failed`), including
    each settings call, every event registration, and failures inside event handlers
    (previously returned to the runtime silently; now logged, handler still returns S_OK).
  - `NavigationStarting` fails closed: if any argument cannot be read the navigation is
    cancelled. It logs id, truncated URI, user-initiated, redirected and the decision.
    Frame navigations are always cancelled and logged.
  - `NavigationCompleted` logs id, `IsSuccess`, `WebErrorStatus` number and name and the
    classification; `ProcessFailed` logs kind number and name; blocked/passed resources,
    refused new windows, denied permissions, cancelled downloads, page `error` messages
    (200 chars) and the 60 s timer (with the last phase reached) are logged.
  - Phases with elapsed times: `environment`, `controller`, `shim_registered`,
    `navigation_started`, `navigation_completed`, `ready`.
  - The notice stays short and gains one extra line `[code]` with the concrete cause.
  - `WebView::diagnostics()` (`#[doc(hidden)]`) returns the whole timeline.
  - Stderr echo, line by line as it happens, when `IMPRO_GIT_DEBUG=1` or `IMPRO_GIT_DIAG=1`.
    Silent otherwise.
- `tests/windows_webview.rs`: failure messages carry the notice text and the timelines
  of every tracked host; phase budgets (30 s, 45 s for the ready handshake) are shorter
  than the host's 60 s timer and the 240 s overall budget, and the failure text says
  which one tripped. It still fails (does not skip) when a notice appears. The
  `location.href` assertion was written blind (`about:blank`); it now compares to the
  value before the navigation attempts, and additionally requires the timeline to contain
  the host's `CANCELLED` decisions for the script navigations.
- `src/log.rs`: `repo.root` natively separated on Windows (see below).
- `tests/test_gitlog.rs`, `tests/test_gitdiff.rs`, `tests/test_gitviews.rs`: see below.

## The other Windows failures

- `test_default_precedence_and_subdirectory_root`: **fixed in the product**, Unix
  byte-identical. Git prints `D:/a/.../repo`. `GitGraphView::root()` and the `open-file`
  containment check use the root the caller passed (`canonicalize()`d, native), not the
  JSON's `repo.root`, so no comparison breaks today; but the field is a filesystem path
  consumers may compare with the explorer's and editor's native paths, and the "git
  failed" fallback of the same field is already native, so it flipped between two
  spellings depending on whether git succeeded. `log::native_root` replaces `/` with `\`
  on Windows only (unit tested for both modes; a Unix `\` is a legal file-name
  character and is never touched).
- `test_showcase_reference_json_parity`: the showcase makes `app.py` executable with
  `chmod` under `cfg(unix)`; NTFS has no mode bits, git records 100644, so the tree and
  every later commit id differ from the Linux-generated `reference.json`. The hash-exact
  test is now `cfg(unix)` with a comment. A new `test_showcase_reference_structure_parity`
  runs on every platform and compares ref names, parent topology (as positions), subjects,
  authors, times, refs, merged names, head, default branch, dirty flag against the same
  reference.
- `test_twenty_file_and_three_thousand_line_performance`: 300 ms on Unix, 1200 ms (4x)
  on Windows because process creation is slower there; the failure message prints the
  measured time.
- `test_gitgraph_core` / `test_pure_helpers`: the failure message now says `node` was
  not found on PATH, how to fix it, and that CI needs a Node setup step first.

## Verification (Linux host)

All heavy commands under `flock ../.heavy.lock systemd-run --user --scope -p MemoryMax=...`
with the shared target directory.

- `cargo fmt --all --check`: clean.
- `scripts/windows/check.sh -p impro-gitviews --all-targets`: clean.
- `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh -p impro-gitviews --all-targets -- -D warnings`: clean
  (the Windows host, the Windows test and the new modules compile for
  `x86_64-pc-windows-gnu`; nothing is linked or run).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `env -u WAYLAND_DISPLAY xvfb-run -a dbus-run-session -- cargo test -p impro-gitviews`
  (`GDK_BACKEND=x11`, `G_DEBUG=fatal-criticals`), debug and `--release`: all pass (lib
  20, gitdiff 19, gitlog 19 + 1 pre-existing ignored, gitviews 5, i.e. the real WebKit
  pages). The new policy/diagnostics/root tests are part of the 20 lib tests.
- `scripts/ci/run-tests.sh js`: 19 pass.
- Forbidden-word, local-path and `git diff --check` gates: see the end of this file's
  commit; all empty.

## Diagnostics format and runbook

Run on the Windows runner (or any machine with the Evergreen runtime):

```
cargo test -p impro-gitviews --test windows_webview -- --nocapture
```

`IMPRO_GIT_DIAG=1` is optional there: the host timeline is always printed on failure, and
one `graph host phases: ...` / `commit host phases: ...` line on success. Set
`IMPRO_GIT_DIAG=1` (or `IMPRO_GIT_DEBUG=1` for the application) to stream the full
timeline to stderr live, which also survives a hang or a process crash.

Shape of the output (`[impro-gitviews webview2#N]` prefixes the live lines; N numbers
the hosts, 1 = graph):

```
phases: environment=412ms controller=980ms shim_registered=1002ms navigation_started=1013ms navigation_completed=1090ms ready=1180ms
+0ms begin: creating the WebView2 environment
+12ms Evergreen runtime version 1xx.0.xxxx.xx
+412ms [environment] environment created
+980ms [controller] controller created
+1002ms [shim_registered] document-created shim registered
+1002ms NavigateToString: document is 159000 bytes (runtime limit 2 MiB); navigation armed
+1003ms NavigateToString returned S_OK
+1013ms [navigation_started] NavigationStarting id=2 uri=data:text/html;charset=utf-8;base64,<212000 chars total, content omitted> user_initiated=false redirected=false -> ALLOWED (the armed document)
+1090ms [navigation_completed] NavigationCompleted id=2 IsSuccess=true WebErrorStatus=0 (UNKNOWN) -> DocumentLoaded
+1180ms [ready] page sent its ready message
```

How to read a failure (the panic text starts with the headline, then elapsed time, then
the `phases:` line and the timeline per host):

| What you see | Meaning |
|---|---|
| `PHASE TIMEOUT` and `phases:` ends at `environment=-` or `controller=-` (or those values are tens of seconds) | slow runner / browser launch; rerun, consider a warm-up step |
| `notice: ... [WebView2 ... failed: ..., HRESULT 0x........]` | that exact call failed; the `FAILED` line in the timeline names it |
| `NavigationStarting ... -> CANCELLED: unexpected navigation target` for a `data:` or other URI before `navigation_started` is set | the document navigation was refused: the policy and the runtime disagree about the URI; the line shows the URI the runtime used (report it) |
| `NavigationCompleted ... IsSuccess=false ... (OPERATION_CANCELED) -> PolicyCancelled` followed by the same page loading | harmless, a script navigation we refused |
| `NavigationCompleted ... IsSuccess=false` with another status (for example 13/16) and `-> DocumentFailed` | the runtime itself failed to load the document; the code line in the notice carries the status name |
| `navigation_completed` set, `ready=-`, `PHASE TIMEOUT` | page loaded but its script did not start: look for `page error:` lines and `resource blocked (403)` lines (a blocked inline resource) |
| `TIMER: the 60 s creation/ready timer fired; last phase reached: X` | stall after phase X |
| `ProcessFailed kind N (...)` | a browser/render/GPU process died |

Expected per hypothesis on the runner if the analysis above were wrong:

- H2 (document answered by the filter): `resource blocked (403): data:text/html...` before
  `NavigationCompleted IsSuccess=true`, then `PHASE TIMEOUT` with `ready=-`. The new
  policy lets `data:` through, so this can no longer happen.
- H3: `navigation_completed` set, `page error: ...`, `ready=-`.
- H4/H6: the failing phase has a large elapsed time or the `TIMER` line; there is no
  `CANCELLED` / `IsSuccess=false`.
- H5: a `FAILED NavigateToString failed: ..., HRESULT 0x...` line.

## Open issues / assumptions

- Not run on Windows (no runtime here). The cause is established by elimination from our
  code against Microsoft's documented event order and the runtime's `data:` URI for
  `NavigateToString`; the runner confirms it (the `navigation_started` line shows the
  real URI).
- The policy accepts `data:text/html` by prefix and `about:blank`; it does not pin the
  base64 spelling or the payload, because the bindings do not define them. Safety comes
  from the single armed, id-tracked navigation, not from the URI text.
- `IsUserInitiated` is logged but not used in the decision (the navigation is
  app-initiated, but the flag's value for `NavigateToString` is not guaranteed, and the
  armed one-shot already makes it irrelevant).
- If the resource handler runs without an environment (only after teardown started), the
  request is logged as `NOT blocked`; it cannot occur while the controller is live.
- The test now additionally asserts that the host logged `CANCELLED` decisions for the
  page-script navigations. This assumes the runtime raises `NavigationStarting` for
  script-initiated navigations, as documented.
- `windows.yml` was not touched; the CI step must put `node` on PATH for
  `test_gitgraph_core` / `test_pure_helpers`.
