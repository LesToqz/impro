STATUS: DONE

## What changed
- `crates/impro/src/app.rs`
  - `capture_window`: on Windows only (`#[cfg(windows)]`), when the widget is mapped and allocated (> 1x1), it renders the
    widget tree offscreen with `WidgetExt::draw` into a `cairo::ImageSurface` (ARGB32, allocated size x scale factor,
    `set_device_scale`), converts it with `gdk::pixbuf_get_from_surface` and saves the PNG with `Pixbuf::savev`.
    An unmapped or unallocated top level falls back to the old `GdkWindow::pixbuf` path. The Linux path is unchanged.
  - Extra timing lines, printed only with `IMPRO_QA_TIMING` (or `IMPRO_QA_TRACE`, as before): `app-application-new-ms`
    (before `Application::new`) and `app-application-register-ms` (after an explicit `register()`, done only when
    timing is enabled). Existing line names are kept. `gtk-startup` fires inside `register()`, so
    `gtk-startup - application` is the cost of GLib/GIO/GTK backend setup, and `application-register - gtk-startup`
    is the rest of registration. No `gtk::init` call exists in the host (it happens inside the toolkit before
    `Application::new`), so that part stays inside `application-new` .. `application`.
- `reports/windows/win-capture.md`.

## Why the old call is white on win32 (best explanation)
`GdkWindow::pixbuf` reads the window's backing store. On X11 that is a real server-side drawable. The Windows GDK
backend paints GTK3 widgets into a cairo/DIB-backed or layered surface and does not keep a readable copy for generic
`gdk_pixbuf_get_from_window`, so it returns an uninitialised (white) buffer. Offscreen `draw` bypasses the backend.

## Verified (Linux host)
- `cargo fmt --all --check`: ok
- `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh -p impro --all-targets -- -D warnings`: clean
- `scripts/windows/check.sh -p impro --all-targets`: clean
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `scripts/ci/smoke.sh <debug impro>` under xvfb: all PASS (screenshot 19729 bytes, first-paint reported)
- `cargo test -p impro --test test_app_icon`: 1 passed
- `IMPRO_QA_TIMING=1` run on Linux shows the new lines; `app-gtk-startup-ms` is still printed.

## Unverified until the runner
- That the offscreen PNG is non-blank for the real UI on Windows (nothing here runs on Windows; only cross-compile checks).
- That the idle after `display.sync()` has a final layout on Windows; the fallback only covers unmapped/unallocated windows.
- WebView2 child windows will not appear in the capture (expected).
- Where the 8 s first-run startup time goes: read it from the new timing lines.
- No Linux unit test was added: the helper is Windows-only, and the Linux capture semantics are untouched.
