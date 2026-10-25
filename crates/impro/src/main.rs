//! Desktop entry point.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
fn main() -> glib::ExitCode {
    impro::cli::run()
}
