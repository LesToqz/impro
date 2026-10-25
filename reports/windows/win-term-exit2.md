# Windows terminal: interactive shells exiting at start, and the grid check

STATUS: PARTIAL

Both runner symptoms are handled in the tests and the backend is hardened and instrumented, but the root cause of B (an interactive `cmd.exe` exiting with code 0 about 13 ms after its banner) is NOT established. Nothing in the code can be shown to produce that exit, and the run-#6 timeline has no event that could say what did. This report separates what the evidence rules out from what stays open, and the new events are chosen so that run #7 decides it.

## B: what the run-#6 timeline and the code prove

Timeline of both panes (`argv=["cmd.exe"]`): `process created` +0.009, `first output byte (16 bytes)` +0.025, `process exit observed (exit code 0)` +0.038, then the normal shutdown chain, `reader EOF (299 bytes)`, `child-exited`.

| Hypothesis from the brief | Verdict | Evidence |
|---|---|---|
| (4) false exit: the wait fires while `cmd.exe` runs, the later close kills it | Refuted as the cause, hardened anyway | The line prints `GetExitCodeProcess`; a live process reports 259 (STILL_ACTIVE), the log says 0. The wait is on a duplicate of the process handle (`duplicate(duplicate(process))`, `DUPLICATE_SAME_ACCESS`), never a thread or pipe handle. The old waiter ignored the result of `WaitForSingleObject`, so a failed wait would have been reported as an exit; that gap is closed (below) |
| (2) stop / hang-up right after spawn | Refuted by the timeline | Every `Session::stop` ends in a `stop requested` line in the controller; none appears. All callers are `startup.rs` (closed or cancelled pane), the widget's `dispose`, and the spawn-result handler |
| (3) the reader writes `ESC[1;1R` to the wrong place | Refuted for start-up | The reader answers only while `closing` is set, and `closing` is set only by `StartClose` (after the exit, or by a stop) |
| (1) the input pipe's write end is closed early | Possible but unproven | The write end is owned by the writer thread for the whole session and the queue's senders live in the controller thread until `Done`. The one early path in the old code: a failing `write_all` ended the thread and closed the write end, which the console host reads as end-of-input. Whether any write failed in run #6 is not visible: nothing logged writes |

What is certain from the timeline is therefore only: the OS process really ended by itself with status 0 while no stop was requested, and the panel reacted correctly (child-exited -> `close_pane`), which is why `panes()` was empty at `windows_panel.rs:148`. The earlier steps passed vacuously for the reason the brief states.

Open candidates that the new log separates: (a) a write error closing the input (now impossible, and logged); (b) bytes that reach the child at start: the widget sends `ESC[I` / `ESC[O` on focus changes once the console host asked for focus reporting (`ESC[?1004h` is in the 16 first bytes), the emulator answers queries through the same path, and the test types `echo impro-dock-ok` as soon as the child id exists, before the shell is up; (c) the console host or the runner image itself (the build was not the same as in run #5 as far as the brief says). Every byte written to the input pipe is now in the timeline.

## Changes

### Backend (`crates/impro-vte`, Windows path; Unix untouched)

- **Exit events must be real.** `lifecycle::ExitProbe { signalled, code }` and `Lifecycle::process_probed`. The waiter thread loops: `WaitForSingleObject` then a zero-timeout probe plus `GetExitCodeProcess`. It reports an exit only when the handle is signalled or the code is not 259; otherwise it logs `exit event ignored`, sleeps 50 ms and waits again (so a failing wait degrades to polling instead of faking an exit). The controller applies the same rule (`process_probed` ignores and counts a non-exit). `Control::ChildExit` now carries the probe.
- **The input pipe belongs to the session.** New portable `input::pump` (generic over `Write`, unit-tested): the loop ends only on the session's closing marker or when every sender is gone. A failed write no longer ends it (the write end stays open, further input is discarded and counted). The writer thread logs `writer thread exit (reason)` and, after dropping the pipe, `input pipe write end closed (reason)`. The dropping of a spawn request's sender cannot end it (tested).
- **Diagnostics** (`IMPRO_TERM_DIAG=1`), new events, in the order a session produces them:

| Event | Meaning |
|---|---|
| `pseudo console created (CxR)` | `CreatePseudoConsole` succeeded |
| `output chunk #n: N bytes [escaped]` | the first 24 reads of every session, bytes escaped as `\e`, `\r`, `\n`, `\xNN` (160 bytes shown). Shows what the console host sends at start and at teardown, e.g. a clear-screen |
| `session attached to the widget` | the spawn result reached the widget (main context) |
| `input write #n: N bytes [escaped]` | the first 40 writes to the child's input: keystrokes, focus reports (`\e[I`, `\e[O`), the emulator's replies, the closing CPR answer |
| `input write failed: ...; the pipe stays open ...` / `N bytes of input were discarded` | a write error (no longer closes the pipe) |
| `ResizePseudoConsole(CxR) returned 0x........` | each resize and its HRESULT |
| `process wait signalled (WaitForSingleObject=WAIT_OBJECT_0, handle=process duplicate, GetExitCodeProcess=N, still_active=false)` | the waiter's confirmation of a real exit |
| `process wait returned <result> but the process is still active (...); exit event ignored` | a false wake-up (first three logged) |
| `stop called from <file>:<line>` | who stopped the session (first call only), before the controller's `stop requested` |
| `writer thread exit (reason)` / `input pipe write end closed (reason)` | the end of the input side and why |
| `session handle released (...)` | the last `Arc<Session>` reference was dropped |

  The existing events are unchanged (`spawn requested`, `process created`, `first output byte`, `process exit observed`, `drain started`, `ClosePseudoConsole ...`, `reader EOF`, `child-exited posted/emitted`, `resources released`).
- Files: `src/lifecycle.rs` (`ExitProbe`, `STILL_ACTIVE`, `process_probed`, `spurious_exits`), `src/input.rs` (new, portable), `src/diag.rs` (`escape`), `src/lib.rs`, `src/windows/conpty.rs`, `src/windows/widget.rs` (one log line). Bounded shutdown, 2 KiB packets, one packet in flight, no `RefCell` borrow across a signal: unchanged.

### Tests

- **Linux-runnable** (`crates/impro-vte/tests/lifecycle.rs`, feature `emulator`, 7 new): an exit event for a still running process (STILL_ACTIVE, unsignalled) is ignored, counted, and neither starts a close nor consumes the later real exit; probe classification (including a real exit with 259); the input loop outlives the dropped request sender and ends only on the marker; it ends when all senders are gone; a failed write keeps the loop (and so the pipe) alive and discards the rest; only the first 40 writes are logged; `escape`.
- **`windows_panel.rs`** (fails on the old behaviour at its first step): `step()` asserts the pane count before and on every iteration of a step, and fails with the timeline, the grid dump and the pane list; conditions are latched. New `stay_alive`: two shells must still run 2 s after start (no pane gone, closed or without child id, and no `process exit observed` in the timeline). Then: input in both panes, resize reaching both consoles (`resized` is no longer vacuous), `exit\r` typed into a live pane closes exactly that pane and leaves the other (ids compared), the survivor still resizes and takes input, `close_current` releases its session.
- **`windows_conpty.rs`**: `wait_all` latches each condition (it reports "ever true / now"). New stage `interactive_shell` for `cmd.exe /d /q` and, if available, `powershell.exe -NoExit`: alive 2 s, takes input, `exit 5` / `exit 3` closes it with exactly that status, once.

## A: why `impro-ok` was not on the grid at the end

Also undetermined. The old wait required the text and the exit at the same instant; a text that was on the grid and later erased passes through the old check unnoticed and then fails it. The command output and a screen erase are both possible sources (a teardown frame from the console host, or a reset). The change makes the evidence decisive without hiding later stages:

- The condition "reached the grid" is latched (also from a `contents-changed` watcher, so a short-lived grid is caught).
- If the text is on the grid at some point and gone after the exit, the stage records a **soft failure** carrying the grid dump and the timeline (with every `output chunk` the console host sent), skips the clipboard step for that round, and the test fails once at the very end listing all soft failures, so the other stages still run and report.

Product decision (pending evidence): the emulator renders exactly what the console host sends, and a finished `cmd /c` command leaves its output visible. If run #7 shows that the text is erased by a sequence the console host emits during teardown (an `output chunk` with `\e[2J`/`\e[H`/`\e[?1049l` after the text), the backend should ignore erasing output that arrives after `process exit observed`; that change needs the escape sequence from the log, so it was not guessed here. If the chunks show no erase, the cause is on the widget side (feed ordering or the text extraction) and the soft failure's grid dump says which grid line holds the text.

## Verification (Linux host, shared target cache, serialised and capped)

| Command | Result |
|---|---|
| `scripts/windows/check.sh -p impro-vte -p impro-terminal --all-targets` | exit 0 |
| `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh -p impro-vte -p impro-terminal --all-targets -- -D warnings` | exit 0 |
| `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo clippy -p impro-vte --features emulator --all-targets -- -D warnings` | clean |
| `cargo test -p impro-vte --features emulator` (xvfb, dbus-run-session, GDK_BACKEND=x11) | green: lifecycle 23 (7 new), emulator 13, contract 1, spawn_path 1, terminal 1 |
| `cargo test -p impro-vte -p impro-terminal` (same) | green, counts unchanged (panesplit 14, resizegate 20, terminal_bar 55, dup 4, panel 38, resize 5, startup 12, unit 1) |
| `cargo deny` | not run (tool not installed); no manifest or lockfile change |

## UNVERIFIED until the runner (CEO reads run #7)

- That B is gone. The guards (a write error cannot close the input, a non-exit is never reported as an exit) are certain by construction and test; whether they were the cause is not known.
- The behaviour of the new Windows stages (alive after 2 s, `exit` closes exactly one pane, PowerShell `-NoExit` status).
- The cause of A.

## Runbook

```
set IMPRO_TERM_DIAG=1
cargo test -p impro-terminal --test windows_panel -- --nocapture
cargo test -p impro-vte --test windows_conpty -- --nocapture
```

(bash: `IMPRO_TERM_DIAG=1 cargo test ...`; the tests set the variable themselves.) Keep the full output as an artifact, pass or fail. Read the panel run for one pane (`#n`):

| What the timeline shows | Conclusion |
|---|---|
| `stay_alive` passes, `exit` closes one pane, rest green | B is gone (the guards fixed it) |
| `input write failed: ...` before `process exit observed` | candidate (1) confirmed in the old form; the pipe now stays open, so the shell should not have left: look at the error text |
| `input write #n: .. [\e[I]` or `[\e[O]` shortly before `process exit observed` | the focus report reaches the console host as the shell starts; test with focus reporting withheld until the child has printed its prompt |
| `input write #n: .. [echo impro-dock-ok\r]` right before the exit, no other input | typed-ahead input at start is what ends the shell; delay the test's first input until the prompt is on the grid and make the panel buffer early input like `startup.rs` does for its own replay |
| `input write` lines only the emulator's replies (`\e[?..c`, `\e[..R`, ...) | a query reply confuses the console host; the replied bytes name which query |
| `ResizePseudoConsole(..) returned 0x8....` followed by the exit | a resize during start-up ends the shell |
| none of the above: `process wait signalled (... GetExitCodeProcess=0 ...)` with no input write at all | the console host or the OS ended the shell on its own; compare runner image builds with run #5 and try `cmd.exe /k` outside the application with the same CreatePseudoConsole parameters |
| `process wait returned ... still active; exit event ignored` | a false wake-up: the old code would have reported it as the exit |
| `stop called from <file>:<line>` | names the caller that stopped the session |

For A, in the `windows_conpty` run: find the `output chunk #n` lines of the `echo impro-ok` session. `impro-ok` in a chunk followed by a chunk with `\e[2J`, `\e[H` or `\e[?1049l` means the console host erased the screen during teardown (product decision above); no erase and a soft failure means the widget lost the text (compare the grid dump); no soft failure means the old failure was only the simultaneity of the two conditions.

## Open issues / assumptions

- B's root cause is open (above); STATUS stays PARTIAL until run #7.
- The 2 s liveness window and the first 24 output chunks / 40 input writes logged per session are constants in the tests and `conpty.rs`/`input.rs`.
- A waiter whose wait keeps failing polls the process every 50 ms until the process ends or the controller is gone; the first three occurrences are logged.
- Workflows are not owned here: the Windows steps should run the two commands above with `--nocapture` and upload the output.
