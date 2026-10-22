# Settings window verification

Run the crate's tests (needs a display; counts per file are in the task report):

```sh
G_DEBUG=fatal-criticals GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- cargo test -p impro-settings-ui
cargo clippy -p impro-settings-ui --all-targets -- -D warnings
```

| File | What it covers |
| --- | --- |
| `tests/test_settings_dialog.rs` | Port of `test_settings_dialog.py`, same test names |
| `tests/test_graph_prefs.rs` | Graph preference validation (graph cases of `test_settings.py`), hotkey merge |
| `tests/test_writer.rs` | Write-behind coalescing, flush, per-path routing, failure reporting |
| `tests/test_settings_wiring.rs` | The window wiring cases that can run against a stand-in owner |

Deliberate differences from the Python test file: the former `ListBox` shim
(`theme_list.select_row(theme_rows[..])`) is not ported; pages are sections of one scrolled
column, so `stack.get_visible_child_name()` became "exactly one section visible" (`visible_page`
in `tests/common`); saves are write-behind, so tests call `flush()` before reading the file.

## Cases for the window task (need the real `MainWindow`)

The source cases of `test_settings_wiring.py` below need the final application window.
The dialog half of each is already covered by the stand-in tests named in brackets.

- `test_gear_accelerator_lazy_reusable_nonmodal_dialog`: `window._settings_dialog` is `None` before
  the first open; the File menu has no Settings entry; the title-bar gear opens the dialog
  (transient for the window, not modal); `Ctrl+,` through the accelerator group calls
  `open_settings` once and reuses the same dialog. [stand-in:
  `test_gear_accelerator_lazy_reusable_nonmodal_dialog`]
- `test_selection_toolbar_setting_loads_and_notes_toggle_applies_live`: with
  `notes_selection_toolbar` off the editor's selection popover stays hidden; toggling it on in the
  dialog (page `notes`) shows it on the next selection, toggling off hides it at once.
- `test_settings_shortcut_with_editor_and_both_terminal_docks_focused`: `Ctrl+,` opens the dialog
  while the editor view, the bottom terminal and the right terminal have focus.
- `test_live_sizes_future_content_reset_and_reopen`: moving the three font-size sliders updates the
  editor (existing and future tabs), the explorer cell size and every terminal pane; a second window
  starts with the stored sizes; Restore resets all three live. [stand-in:
  `test_live_sizes_future_content_reset_and_reopen`]
- `test_failed_save_still_changes_live_and_reused_dialog`: when the file cannot be written the window
  still applies the sizes, the error notice reads `Could not save settings to <path>` (the window
  registers it with `set_save_failed_handler`) and the reopened dialog shows the live sizes.
  [stand-in: `test_failed_save_still_changes_live_and_reused_dialog` in `test_writer.rs`]
- `test_invalid_saved_sizes_are_safe_before_first_frame`: a settings file with invalid or
  out-of-range sizes gives clamped sizes in editor/explorer/terminals before the first frame and in
  the dialog. [stand-in: `test_invalid_saved_sizes_are_safe_before_first_frame`]
- `test_terminal_base_font_preserves_zoom_and_commits_gate` and
  `test_font_changes_preserve_real_shell_prompt_in_both_docks`: terminal-panel behaviour driven by
  `font-size-changed`; they need the real terminal docks and a shell.

Further window contract checks (no Python equivalent):

- `F1` calls `present(Some("hotkeys"))`; the gear/`Ctrl+,` path calls `present(None)`.
- Before each `present`, the window calls `set_values` with the live values (the file values
  overridden by what is applied), `set_theme_id`/`set_sizes` or the combined `set_values`, and
  `set_hotkeys(&merge_hotkeys(..))` (an unchanged list is free).
- Changes made elsewhere (graph pane settings, git graph orientation, theme cycling with
  `Ctrl+Alt+T`) reach an existing dialog through `set_values({"graph": ..})`,
  `set_values({"gitgraph_orientation": ..})` and `set_theme_id`; none of these emits.
- `setting-changed` for `reduce_motion` calls `impro_ui::motion::set_reduce_motion` and posts the
  value to the graph/git pages; `accent`, `ui_font`, `editor_font` re-run `apply_theme`.
- The window calls `impro_settings_ui::flush()` when it quits so the last change reaches disk, and
  persists the theme and font sizes itself (the dialog only signals them); use `save_async`
  for those writes to stay off the main thread.
- `set_project_root` is called on project change so the notes folder row is right.

## QA

`cargo run -p impro-settings-ui --example settings_demo -- [page] [--theme ID]` opens the window.
`--timing [--prebuild]` prints the timings quoted in the report; `--shots DIR --prefix P [--height N
--width N]` writes one PNG per page plus a search view (used for the pixel comparison against the
original dialog).
