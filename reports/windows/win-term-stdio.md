# win-term-stdio

STATUS: DONE

## Cause
Run #7 evidence: the child created by `CreateProcessW` in `crates/impro-vte/src/windows/conpty.rs` inherited the
parent's redirected standard handles instead of using the pseudo console. `cmd.exe /d /c echo impro-ok` sent
`impro-ok` to the test process's stdout pipe (it shows in the runner log); the console host's output stream held
only 86 bytes in 2 chunks, with no `impro-ok`. Two interactive `cmd.exe` in `windows_panel` printed their banner
and prompt to the runner log, not the pane grid. They read EOF from the runner's stdin and exited 0 about 2 ms
after the wait was signalled, with zero `input write` events.

## Fix
- `startup_info_for_pseudoconsole(attribute_list)` in `conpty.rs` builds the `STARTUPINFOEXW`. It sets
  `STARTF_USESTDHANDLES` and `hStdInput = hStdOutput = hStdError = INVALID_HANDLE_VALUE`, and keeps the pseudo
  console attribute. `bInheritHandles` stays FALSE.
- New `IMPRO_TERM_DIAG` event before process creation: `child stdio: explicit invalid handles (pseudo console only)`.
- `cfg(windows)` unit test `startup_info_uses_explicit_invalid_stdio_handles` asserts the flag, the three handles
  and the attribute list.
- Other child-creating paths: `CreateProcessW` is called in one place only. `hangup_pid` only signals an existing
  session. `windows_shell_argv` and `command_line` quoting are correct, including a shell path with spaces.
  No change needed in `impro-terminal`.
- The two Windows tests (`windows_conpty.rs`, `windows_panel.rs`) were re-read. Their expectations (`impro-ok` on
  the grid, shells alive, typed `exit` closes the pane) already describe the attached behaviour. They stay strict
  and unchanged.

## Verified
- `scripts/windows/check.sh -p impro-vte -p impro-terminal --all-targets`: clean.
- `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh -p impro-vte -p impro-terminal --all-targets -- -D warnings`: clean.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test -p impro-vte --features emulator` and `cargo test -p impro-terminal --lib` (xvfb): pass.
- The Win32 behaviour itself is not testable on Linux; run #8 is the proof.

## What to see in run #8 (Tests job)
`output chunk` lines contain `impro-ok`; `windows_terminal_lifecycle_and_widget` and
`windows_panel::split_resize_close_and_input` PASS; `RESULT windows-terminal: PASS`.

## Open issues
- `cargo deny` is not installed on this machine, so it was not run. No dependency or lockfile changes were made.
