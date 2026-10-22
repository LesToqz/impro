# Chrome integration checks

Run component coverage with:

```sh
G_DEBUG=fatal-criticals GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- cargo test -p impro-chrome
cargo clippy -p impro-chrome --all-targets -- -D warnings
```

The following source test names require the final application window and its
action registry. They are intentionally assigned to window integration:

- `test_all_accelerators_fire_actions`: activate every File/View accelerator and
  check the corresponding editor, settings or dock action, including the palette.
- `test_shortcut_palette_hide_last_closed_and_fullscreen_sync`: toggle the right
  dock by shortcut, palette callback, explicit hide, fullscreen restore and last
  pane closure. Check both visibility and title-bar active state after each step.
- `test_window_glyph_pixels_in_dark_and_light_including_restore`: with a window
  manager running, maximize and restore the real application in both themes;
  verify native minimize/maximize/restore/close marks remain cog sized. The demo
  and component tests verify unmagnified native controls and decoration rebuilds.

The window owns the File/View menu entries, command callbacks and accelerators;
chrome receives the menu bar. No action registration belongs in this crate.

`examples/chrome_demo.rs` runs without the application or feature crates:

```sh
cargo run -p impro-chrome --example chrome_demo -- palette impro-dark
```

Modes: `titlebar`, `palette`, `switcher`, `notices`. Ctrl+P opens the palette;
Ctrl+O opens the switcher. An optional third argument captures a PNG and exits.
The QA harness and dark/light reference, Rust and difference PNGs are under
`.ceo/parity/` in the task worktree. Equal-sized widget captures have no changed
pixels in all eight fixtures. The reference brand label is set to `impro` before
capture. All remaining widgets, CSS, menus, data and theme match the source.

Timing tests (optimized build, no benchmark dependency):

```sh
cargo test --release -p impro-chrome --test test_palette benchmark_rank -- --ignored --nocapture
GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- cargo test --release -p impro-chrome --test test_prompt --test test_overlay benchmark_ -- --ignored --nocapture
```

The quick switcher accepts a supplied file snapshot. Call `set_files(paths,
recent)` when enumeration finishes on the caller's worker. Lists over 5,000
entries are indexed, sorted and ranked on workers, with cancellable searches
and at most 100 delivered rows. `index_ready()` and `search_pending()` allow
integration tests to wait for deterministic results. Enter during a pending
search waits for that query's results. Rows and shortcut chips are retained and
updated in place. No filesystem enumeration happens in the prompt.

Notice motion uses the executable source tokens: 224 ms entry, 128 ms exit,
640 ms hover grace and 3,840 ms default hold; the slide distance is 350 px.
The older source docstring's timing values do not describe the current tokens.
The shared motion engine supplies reduced-motion handling and removes its clock
when movement finishes. A positive notice hold uses a single expiry source;
sticky notices have no timers. Notice frame updates allocate no Rust collections.

Prompt prewarming is opt-in: after the host window is realised, call
`host.cache_prompt(&palette)` before the first open. The host keeps one hidden,
realised prompt tree and reuses it on later opens. Caching another prompt releases
the previous hidden tree. Query reset, provider reload and focus restoration still
happen on every open/close. Ordinary `show_prompt` remains supported without
prewarming. The window owner must call this hook to get the first-open benefit.

The ignored `benchmark_prompt` records first draw once, separately from mapping,
and prints three reopen samples. Set `IMPRO_CHROME_BENCH_CACHE=1` to include cache
preparation before the timed open; the preparation cost is printed separately.
