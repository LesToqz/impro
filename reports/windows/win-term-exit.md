# Windows terminal exit / close

STATUS: PARTIAL

Everything verifiable on Linux passes. The fix itself is UNVERIFIED until the Windows runner executes the two tests: the root cause below is established by reading the code against known ConPTY behaviour, not by observing it. The new timeline is meant to confirm or refute it in one run.

## Cause

Both failing tests wait for `child-exited`. In the run-#5 design the controller thread reported it only after `reader.join()`, and the reader ends on EOF of the output pipe, which ConPTY delivers only after `ClosePseudoConsole` has finished. Reading `conpty.rs` against the ConPTY facts in the brief:

1. **(a)** EOF needs `ClosePseudoConsole`. The old code did start the close on a helper thread when the exit wait fired, so it did not deadlock by (a).
2. **(c) The close could never complete when conhost asks a question at shutdown.** After the exit wait fired, the controller sent the writer its closing marker (and `CancelSynchronousIo`) before waiting for the reader. So from that moment no input reached conhost, and a shutdown cursor-position request (`ESC [ 6 n`) went unanswered. The emulator's own reply is routed through the widget's `feed_child`, which also needs the writer, the main thread and an attached session. With the close blocked, the output pipe never reaches EOF, `reader.join()` never returns and `Output::Exited` is never sent. This matches both runner symptoms: output present, exit never reported.
3. **No bound anywhere.** `reader.join()`, the close and the writer had no timeout, so any stall in ConPTY became a permanent hang. A failed spawn also closed the console on the spawning worker, and one error path (monitor-handle duplication failing) dropped the console while the reader was still blocked waiting for its session, a deadlock.
4. `Resize` could call `ResizePseudoConsole` with a handle already handed to the closer thread (use after close).

`PSEUDOCONSOLE_*` flags: none are passed (flags = 0), so (d) does not apply; startup never waits for a cursor reply.

### Panel test

`panel.rs` is correct as written: `close_current()` / `close_pane()` remove the pane from the model **synchronously**, and a `child-exited` handler calls `close_pane` for a shell that exits by itself. With the old test, "one pane remains" can therefore only time out if the pane count was not 2 before `close_current()` (0 or 3+), or if `current_pane()` returned nothing. The exit signal cannot explain that. I could not find which from the code (the brief's log lines do not include the pane count), so the new test asserts the count before and after, and its failure message lists every pane (id, child id, closed flag, grid text) next to the backend timeline. If run #6 still fails at this step, that dump will show it. The rewritten test also covers the intended sequence: typed `exit` closes its pane through `child-exited`, resize of the survivor, input, then `close_current` of a running shell, then the session's release.

## New shutdown sequence

The portable state machine is `crates/impro-vte/src/lifecycle.rs`: no clock, no threads, driven by timestamped events. The driver in `windows/conpty.rs` performs its actions.

```
Running --process exited--> Draining --quiet 120 ms (no output) or 1 s after exit--> Closing --exited && reader EOF--> Done
Running|Draining --stop (widget closed / hang-up)-----------------------------------^
Draining --reader EOF------------------------------------------------------------------^   (EOF before exit: close at once)

Draining : child gone, reader still delivering to the main context; the main thread is never blocked
           (the controller waits on a channel with the next deadline as timeout)
Closing  : action StartClose -> ClosePseudoConsole on a helper thread; the reader keeps draining
           until EOF; the writer stays alive; the reader answers a shutdown ESC[6n itself
           (ESC[1;1R), so nothing depends on the main thread
           + stop requested: TerminateProcess if the child outlives 250 ms
           + no EOF 3 s after the close began: CancelRead (CancelSynchronousIo on the reader,
             repeated every 50 ms), further output discarded
           + still no EOF 2 s after the first cancel: reader abandoned, exit reported "forced"
Done     : action Emit (exactly once): exit code via GetExitCodeProcess, session removed from the
           registry, Output::Exited posted -> widget emits child-exited once on the main context
           then: writer told to stop, joined; reader joined if finished (else detached);
           close helper joined if ClosePseudoConsole returned (else detached); "resources released"
```

Covered by the same sequence: normal exit; exit with output in flight (each read or finished packet restarts the quiet period, capped at 1 s); instant exit (quiet period measured from the exit); EOF before the exit is observed; stop while running (close starts at once, terminate after the grace period, no terminate if the child leaves in time); stop during the drain; failed spawn (`abort_start`: close off-thread, draining, bounded wait, cancel and abandon, then the error is returned); duplicate events (single Emit).

Behaviour kept: 2 KiB packets, one in flight with acknowledgement, 256-byte parser slices and the 2 ms budget, no `RefCell` borrow across a signal. The reader now attaches to its session lazily (on the first chunk) so the pipe is drained while the process is still being created; spawn-success is still delivered before any output.

## Diagnostics format

`IMPRO_TERM_DIAG=1` (any value except empty or `0`) prints one line per event to stderr and keeps it in memory (`impro_vte::diag::timeline()` / `timeline_text()`, used by the tests):

```
[impro-term +  1.234s #2 pid=4321] <event>
```

`+seconds` counts from the first diagnostic line, `#n` numbers spawn requests, `pid` is `-` until the process exists. Events, in the order a healthy session produces them:

| Event text | Meaning |
|---|---|
| `spawn requested argv=[..] size=CxR` | `spawn_async` reached the worker |
| `process created` | `CreateProcessW` succeeded (pid now shown) |
| `first output byte (N bytes)` | first read from the output pipe |
| `process exit observed (exit code N)` | the process wait fired |
| `stop requested (widget closed or hang-up)` | `Session::stop` was processed |
| `drain started; closing the pseudo console` | drain ended, `StartClose` performed |
| `ClosePseudoConsole called` / `returned after N ms` | the helper thread's call and its duration |
| `cursor-position request answered by the reader` | a shutdown `ESC[6n` was seen and answered |
| `grace period over; terminating the child` | stop requested and the child outlived 250 ms |
| `reader did not reach EOF; cancelling its read` | 3 s after the close began (repeats every 50 ms) |
| `reader EOF (<reason>; N bytes read)` | the reader ended: `end of file` or `read failed: ...` |
| `child-exited posted (status N[, reader abandoned])` | controller sent `Output::Exited` |
| `child-exited emitted (status N)` / `suppressed (...)` | widget emitted the signal / widget closed or repeat |
| `resources released (reader joined: B, close returned: B)` | all handles of the session released |
| `spawn failed: ...` | the spawn error |

The Windows tests set the variable themselves. `windows_conpty` waits with named conditions and, on timeout, panics with which conditions were still false, the grid text of every terminal it created and the whole timeline; `windows_panel` does the same with the panes dump. Timeouts stay at 30 s with phase-named labels. New checks: `child-exited` exactly once (300 ms settle), a widget closed while its shell runs releases the session and emits nothing.

## Files

- `crates/impro-vte/src/lifecycle.rs` (new, portable): `Lifecycle`, `Timing`, `Action`, `Phase`, `CprScanner`.
- `crates/impro-vte/src/diag.rs` (new, portable): timeline logger.
- `crates/impro-vte/src/lib.rs`: both modules are public under `cfg(any(windows, feature = "emulator"))`.
- `crates/impro-vte/src/windows/conpty.rs`: controller rewritten around the state machine; reader/closer/abort helpers; lazy attach; `Session::diag`.
- `crates/impro-vte/src/windows/widget.rs`: logs the `child-exited` outcome.
- `crates/impro-vte/tests/lifecycle.rs` (new, feature `emulator`): 16 tests.
- `crates/impro-vte/tests/windows_conpty.rs`, `crates/impro-terminal/tests/windows_panel.rs`: diagnostics and the new sequences.
- No dependency, lockfile, workflow, Unix or `sys/` change. `crates/impro-terminal/src` needed no change.

## Verification (Linux host, shared target cache, serialised and memory-capped)

| Command | Result |
|---|---|
| `scripts/windows/check.sh -p impro-vte -p impro-terminal --all-targets` | exit 0 |
| `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh -p impro-vte -p impro-terminal --all-targets -- -D warnings` | exit 0, no warnings |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo clippy -p impro-vte --features emulator --all-targets -- -D warnings` | clean |
| `cargo test -p impro-vte --features emulator` (xvfb) | all green: 16 lifecycle, 13 emulator, 1 contract, plus unchanged |
| `cargo test -p impro-vte -p impro-terminal` (xvfb) | all green, counts identical to before (panesplit 14, resizegate 20, terminal_bar 55, dup 4, panel 38, resize 5, startup 12) |
| `cargo +1.88.0 check -p impro-vte --features emulator` | exit 0 |
| `cargo deny` | not run: the tool is not installed on this machine. No manifest or lockfile change, so the result cannot differ from the last run |
| `scripts/ci/run-tests.sh all` | not run: only `impro-vte` and `impro-terminal` changed and both were tested above |

## UNVERIFIED until the runner

- That the unanswered shutdown request is the actual blocker (cause item 2). If conhost 26100 blocks `ClosePseudoConsole` for another reason, the reader is cancelled after 3 s and the exit is still reported (about 3 to 5 s late), so the tests should pass with a visible `reader did not reach EOF` line.
- That `CancelSynchronousIo` interrupts the reader's blocking `ReadFile` on that pipe.
- The real exit codes: `echo` is expected as 0, `exit 7` as 7, a hung-up shell as any code (the test only counts).
- Why the panel test failed at "one pane remains" (see Panel test).
- Per-test wall time: each exit now takes at least the 120 ms quiet period.

## Runbook

For the CEO's workflow owner: run both tests with the variable set and `--nocapture`, and upload the full output (even on success) as an artifact.

```
set IMPRO_TERM_DIAG=1
cargo test -p impro-vte --test windows_conpty -- --nocapture
cargo test -p impro-terminal --test windows_panel -- --nocapture
```

(bash: `IMPRO_TERM_DIAG=1 cargo test ...`.) The tests also set it, so it is only needed to see the timeline of a passing run live. Read the timeline for the session in question (`#n`):

| What the timeline shows | Outcome |
|---|---|
| ... `process exit observed` -> `drain started` -> `ClosePseudoConsole returned after <500 ms` -> `reader EOF (end of file` -> `child-exited posted` -> `child-exited emitted` -> `resources released` | Healthy. Fix confirmed |
| `ClosePseudoConsole called` with **no** `returned`, preceded by `cursor-position request answered by the reader`, then `reader EOF` | The shutdown request is real and our answer unblocks the close |
| `ClosePseudoConsole called`, no `returned`, no answered request, then after about 3 s `reader did not reach EOF`, then `reader EOF (read failed...)`, `child-exited posted` | The close blocks for a different reason; the fallback works. The close duration and what was last read (grid dump) identify the cause |
| ... `reader did not reach EOF` repeated for 2 s, then `child-exited posted (... reader abandoned)` | `CancelSynchronousIo` does not interrupt the read; switch to `CancelIoEx` on the pipe handle or closing the read handle from the controller |
| No `process exit observed` | The process wait never fired: handle duplication or the shell is still alive (check `pid` against the process list); the test's own conditions show it |
| `process exit observed` but no `drain started` | The quiet period never ended: output keeps coming (check `first output byte`/grid), capped at 1 s, so a hang here would be a bug in the machine |
| `child-exited posted` but no `child-exited emitted` | Main-context delivery: `suppressed` means the widget was closed or the exit repeated, otherwise the GLib channel was not dispatched (`iteration` not running) |
| `child-exited emitted` but the pane stays (panel test) | Compare with the panes dump: the pane count and ids decide whether the panel or the signal is at fault |
| `resources released` missing after `child-exited posted` | The release steps hung (writer cancel loop or joins); the controller thread is stuck after the exit report |

Panel test failure labels: `both children` (a spawn did not finish), `input reaches both panes`, `resized grids`, `typed exit closes its pane` (the corrected-signal path), `survivor resized`, `surviving pane input`, `closed pane releases its session`. The `conditions:` block says which condition stayed false.

## Open issues / assumptions

- Cause is inferred (above); STATUS stays PARTIAL until the runner confirms it.
- Timeouts (`quiet` 120 ms, `max_drain` 1 s, `grace` 250 ms, `close_timeout` 3 s, `abandon` 2 s) are constants in `Timing::default()`; the failed-spawn path uses the same values.
- When the reader is abandoned its thread and the close helper may stay blocked in the background, holding only their own handles; the child is already gone, and the exit is reported.
- A shutdown cursor-position request is answered with row 1, column 1 by the reader; the emulator's answer to ordinary requests is unchanged. During a stop with a still-running child, the stray reply may reach the child's input within the 250 ms grace period.
- Workflows are not owned here: the Windows test steps should run the two commands above (or any step running these tests) with `--nocapture` and keep their output as an artifact.
