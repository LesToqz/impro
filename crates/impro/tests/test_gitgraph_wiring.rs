use std::sync::Mutex;
static LOCK: Mutex<()> = Mutex::new(());
fn run(mode: &str) {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let exe = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/window_qa");
    let output = std::process::Command::new(exe)
        .arg(mode)
        .env("G_DEBUG", "fatal-criticals")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("scenario-ok"));
}
#[test]
fn test_settings_orientation_icons_options_and_startup_round_trip() {
    run("settings");
}
#[test]
fn test_disabling_gitgraph_preserves_open_commit_tabs() {
    run("feature");
}
#[test]
fn test_menu_palette_accelerator_and_bottom_reuse() {
    run("git");
}
#[test]
fn test_full_bottom_falls_back_right_and_full_docks_notice_once() {
    run("capacity");
}
#[test]
fn test_dot_commit_last_title_files_parent_reopen_and_close_signal() {
    run("git");
}
#[test]
fn test_selection_sync_files_commits_later_graph_and_graph_close() {
    run("git");
}
#[test]
fn test_commit_file_links_existing_and_deleted() {
    run("links");
}
#[test]
fn test_theme_accent_fonts_motion_and_size() {
    run("theme");
}
#[test]
fn test_monitor_refresh_commit_survives_folder_change_closes_and_reroots() {
    run("refresh");
}
#[test]
fn test_short_hash_worker_validation_dedup_and_stale_folder() {
    run("hash");
}
#[test]
fn test_empty_states_and_error_notice_once() {
    run("errors");
}
#[test]
fn test_chooser_both_docks_three_kinds_and_empty_dock_rules() {
    run("chooser");
}
#[test]
fn test_all_accelerators_fire_actions() {
    run("accelerators");
}
#[test]
#[ignore = "requires an X11 window manager for native maximize/restore"]
fn test_window_glyph_pixels_in_dark_and_light_including_restore() {
    run("controls");
}
