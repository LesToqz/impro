//! Command-line entry point.
use crate::app;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
const USAGE: &str = "usage: impro [-h] [--version] [folder]";
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run(PathBuf),
    Help,
    Version,
}
fn parse_args(args: impl IntoIterator<Item = OsString>, cwd: &Path) -> Result<Command, String> {
    let mut folder = None;
    let mut positional = false;
    let mut extra = Vec::new();
    for arg in args {
        if !positional {
            match arg.to_str() {
                Some("--") => {
                    positional = true;
                    continue;
                }
                Some("--version") => return Ok(Command::Version),
                Some("-h" | "--help") => return Ok(Command::Help),
                Some(s) if s.starts_with('-') => {
                    extra.push(arg.to_string_lossy().into_owned());
                    continue;
                }
                _ => {}
            }
        }
        if folder.is_none() {
            folder = Some(arg);
        } else {
            extra.push(arg.to_string_lossy().into_owned());
        }
    }
    if !extra.is_empty() {
        return Err(format!("unrecognized arguments: {}", extra.join(" ")));
    }
    let raw = folder
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.to_path_buf());
    let expanded = impro_core::platform::expand_tilde(&raw);
    let full = if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    };
    if !full.is_dir() {
        return Err(format!("not a directory: {}", raw.display()));
    }
    full.canonicalize()
        .map(Command::Run)
        .map_err(|e| format!("cannot open directory {}: {e}", raw.display()))
}
pub fn run() -> glib::ExitCode {
    let started = std::time::Instant::now();
    std::panic::set_hook(Box::new(|info| eprintln!("impro panic: {info}")));
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            attach_console();
            eprintln!("impro: error: cannot read current directory: {e}");
            return glib::ExitCode::from(2);
        }
    };
    match parse_args(std::env::args_os().skip(1), &cwd) {
        Ok(Command::Version) => {
            attach_console();
            println!("impro {}", impro_core::identity::VERSION);
            glib::ExitCode::SUCCESS
        }
        Ok(Command::Help) => {
            attach_console();
            println!("{USAGE}\n\nimpro desktop IDE\n\npositional arguments:\n  folder\n\noptions:\n  -h, --help   show this help message and exit\n  --version    show the application version and exit");
            glib::ExitCode::SUCCESS
        }
        Err(error) => {
            attach_console();
            eprintln!("{USAGE}\nimpro: error: {error}");
            glib::ExitCode::from(2)
        }
        Ok(Command::Run(root)) => app::run(root, started),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    #[test]
    fn test_optional_folder_and_default() {
        let cwd = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(
            parse_args(args(&[]), &cwd).unwrap(),
            Command::Run(cwd.clone())
        );
        assert_eq!(parse_args(args(&["."]), &cwd).unwrap(), Command::Run(cwd));
    }
    #[test]
    fn test_version_and_help_without_display() {
        assert_eq!(
            parse_args(args(&["--version"]), Path::new("/")).unwrap(),
            Command::Version
        );
        assert_eq!(
            parse_args(args(&["-h"]), Path::new("/")).unwrap(),
            Command::Help
        );
    }
    #[test]
    fn test_same_argument_errors() {
        assert_eq!(
            parse_args(args(&["/does-not-exist"]), Path::new("/")).unwrap_err(),
            "not a directory: /does-not-exist"
        );
        assert_eq!(
            parse_args(args(&["--bad"]), Path::new("/")).unwrap_err(),
            "unrecognized arguments: --bad"
        );
        assert_eq!(
            parse_args(args(&["/tmp", "extra"]), Path::new("/")).unwrap_err(),
            "unrecognized arguments: extra"
        );
    }
}

/// Release GUI executables have no console by default. Inherited redirected
/// handles are already usable: AttachConsole can replace them, so save and restore
/// each valid handle if the other stream needs attachment. Never allocate a new
/// console when launched by Explorer.
fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            System::Console::{
                AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
                STD_OUTPUT_HANDLE,
            },
        };
        let output = GetStdHandle(STD_OUTPUT_HANDLE);
        let error = GetStdHandle(STD_ERROR_HANDLE);
        let valid = |handle: windows_sys::Win32::Foundation::HANDLE| {
            !handle.is_null() && handle != INVALID_HANDLE_VALUE
        };
        // SAFETY: only process-owned standard handles and the documented parent
        // process constant are used; no handle is closed or dereferenced here.
        if (!valid(output) || !valid(error)) && AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
            if valid(output) {
                SetStdHandle(STD_OUTPUT_HANDLE, output);
            }
            if valid(error) {
                SetStdHandle(STD_ERROR_HANDLE, error);
            }
        }
    }
}
