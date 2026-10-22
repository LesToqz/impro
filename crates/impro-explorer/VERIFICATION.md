# Completion audit

The continuation started at `7901df0`. Existing implementation was retained.

| Scope | Existing implementation | Verification needed at handoff |
| --- | --- | --- |
| Explorer tree and actions | Lazy sorted worker scans, chunked merge, icons, shortcuts, dialogs, trash, path signals, root changes and settings | Workspace build and GTK suite |
| Git decorations | Bounded hardened subprocesses, inherited folder status, snapshot differences, 150 ms debounce, metadata/current-file monitors | GTK and pure tests; benchmark |
| Notes | Feature gate, toggle, draggable divider, sorting, creation, monitoring and editor selection | Notes suite and screenshots |
| Search engine | Size/encoding/binary/symlink rules, caps, clipping, current-file priority, live buffer overlay, names and cancellation | Search suite and throughput |
| Search panel | Streaming rows, collapse, activation, keyboard behavior, case/regex controls and errors | GTK and wiring suites; dispatch latency |
| CSS and public API | Templates, registered builders, root exports and crate API guide already present | Check window calls against guide, theme parsing and lint |
| Tests | Six requested suites plus reentrancy suite already present | Original name comparison and full execution |
| Visual/performance evidence | Demo, performance example and earlier image pairs present | Fresh captures, inspect differences and record measured limits |

All original test names are present in `test_explorer_panel`, `test_gitstatus`,
`test_search`, and `test_notes_section`. The following original integration tests
require the eventual window/editor implementation and are deferred rather than
claimed as ports of the real host behavior:

- `test_window_wires_editor_save_and_focus_refresh` (git suite).
- `test_ctrl_f_is_still_in_file_find`.
- `test_ctrl_shift_f_restores_terminal_fullscreen_and_view_menu_item`.
- `test_ctrl_zero_resets_to_configured_editor_size_and_settings_update_it`.
- `test_markdown_preview_switches_to_source`.
- `test_open_file_at_clamps`.
- `test_result_is_scrolled_into_view_near_the_middle`.

The other twelve search-wiring tests use a GTK window and editor stand-in through
the public explorer API; they do not prove final application wiring. Additional
tests cover large folders, incremental updates, cancellation, Unicode lines,
quick-switcher listing, binary rejection and six signal reentrancy scenarios.
Final command results, timings and visual limitations are in `.ceo/report.md`.

## Performance continuation (perf2)

- Tree icons come from an asynchronous cache (`icons.rs`); the search entry's clear icon is warmed.
- Notes list: merged in place in 3 ms steps with fixed-height rows; edited texts are formatted on the worker.
- Every expanded folder is watched (`explorer/watch.rs`); bursts become one in-place re-listing.
- Paths with bytes that are not UTF-8 round-trip through `util::encode_path` / `decode_path`.
- GTK iterators are advanced through `util::next_iter` / `find_child` / `remove_*_row`; the earlier
  `iter_next(&iter)` loops only matched the first row in release builds (release test run:
  `cargo test --release -p impro-explorer`).
- Measurements: `examples/perf.rs` (`search`, `bigdir`, `notes`, `idle`); figures are in `.ceo/report.md`.
