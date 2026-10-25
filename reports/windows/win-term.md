# Windows terminal

STATUS: DONE

All Linux-verifiable acceptance items pass (see Verification). Windows runtime behaviour is UNVERIFIED on this Linux machine: the Windows-only tests compile but are to be run on the GitHub Windows runner.

## Design decision and evidence

Selected `alacritty_terminal` **0.26.0**, pinned in the manifest and cargo-generated lockfile, with its optional serialization feature disabled. Its published MSRV is 1.85 and its license is Apache-2.0. It supplies the mature grid, scrollback/reflow, selections, character widths, attributes, hyperlinks, terminal modes, damage tracking, and VT parser. The portable adapter also uses `regex` and `unicode-width` 0.2.2. These dependencies are optional on Linux and mandatory only on Windows. `pangocairo` and `windows-sys` 0.61 are Windows-only direct dependencies already used elsewhere in the workspace.

The spike compiled this candidate on Linux and for `x86_64-pc-windows-gnu`, then exercised its parser, selection and resize behavior. Reviewed the published APIs of `portable-pty` 0.9.0 (MIT) and `vt100` 0.18.1 (MIT, MSRV 1.85); neither alternative was compiled or added. The former would still require a grid implementation; the latter would require more selection, hyperlink, damage and rendering glue. No license-policy expansion was needed.

References: [emulator API](https://docs.rs/alacritty_terminal/0.26.0/alacritty_terminal/), [PTY alternative](https://docs.rs/portable-pty/0.9.0/portable_pty/), [parser alternative](https://docs.rs/vt100/0.18.1/vt100/).

Used a small owned ConPTY layer instead of the emulator's integrated PTY event loop. This permits GLib completion callbacks, cancellation/timeouts, explicit process-handle ownership, and GTK-friendly delivery without importing a second event loop. Followed the [Microsoft session lifecycle](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session), including non-inherited pipes, an aligned process-attribute list, releasing ConPTY pipe ends after process creation, and independent read/write threads. [ClosePseudoConsole](https://learn.microsoft.com/en-us/windows/console/closepseudoconsole) may block on older Windows, so closing and the 250 ms process termination watchdog run independently while the output reader remains active.

## Files and behavior

- `crates/impro-vte/src/emu/`: GTK-independent screen adapter, selection/text/HTML extraction, OSC metadata, URL matching, mouse packets, complete ordinary xterm keyboard tables and Windows command-line quoting. Tracking preserves row coordinates after history eviction and custom tab stops across reset.
- `crates/impro-vte/src/windows/conpty.rs`: dynamically resolved ConPTY API, RAII pipe/process/attribute/console ownership, UTF-16 working directories and environment, process creation, cancellation checkpoints, reader/writer/controller/waiter workers, resize and shutdown. The GTK spawn deadline can report cancellation before a slow system call finishes; any subsequently created child is shut down.
- `crates/impro-vte/src/windows/widget.rs`: real `gtk::DrawingArea` subclass; Pango/Cairo drawing, palette/truecolor and attributes, wide/combining glyphs, cursor settings, scrolling, mouse selection/reporting, IME commit/preedit, clipboard, titles and signals. Glyph layouts are cached by text and font style; terminal damage limits redraw regions. No emulator borrow is held while invoking GTK or user callbacks.
- `crates/impro-vte/src/stub.rs`: existing opt-in fixture retained under `stub-backend`; Windows now exports the real backend. `src/unix.rs` and everything under `sys/` are unchanged.
- `crates/impro-terminal/src/{panel,startup,resizegate}.rs`: Windows shell discovery/arguments, process-id conversion and real hangup, and output-driven redraw pacing. Configured shell settings take priority, then `SHELL`, PowerShell 7 on PATH or in Program Files, Windows PowerShell, and `ComSpec`/cmd. PowerShell receives `-NoLogo`; cmd receives no Unix `-i`. Windows startup sets `TERM_PROGRAM=impro` and `COLORTERM=truecolor`, without adding TERM.
- `crates/impro-terminal/tests/*.rs`: existing Unix shell fixtures are explicitly Unix-only; all their Linux test bodies remain intact. New `windows_panel.rs` covers live two-pane input, allocation and closure. The WebKit QA example has a non-Unix stub main; the dock demo keeps its explicit bash setting only on Unix.
- New portable tests, a native VTE contract comparison, Windows ConPTY/widget integration tests, and `examples/emulator_bench.rs`. The Windows integration target uses a single GTK test thread, explicit predicates, 30-second deadlines and repeated instant exits.
- `crates/impro-vte/Cargo.toml` and `Cargo.lock`: isolated dependencies; cargo alone wrote the lockfile. No root manifest, host crate, workflows, packaging, dependency-policy or system binding changes.

## Process, output and PTY semantics

Each reader posts at most one **2 KiB** output packet and waits for its acknowledgement. GLib consumes at most 256 bytes per parser call and yields after a 2 ms dispatch budget, retaining a partially consumed packet for the next idle dispatch. The source removes itself when that packet finishes. Packet destruction decrements the unread counter even if the widget disappears. Spawn success is delivered before activating reader/exit delivery; `child-exited` is emitted once, after output has drained. Output signals, title signals and clipboard callbacks execute without a borrowed screen or session cell.

`Pty::unread_bytes()` counts bytes already read from ConPTY but not yet consumed by the emulator. It deliberately excludes bytes still inside the OS pipe. `canonical_or_echo()` returns **true** because ConPTY has no termios query; early input follows the existing bounded one-second startup grace period. `fd()` is **-1**. `size()` reports the last successfully applied ConPTY dimensions, in columns/rows order. A 16 ms one-shot debounce coalesces widget allocations; the controller skips redundant dimensions.

On Windows the redraw gate listens to the output-driven `contents-changed` delivery, marks output as observed, and releases after the posted packet is consumed. There is no polling source or fake Unix descriptor. Apart from focused/enabled cursor blinking, a settled terminal has no repeating GLib source. Windows idle CPU, draw time and key-to-paint latency require runner measurements; Linux parser timings alone do not prove those targets.

The spawn callback carries a real process HANDLE in `glib::Pid`, borrowed from the terminal session until child exit. The panel immediately uses `GetProcessId`; it never truncates a HANDLE into an integer. Numeric ids resolve through a weak session registry for hangup. Closing the widget requests background teardown. Closing the pseudo console begins before the 250 ms grace timeout; a remaining child is terminated, workers finish, and owned handles close.

The environment merges inherited values and supplied overrides case-insensitively, with a sorted double-NUL-terminated UTF-16 block. Explicit `FILE_AND_ARGV_ZERO` is handled. Windows executable lookup uses CreateProcess search rules; Unix-only session, controlling-terminal, descriptor-inheritance and redirection flags have no meaningful ConPTY equivalent and are ignored. `PtyFlags` remains the shared value type; this wrapper's spawn API always uses default PTY flags on both backends. Primary selection operations are intentional no-ops on Windows.

## API parity

All public methods in the Unix wrapper remain callable with the same arguments and result shapes, subject to the inherited platform differences below.

| Unix public methods | Windows implementation |
|---|---|
| `init`, `Default::default`, `Terminal::new` | Real GTK drawing-area terminal; normal GTK-thread assertion |
| `spawn_async`, `spawn_async_path` | Worker ConPTY creation; cwd, argv, environment, flags, timeout, cancellable and GLib completion |
| `set_size`, `column_count`, `row_count` | Emulator grid plus debounced ConPTY resize |
| `set_scrollback_lines`, `scrollback_lines` | Primary-grid history configuration and round-trip value |
| `pty` | Shared live session; absent before spawn and after exit |
| `Pty::fd` | `i32`, -1; no Windows fd |
| `Pty::size`, `Pty::unread_bytes`, `Pty::canonical_or_echo` | Semantics described above |
| `reset` | Reset modes/parser; independently preserve content/history/cursor and custom tabs according to the two flags |
| `has_selection`, `select_all`, `unselect_all` | Engine selection; soft-wrap-aware text, wide cells and hard-line endings |
| `copy_clipboard_format`, `paste_clipboard` | Text/HTML clipboard targets; asynchronous paste, bracketed-paste filtering |
| `copy_primary`, `paste_primary` | Intentional no-ops |
| `set_font`, `font`, `set_font_scale`, `font_scale` | Pango description and scale; cache invalidation and grid recalculation |
| `char_width`, `char_height` | Rounded font metrics actually used for drawing and hit testing |
| `set_cursor_shape`, `cursor_shape`, `set_cursor_blink_mode`, `cursor_blink_mode` | Widget defaults plus terminal cursor sequences; focused/mapped cursor blink |
| `set_colors`, `set_default_colors` | Default colors, validated 16/256 palette, 256-color cube and truecolor |
| `set_color_cursor`, `set_color_cursor_foreground` | Block/beam/underline cursor colors |
| `set_color_foreground`, `set_color_background` | Default cell colors |
| `set_color_highlight`, `set_color_highlight_foreground` | Selection colors |
| `set_audible_bell`, `audible_bell` | Stored setting and display beep; coalesced within a packet |
| `set_scroll_on_output`, `scroll_on_output`, `set_scroll_on_keystroke`, `scroll_on_keystroke` | Viewport-follow behavior |
| `set_mouse_autohide`, `mouse_autohide` | Hide pointer on typed input; restore on motion |
| `set_allow_hyperlink`, `allow_hyperlink`, `hyperlink_check_event` | OSC 8 and URL detection; hovered link cells underlined |
| `set_input_enabled`, `input_enabled` | Gates keyboard, IME, paste and mouse reporting; direct programmatic `feed_child` stays available |
| `set_bold_is_bright`, `bold_is_bright` | Renderer/HTML palette brightening |
| `window_title` | OSC 0/2 title |
| `current_directory_uri` | OSC 7 metadata and property notification |
| `current_file_uri` | None; no Windows shell file metadata is synthesized |
| `feed`, `feed_child` | Parse display bytes / enqueue bytes for the input worker |
| `connect_selection_changed`, `connect_contents_changed` | Real selection/output signals |
| `connect_window_title_changed`, `connect_current_directory_uri_changed` | Title signal / cwd property notification |
| `connect_child_exited`, `connect_commit` | Exactly-once exit status / user-typed data |
| `text_format`, `text_selected`, `text_range_format` | Plain or styled HTML; native VTE extraction contract checked on Linux |
| `cursor_position` | Column and absolute row, retaining the coordinate epoch when history is evicted |
| `set_rewrap_on_resize`, `rewrap_on_resize` | Grid resize with or without reflow; tabs/margins/inactive grid still updated |

Inherited platform differences: counts/grid coordinates use `i64` rather than Windows `libc::c_long`; adjustment getters are inherent `Option<gtk::Adjustment>` methods instead of a `Scrollable` interface; `ffi` is absent. The Windows parent changed from the fixture's Box to DrawingArea while preserving Widget identity and the existing signal names. `hadjustment()` remains an inert adjustment because terminal output wraps rather than scrolling horizontally. `maximum_output_slice()` is an extra Windows diagnostics getter used by runner tests.

Empirical extraction detail: the Unix wrapper's comments describe all-history text and an inclusive rectangle, but libvte actually returns the current viewport for `text_format`, and a linear span with an exclusive final column for `text_range_format`. `emulator_contract.rs` compares the actual native output, selection, cursor and reset behavior against the adapter. Select-all and explicit absolute ranges expose retained scrollback. No Unix source was changed to resolve this documentation discrepancy.

## Verification

All cargo commands used the shared warm target cache with incremental compilation disabled, serialised and memory-capped (`flock ../.heavy.lock systemd-run --user --scope -p MemoryMax=3500M`, release 5000M). GUI tests ran under `env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- ...`.

| Check | Result |
|---|---|
| `scripts/windows/check.sh -p impro-vte -p impro-terminal --all-targets` | exit 0 (lib, tests, examples incl. `webkit_host_qa` stub main and `windows_conpty`/`windows_panel`) |
| `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh -p impro-vte -p impro-terminal --all-targets -- -D warnings` | exit 0, no warnings |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo clippy -p impro-vte --features emulator --all-targets -- -D warnings` | clean |
| `cargo test -p impro-vte -p impro-terminal` (debug) | all green; impro-terminal unit 1, panesplit 14, resizegate 20, terminal_bar 55, dup 4, panel 38, resize 5, startup 12 (performance: 1 ignored), identical to main; impro-vte `spawn_path` 1 and `terminal` 1 unchanged |
| `cargo test --release -p impro-terminal` | all green, same counts |
| `cargo test -p impro-vte --features emulator` | all green: 13 portable emulator tests, 1 native-VTE contract comparison, plus the unchanged VTE tests |
| `scripts/ci/run-tests.sh all` | exit 0 in 10m25s: unit, integration (incl. fatal-criticals group), doc and JS groups all green (126 passing cargo test-result lines, 0 failures; Node 19 pass / 0 fail); the known load-sensitive `impro-markdown` timing test passed this run |
| `cargo deny --locked check advisories bans licenses sources` (as in `security.yml`, cargo-deny 0.20.2) | `advisories ok, bans ok, licenses ok, sources ok` |
| same with `--all-features`, and with `--target x86_64-pc-windows-gnu` | all four ok (only the existing duplicate/wildcard warnings); the Windows graph contains `alacritty_terminal` 0.26.0, `vte` 0.15.0, `miow`, `piper` and passes the unchanged licence allow-list. `deny.toml` is unchanged |
| `cargo +1.88.0 check -p impro-vte --features emulator` | exit 0 (MSRV) |
| `cargo tree -p impro -e normal --target x86_64-unknown-linux-gnu \| grep -ciE 'alacritty\|miow\|piper'` | `0` (also `0` with `-e normal,build,dev`, and no `rustix-openpty`/`signal-hook` in that tree). `Cargo.lock` lists the emulator's unix-only packages, but they are reachable only through the optional `emulator` feature, which no default build enables; the Linux release dependency graph, binary and startup are unchanged by construction |
| Forbidden-word gate over tracked files and over `git log main..HEAD`, local-path scan, `git diff --check` | all empty |

### Emulator throughput (Linux, release, `cargo run --release -p impro-vte --features emulator --example emulator_bench`)

50 MiB of escape-heavy output (truecolor SGR, CJK, combining marks, bold, 120x40 grid, 10 000 lines of history) fed in 2 KiB packets, as the ConPTY reader delivers them:

`bytes=52427856 batches=25852 seconds=0.468 throughput_mib_s=106.76 mean_batch_us=18.12 max_batch_us=2010.74`

So parsing costs about 18 us per packet (about 0.5 % of the 4 ms budget); the 2 ms maximum is a single scheduler outlier on a shared machine, and the widget additionally enforces its own 2 ms slice budget. This is the emulator only: Windows draw cost and key-to-paint latency are not measurable here.

### Self-review of `widget.rs` and `conpty.rs` (continuation pass)

(a) RefCell borrows: every `borrow`/`borrow_mut` in the widget is a statement-scoped temporary or a block that ends before any `emit_by_name`, `notify`, `queue_draw*`, clipboard or callback call; `paint` snapshots cells/colours before Pango; the adjustment `value-changed` handler uses `try_borrow_mut`. No defect found.
(b) Main-thread cost: output is consumed in 256-byte parser calls with a 2 ms budget per idle dispatch; no repeating source exists except the 600 ms blink, which is installed only while focused, mapped, input-enabled and the cursor visible. Fixed: `contents-changed` was emitted after every 256-byte slice (up to nine times per packet); it is now emitted once per consumed packet (and by the public `feed`). Fixed: `paint` filled a rectangle for every cell; cells whose background equals the default background are no longer filled (the background is painted once).
(c) Handles/threads: pipes, process, duplicates, attribute list and pseudo console are RAII; the reader, writer, waiter, controller and closer threads are joined or end on handle closure. Fixed: if duplicating the monitor handles failed after `CreateProcessW` succeeded, the child was left running unsupervised; it is now terminated. Instant exit, failed spawn and cancellation paths were traced and release everything once.
(d) `child-exited`: sent once by the controller after the reader is joined (so after all output was acknowledged), guarded in the widget by `exited.replace(true)`.
(e) Missing ConPTY: `Api::load` fails with "ConPTY requires Windows 10 version 1809 or newer"; the widget prints that message in the terminal area and returns it as the spawn error.

## Windows runner checklist — all UNVERIFIED here

1. Run `cargo test -p impro-vte --test windows_conpty -- --nocapture` and `cargo test -p impro-terminal --test windows_panel -- --nocapture` on a GTK-capable Windows runner. First inspect ConPTY creation, argv/path quoting, environment, handle inheritance and callback-before-output/exit ordering. The integration test repeats instant cmd exits and checks failure/cancellation paths.
2. Confirm numeric exit 7, exactly-once exit, explicit hangup and closing a pane/window. Watch for stuck readers/close calls, lingering child processes and accumulating handles after repeated splits.
3. Check PowerShell availability and ANSI output, both-pane typing and actual ConPTY size acknowledgements. Manually exercise PowerShell 7, Windows PowerShell, cmd, configured shells, full-screen alternate-screen programs and continuous drag-resize.
4. Inspect the reported longest GTK slice during 20000 output lines; test asserts below 50 ms and verifies GTK timeout dispatch and scrollback growth. Measure the stricter 4 ms consumption, 8 ms key-to-paint and idle-CPU targets on Windows; they are not established by cross compilation.
5. Exercise clipboard ownership/HTML targets, selection dragging, wheel/mouse protocols, OSC 8/regex links, IME preedit/commit and non-US AltGr. Check glyph metrics/fallback, combining text, emoji, palettes, cursor modes, colors and draw cost on the CI desktop.

## Open issues / assumptions

- Windows binaries cannot be linked or executed on this machine; cross compilation validates Rust types, not Windows DLL availability or runtime behaviour. Everything in the runner checklist is unverified, not passed.
- Suspected, not measurable here: (1) `paint` redraws the whole viewport per damaged region with one Pango layout per non-blank cell (layouts are cached); if the runner shows draw cost above the 8 ms key-to-paint target, a per-row text-run layout is the next step. (2) One 2 KiB packet is in flight at a time (a handshake with the reader thread), which bounds throughput of floods such as `type` on a large file; the main thread stays responsive by design but wall-clock throughput should be measured. (3) `AltGr` detection combines `GetKeyState(VK_RMENU)`, GDK's `MOD5` and the keymap's consumed modifiers; exotic layouts need a manual check.
- The existing Unix shell-fixture integration targets of `impro-terminal` are gated to Unix and replaced on Windows by `windows_panel.rs`; Linux test bodies, native VTE source and the binding are unchanged.
- The dependency policy graph (`deny.toml` `targets`) stays Linux-only. The Windows and all-features audits above were run ad hoc and are clean; a later CI task should add `cargo deny --target x86_64-pc-windows-gnu` and `cargo test -p impro-vte --features emulator` to the workflows (not owned by this task).
- `impro-markdown table_widgets::test_two_hundred_rows_build_in_bounded_slices_and_type_fast` is a pre-existing load-sensitive timing test (it can fail on plain main under load); it passed in the final full run.
