//! Desktop entry point.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
fn main() -> glib::ExitCode {
    #[cfg(windows)]
    impro::win::init_process();
    impro::cli::run()
}
