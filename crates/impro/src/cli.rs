//! Command-line entry point.
use crate::{
    app,
    launch::{self, Targets},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
const USAGE: &str = "usage: impro [-h] [--version] [path ...]";
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run(Targets),
    Help,
    Version,
}
fn parse_args(args: impl IntoIterator<Item = OsString>, cwd: &Path) -> Result<Command, String> {
    parse_args_with(args, cwd, impro_core::platform::expand_tilde)
}
fn parse_args_with(
    args: impl IntoIterator<Item = OsString>,
    cwd: &Path,
    expand: impl Fn(&Path) -> PathBuf,
) -> Result<Command, String> {
    let mut paths = Vec::new();
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
        paths.push(arg);
    }
    if !extra.is_empty() {
        return Err(format!("unrecognized arguments: {}", extra.join(" ")));
    }
    launch::classify(&paths, cwd, expand).map(Command::Run)
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
            println!("{USAGE}\n\nimpro desktop IDE\n\npositional arguments:\n  path         a folder to open as the project, or files to open in the editor\n\noptions:\n  -h, --help   show this help message and exit\n  --version    show the application version and exit");
            glib::ExitCode::SUCCESS
        }
        Err(error) => {
            attach_console();
            eprintln!("{USAGE}\nimpro: error: {error}");
            glib::ExitCode::from(2)
        }
        Ok(Command::Run(targets)) => app::run(launch::prepare(targets, &cwd), started),
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

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    fn dir(path: &Path) -> Targets {
        Targets {
            dir: Some(path.canonicalize().unwrap()),
            files: vec![],
        }
    }
    #[test]
    fn usage_names_paths() {
        assert_eq!(USAGE, "usage: impro [-h] [--version] [path ...]");
    }
    #[test]
    fn injected_expansion_sees_every_path_argument_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        for raw in [
            r"C:\Users\x\proj",
            "C:/Users/x/proj",
            r"\\server\share\proj",
            r"~\proj",
            "~/proj",
        ] {
            let result = parse_args_with(args(&[raw]), tmp.path(), |seen| {
                assert_eq!(seen.as_os_str(), std::ffi::OsStr::new(raw));
                tmp.path().to_path_buf()
            })
            .unwrap();
            assert_eq!(result, Command::Run(dir(tmp.path())));
        }
    }
    #[cfg(windows)]
    #[test]
    fn windows_absolute_roots_and_tilde_components() {
        for raw in [
            r"C:\Users\x\proj",
            "C:/Users/x/proj",
            r"\\server\share\proj",
        ] {
            assert!(Path::new(raw).is_absolute());
            assert_eq!(
                impro_core::platform::expand_tilde(Path::new(raw)),
                Path::new(raw)
            );
        }
        let tmp = tempfile::tempdir().unwrap();
        let forward = tmp.path().to_string_lossy().replace('\\', "/");
        assert!(matches!(
            parse_args(args(&[&forward]), tmp.path()),
            Ok(Command::Run(_))
        ));
        if let Some(home) = impro_core::platform::home_dir() {
            assert_eq!(
                impro_core::platform::expand_tilde(Path::new(r"~\proj")),
                home.join("proj")
            );
        }
    }
    #[test]
    fn test_optional_path_and_default() {
        let cwd = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(
            parse_args(args(&[]), &cwd).unwrap(),
            Command::Run(Targets::default())
        );
        assert_eq!(
            parse_args(args(&["."]), &cwd).unwrap(),
            Command::Run(dir(&cwd))
        );
    }
    #[test]
    fn directories_and_files_may_be_mixed() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        std::fs::create_dir(base.join("proj")).unwrap();
        std::fs::create_dir(base.join("other")).unwrap();
        std::fs::write(base.join("a.txt"), "a").unwrap();
        std::fs::write(base.join("proj/b.txt"), "b").unwrap();
        let got = parse_args(args(&["a.txt", "proj", "proj/b.txt", "other"]), &base).unwrap();
        assert_eq!(
            got,
            Command::Run(Targets {
                dir: Some(base.join("proj")),
                files: vec![base.join("a.txt"), base.join("proj/b.txt")],
            })
        );
    }
    #[test]
    fn double_dash_makes_dashed_names_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        std::fs::write(base.join("-odd.txt"), "x").unwrap();
        assert_eq!(
            parse_args(args(&["--", "-odd.txt"]), &base).unwrap(),
            Command::Run(Targets {
                dir: None,
                files: vec![base.join("-odd.txt")],
            })
        );
        assert_eq!(
            parse_args(args(&["-odd.txt"]), &base).unwrap_err(),
            "unrecognized arguments: -odd.txt"
        );
        // After `--` even `--version` is a path.
        assert!(parse_args(args(&["--", "--version"]), &base)
            .unwrap_err()
            .starts_with("no such file or directory"));
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
        assert_eq!(
            parse_args(args(&["--help"]), Path::new("/")).unwrap(),
            Command::Help
        );
        // Help wins even beside a path that does not exist.
        assert_eq!(
            parse_args(args(&["/does-not-exist", "--help"]), Path::new("/")).unwrap(),
            Command::Help
        );
    }
    #[test]
    fn test_same_argument_errors() {
        assert_eq!(
            parse_args(args(&["/does-not-exist"]), Path::new("/")).unwrap_err(),
            "no such file or directory: /does-not-exist"
        );
        assert_eq!(
            parse_args(args(&["--bad"]), Path::new("/")).unwrap_err(),
            "unrecognized arguments: --bad"
        );
        assert_eq!(
            parse_args(args(&["/tmp", "extra-missing"]), Path::new("/")).unwrap_err(),
            "no such file or directory: extra-missing"
        );
        assert_eq!(
            parse_args(args(&["--bad", "--worse"]), Path::new("/")).unwrap_err(),
            "unrecognized arguments: --bad --worse"
        );
    }
}
