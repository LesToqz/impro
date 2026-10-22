# Editor verification and host integration

The crate owns file/view tabs, file IO, edit history, shortcuts, find, syntax tags,
Cairo/Pango gutter drawing, Markdown controller lifecycle and stylesheet registration.
The window must use the public surface documented in `src/lib.rs`.

## Integration contract

- Call `init()` before applying the initial theme. Construct `EditorPanel::new(root)`
  on GTK's main thread. It is a native GtkBox.
- Opening is asynchronous. Use `open_file_with(path, done)` when the next action
  needs the tab. `open_file_at` handles its own completion, clamping and centring.
- Saves, file closure and quit requests take a completion callback. A window delete
  handler should prevent immediate destruction, call `request_quit`, then destroy
  the window only when its callback reports success. Bind Save and Save All through
  `save_current` / `save_all`. View tab closure is synchronous except when requested
  inside GTK's page insertion signal, where removal is deferred to the next idle.
- Connect `current-file-changed` to explorer selection / Git focus; connect
  `file-saved` to explorer refresh and Git updates. `dirty-changed` supports the
  window's unsaved indicator. Connect `error` to the host's error message dialog.
  The complete error text is also available through `last_error()`.
- Supply `buffer_text` as the explorer search overlay's text provider. Deleted
  buffers are intentionally excluded. File paths remain native PathBuf values;
  typed signal connectors also round-trip paths with invalid UTF-8 bytes.
- Use `add_view_tab_with_shutdown` for commit pages; supply their shutdown callback.
  No Git or terminal crate dependency is introduced by this crate.
- Read settings off-thread, then call `apply_settings` and `apply_theme` on GTK.
  The saved editor size is separately retained for Ctrl+0. Root resolution also
  runs on a worker; `is_busy()` includes pending root, read and save operations.

## Test coverage and remaining integration checks

The five requested suite filenames are retained, with upstream assertion names.
The syntax mapping cases for Python, JSON, Markdown, HTML/CSS, JavaScript,
TypeScript, shell, YAML, TOML, C, Rust, Go and SQL retain the source fixtures and
expected token roles. Additional tests cover incremental checkpoint equivalence,
symlink boundaries, settings, non-UTF-8 paths and GTK signal re-entry.

The gutter now draws directly rather than creating one GtkLabel for each row.
Tests compare its recorded draw geometry against independent GtkTextView line
positions, including the final empty line, font changes, scrolling, taller tagged
lines and a separate scale-two process. Native mouse navigation, GTK arrow/page/
home/end bindings, tab keyboard propagation, rounded tab corner pixels and status
hover pixels are exercised. The render loop uses cached digit layouts and a fixed
row array; inspecting `painted_rows()` is the allocating diagnostics API.

The following checks require other tasks and are not claimed as full host tests:

- `test_explorer_line_37_offscreen_screenshot`: real explorer activation, window
  layout and focus policy.
- `test_readable_width_positions_rendered_paragraphs_on_toggle_and_resize` and the
  renderer-specific assertions within Markdown tests: the Markdown crate currently
  provides no-op controllers by contract. Editor tests prove mode, source/caret,
  editability, project/note/outside classification, size limits, controller attach/
  detach and cancellation. Actual preview rendering, table/block editing, slash
  menu contents and selection-toolbar pixels need the completed Markdown crate.
- The real window/terminal shortcut policy, search reveal centring across final
  splitters, quit dialog ownership and live Git/commit page updates require the
  final window host. The standalone demo supplies an ordinary GTK window only.

The three tests about an optional Python lexer import have no runtime equivalent:
syntax support is a compiled Rust dependency. Plain/unknown files, inert destroyed
highlighters and skipped oversized files are tested instead.

The upstream 20k-line performance assertion is retained as an ignored timing test
so ordinary debug tests do not apply an optimized timing budget. Run it explicitly
with `cargo test --release -p impro-editor --test test_highlight -- --ignored` under
Xvfb on an idle machine. Its debug execution measured 15.34 s and passed the slice
budget; the optimized results are recorded in the task report.

## Performance and visual reproduction

`examples/editor_demo.rs` opens Python, Rust, Markdown and JSON tabs. Set
`IMPRO_DEMO_THEME`, `IMPRO_DEMO_FIXTURES` and `IMPRO_QA_SCREENSHOT` to capture a
settled 900x600 window. The task's reference captures and pixel differences are
under `.ceo/parity/`; the report records the measured result, not a universal
claim of identical syntax classification.

`examples/editor_perf.rs` prints first text paint, initial and incremental
highlighting, 200 edits, edit-to-draw latency, a rendered scroll sweep and idle
process CPU. Run the optimized executable under Xvfb. Draw latency ends after the
GTK default draw handler; it does not measure a physical keyboard or compositor scanout.
The event-driven display flush uses GTK3's GDK update API after text validation,
with no recurring timer. Highlighting snapshots only after debounce, reuses parser
checkpoints, and paints the changed span in short visible-first batches. Initial
syntax data loads inside the first worker job. Cancelled partial paints force a
fresh pass so cached lexer state cannot leave stale or missing tags.

Known lexer differences: TypeScript/React uses the bundled JavaScript grammar
with contextual type/decorator refinements; TOML and Dockerfile have local syntect
syntax definitions. Long lines and documents above the highlighting limits retain
language labels but skip syntax tags. This is independent of the 5 MiB open limit.
