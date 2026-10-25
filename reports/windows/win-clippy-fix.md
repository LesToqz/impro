STATUS: DONE

- Changed: `crates/impro-editor/src/file.rs` (added `#[allow(clippy::permissions_set_readonly_false)]` plus a one-line comment on the Windows-only test `read_only_save_preserves_original_attribute_and_contents`).
- Verified: `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check.sh --workspace --all-targets --keep-going -- -D warnings` exits 0 with no errors; `scripts/windows/check.sh --workspace --all-targets` exits 0.
- Linux: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p impro-editor --lib` (xvfb, 3 passed) are all clean.
- Open issues: none.
