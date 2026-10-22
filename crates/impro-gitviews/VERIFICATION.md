# Git page integration verification

The crate supplies GTK composition handles and callbacks; the application owns
its dock and editor registries. See the crate-level API documentation in
`src/lib.rs`. All paths passed to constructors should be repository/folder
paths. File-open callbacks supply canonical paths and one-based line numbers.

## Automated verification

```sh
cargo build --workspace
G_DEBUG=fatal-criticals GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- cargo test -p impro-gitviews
node crates/impro-gitviews/tests/js/index.js
cargo fmt --check
cargo clippy -p impro-gitviews --all-targets -- -D warnings
```

`test_gitviews` launches the `webkit_host_qa` example as a separate process so
all WebKit/GTK work runs on that process's main thread. Cargo's default test
command builds this example. When running just the integration executable,
build the example first. Browser assertion groups retain the source test case
names and cover the embedded documents, rather than loading test-only pages.
The standalone-script assertion checks two embedded nonce scripts instead of
two external script URLs. The missing-library test does not apply to the hard
WebKit dependency. Shutdown deliberately cancels/discards bounded Git work
without joining workers on GTK's thread.

`git_perf` generates 1,500 real commits using Git fast-import, records load and
two-frame paint milestones, samples WebKit process CPU/RSS through `/proc`, and
checks zero web processes before first use and after shutdown. A sample includes
WebKit's own housekeeping, so short idle samples can contain a scheduler tick.
No page schedules a repeating idle timer or a continuing animation frame loop.

```sh
G_DEBUG=fatal-criticals GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- cargo run -p impro-gitviews --example git_perf
cargo run -p impro-gitviews --example git_demo
```

`git_demo --export DIRECTORY` also exports embedded documents, palettes, and
real graph/detail JSON from a deterministic showcase repository for a pixel
comparison harness. It needs no display in export mode. All eight comparison
captures use identical data, colours, fonts, dimensions, motion preference and
viewer timezone.

## Real-window cases for the window task

These cases require the application window and are intentionally outside this
crate's ownership. Retain the source suite's case names:

- `test_settings_orientation_icons_options_and_startup_round_trip`: normalize
  and apply the dialog's orientation, 100–10,000 history limit, remote/tag
  visibility and timestamp values; save orientation changes and synchronize
  every graph and the settings dialog.
- `test_disabling_gitgraph_preserves_open_commit_tabs`: disabling the feature
  changes graph availability without removing already-open read-only commits.
- `test_menu_palette_accelerator_and_bottom_reuse`: wire Open git graph,
  Refresh, Jump to HEAD and Close commit tabs; Ctrl+Alt+H opens/reuses the wide
  bottom pane and focuses its WebView.
- `test_full_bottom_falls_back_right_and_full_docks_notice_once`: use the right
  dock when the bottom is full; report two full docks once.
- `test_chooser_both_docks_three_kinds_and_empty_dock_rules`: chooser entries,
  per-dock capacity, empty-dock rules and graph pane close behavior.
- `test_dot_commit_last_title_files_parent_reopen_and_close_signal`: use
  `connect_open_commit` on both hosts; resolve full hashes off-thread,
  deduplicate `commit:<hash>`, append read-only view tabs at the right end,
  label with seven hash characters, and connect tab close to `shutdown`.
- `test_selection_sync_files_commits_later_graph_and_graph_close`: apply the
  selected commit hash to every graph, including graphs created later; clear
  selection on file tabs. Closing a graph preserves the commit registry.
- `test_commit_file_links_existing_and_deleted`: open callback paths at
  `line - 1`; removed files produce no working-tree open callback.
- `test_theme_accent_fonts_motion_and_size`: send effective UI colours and
  explicit UI/editor font families, accent, exact syntax palette, motion
  preference and live editor font size to every page.
- `test_monitor_refresh_commit_survives_folder_change_closes_and_reroots`:
  refresh retains commit tabs; connect `connect_root_changed` to close the old
  repository's commit tabs before accepting new commit-open requests.
- `test_short_hash_worker_validation_dedup_and_stale_folder`: reject invalid
  hashes; resolve 4–39 character hex names on a worker; deduplicate pending
  requests and discard resolutions from an old folder epoch.
- `test_empty_states_and_error_notice_once`: display friendly repository and
  unborn-history states; deduplicate error notifications in the window.
- `test_real_repository_qa`: repeat graph orientations, dark/light themes,
  unified/split commits, parent/file links, refresh, folder switch, tab close,
  docking, keyboard navigation and process cleanup in the final application.

## Source asset differences

- All eight page assets and both core tests are copied with the product/bridge,
  JavaScript globals, CSS identifiers, comments and fixture content rebranded.
- JS test import paths point to this crate's `web` directories. The directory
  test entry point imports only the two Git test modules.
- Graph JS tracks the refresh feedback timer and clears it, the pulse timer,
  and pulse state on pause. Pulse and tooltip scheduling also reject hidden
  pages. Tooltip timers and animation frames already pause.
- Commit JS adds pause/resume: pause disconnects lazy observation, clears copy
  feedback and its timer, and freezes animations; resume observes unrendered
  expanded files. Observation/render entry points respect the paused flag.
- Commit CSS adds one paused-state animation/transition rule.
- At embedding time Rust replaces CSS links and script URLs with inline assets;
  commit scripts receive an explicit nonce and its CSP uses that nonce.
  Stored HTML files, graph CSS and both core JS files have no functional edits.

The host uses a single lazily created related-view process for all live Git
pages. The final page terminates it; closing any sibling cannot terminate it.
The eight 1100×700 comparison pairs in the task report show zero changed pixels.
