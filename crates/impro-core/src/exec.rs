//! Platform directories and executable discovery, without GTK dependencies.
#[path = "platform.rs"]
mod directories;
pub use directories::*;
use std::path::{Path, PathBuf};

/// Whether a file can be selected as a program. Unix uses access(X_OK), including
/// effective filesystem restrictions. Windows accepts explicit file paths, or a
/// name with an executable extension from PATHEXT (case-insensitive).
pub fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::CString::new(path.as_os_str().as_bytes()).is_ok_and(|path| {
            // SAFETY: path is a valid NUL-terminated string, read only by access.
            unsafe { libc::access(path.as_ptr(), libc::X_OK) == 0 }
        })
    }
    #[cfg(windows)]
    {
        path.components().count() > 1 || has_executable_extension(path, &extensions())
    }
}

/// Resolve a user-selected path or a bare command on PATH. Only Windows expands
/// PATHEXT; explicitly selected files need no extension there.
pub fn find_executable(text: &str) -> Option<PathBuf> {
    let direct = Path::new(text);
    if is_executable(direct) {
        return Some(direct.to_path_buf());
    }
    if direct.components().count() > 1 {
        return None;
    }
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(text);
        #[cfg(unix)]
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let extensions = extensions();
            if candidate.is_file() && has_executable_extension(&candidate, &extensions) {
                return Some(candidate);
            }
            for extension in extensions {
                let mut name = candidate.as_os_str().to_os_string();
                name.push(extension);
                let extended = PathBuf::from(name);
                if extended.is_file() {
                    return Some(extended);
                }
            }
        }
    }
    None
}

#[cfg(any(windows, test))]
fn parse_extensions(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|s| s.starts_with('.') && s.len() > 1 && !s.contains(['/', '\\']))
        .map(str::to_owned)
        .collect()
}

#[cfg(windows)]
fn extensions() -> Vec<String> {
    parse_extensions(&std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into()))
}

#[cfg(any(windows, test))]
fn has_executable_extension(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| {
            extensions
                .iter()
                .any(|candidate| candidate[1..].eq_ignore_ascii_case(ext))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_extensions_are_case_insensitive_and_configurable() {
        let extensions = parse_extensions(".COM;.EXE; .BAT ;.CMD;.PS1");
        assert!(has_executable_extension(
            Path::new("shell.ExE"),
            &extensions
        ));
        assert!(has_executable_extension(
            Path::new("script.ps1"),
            &extensions
        ));
        assert!(!has_executable_extension(
            Path::new("notes.txt"),
            &extensions
        ));
        assert!(!has_executable_extension(Path::new("shell"), &extensions));
    }
    #[test]
    fn directories_and_missing_files_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_executable(dir.path()));
        assert!(!is_executable(&dir.path().join("missing.exe")));
    }
    #[cfg(unix)]
    #[test]
    fn unix_requires_execute_permission() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shell");
        std::fs::write(&path, b"exit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!is_executable(&path));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(is_executable(&path));
    }
    #[cfg(windows)]
    #[test]
    fn explicit_existing_files_need_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shell");
        std::fs::write(&path, b"exit 0\n").unwrap();
        assert!(is_executable(&path));
    }
}
