//! Per-user directories with platform conventions, kept free of GTK/GLib.
//!
//! Unix follows XDG (`XDG_CONFIG_HOME`, else `~/.config`); Windows uses `%APPDATA%`
//! (the roaming profile), falling back to `%USERPROFILE%\AppData\Roaming`. The lookups
//! take an environment getter so both rules are unit-testable on any host.
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn non_empty(get: &impl Fn(&str) -> Option<OsString>, key: &str) -> Option<OsString> {
    get(key).filter(|v| !v.is_empty())
}

/// Home directory exactly as the Unix code has always read it: `$HOME`, even if empty.
#[cfg_attr(windows, allow(dead_code))]
fn unix_home_dir(get: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    get("HOME").map(PathBuf::from)
}

/// `%USERPROFILE%`, then `$HOME` (set by MSYS2/Git-bash style shells).
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_home_dir(get: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    non_empty(get, "USERPROFILE")
        .or_else(|| non_empty(get, "HOME"))
        .map(PathBuf::from)
}

#[cfg_attr(windows, allow(dead_code))]
fn unix_config_root(get: &impl Fn(&str) -> Option<OsString>) -> PathBuf {
    non_empty(get, "XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            unix_home_dir(get)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
        })
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_config_root(get: &impl Fn(&str) -> Option<OsString>) -> PathBuf {
    non_empty(get, "APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            windows_home_dir(get)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("AppData")
                .join("Roaming")
        })
}

fn env(key: &str) -> Option<OsString> {
    std::env::var_os(key)
}

/// The user's home directory (`$HOME`; on Windows `%USERPROFILE%`, then `$HOME`).
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return windows_home_dir(&env);
    #[cfg(not(windows))]
    return unix_home_dir(&env);
}

/// The directory that holds per-application config directories.
pub fn config_root() -> PathBuf {
    #[cfg(windows)]
    return windows_config_root(&env);
    #[cfg(not(windows))]
    return unix_config_root(&env);
}

/// Expands a leading `~` / `~/…` (and `~\…` on Windows) with [`home_dir`]; any other
/// input, or a missing home directory, is returned unchanged.
pub fn expand_tilde(path: &Path) -> PathBuf {
    expand_tilde_with(home_dir(), path)
}

fn expand_tilde_with(home: Option<PathBuf>, path: &Path) -> PathBuf {
    let Some(home) = home else {
        return path.to_path_buf();
    };
    match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

/// Default family list (Pango syntax, comma separated) for terminals and code. On Windows Pango
/// resolves the generic `Monospace` to Courier New; Windows' own console and editor fonts read
/// better and match other Windows software (Cascadia Mono ships with recent Windows and Windows
/// Terminal, Consolas with every supported Windows).
pub const MONOSPACE_FAMILY: &str = if cfg!(windows) {
    "Cascadia Mono,Consolas,Courier New"
} else {
    "Monospace"
};

/// Default family for running text outside the chrome (notes). On Windows the generic
/// `sans-serif` resolves to Arial; the system's interface font is Segoe UI.
pub const SANS_FAMILY: &str = if cfg!(windows) {
    "Segoe UI"
} else {
    "sans-serif"
};

/// `std::fs::canonicalize` without the Windows verbatim prefix where the plain path names the
/// same file: what the application shows, stores and hands to other programs.
///
/// On Windows `canonicalize` returns `\\?\C:\...` (or `\\?\UNC\server\share\...`). Many programs
/// mishandle that spelling: Windows PowerShell started in such a directory shows its prompt as
/// `Microsoft.PowerShell.Core\FileSystem::\\?\C:\...`, `cmd.exe` refuses UNC-style working
/// directories, and the path reads oddly in titles and dialogs. On other platforms this is
/// exactly `std::fs::canonicalize`.
pub fn canonicalize(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(|p| simplified(&p))
}

/// [`canonicalize`]'s prefix removal on its own, for paths that are already absolute.
pub fn simplified(path: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(plain) = path.to_str().and_then(simplify_verbatim) {
        return PathBuf::from(plain);
    }
    path.to_path_buf()
}

/// The plain spelling of a verbatim Windows path when it means the same thing, else `None`.
///
/// Only drive (`\\?\C:\...`) and UNC (`\\?\UNC\server\share\...`) paths qualify, and only when
/// the plain form stays under `MAX_PATH` and no component is one Win32 path parsing would alter:
/// a reserved device name (`CON`, `NUL`, `COM1`, ...), a name ending in a dot or a space, `.` or
/// `..`, or one with characters a plain path cannot carry. Pure, so every host can test it.
#[cfg_attr(not(windows), allow(dead_code))]
fn simplify_verbatim(text: &str) -> Option<String> {
    const MAX_PATH: usize = 260;
    let (plain, components) = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        let plain = format!(r"\\{rest}");
        let mut parts = rest.split('\\');
        let (server, share) = (parts.next()?, parts.next()?);
        if server.is_empty() || share.is_empty() {
            return None;
        }
        (plain, rest.to_owned())
    } else {
        let rest = text.strip_prefix(r"\\?\")?;
        let bytes = rest.as_bytes();
        let drive = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\';
        if !drive {
            return None;
        }
        (rest.to_owned(), rest[3..].to_owned())
    };
    if plain.len() >= MAX_PATH {
        return None;
    }
    let safe = components
        .split('\\')
        .filter(|part| !part.is_empty())
        .all(|part| {
            let stem = part.split('.').next().unwrap_or(part).trim_end();
            let upper = stem.to_ascii_uppercase();
            let device = matches!(
                upper.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) || ((upper.starts_with("COM") || upper.starts_with("LPT"))
                && upper.len() == 4
                && matches!(upper.as_bytes()[3], b'1'..=b'9'));
            !device
                && part != "."
                && part != ".."
                && !part.ends_with('.')
                && !part.ends_with(' ')
                && !part
                    .chars()
                    .any(|c| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '|' | '?' | '*'))
        });
    safe.then_some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn getter(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn unix_prefers_xdg_then_home_then_dot() {
        let env = getter(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")]);
        assert_eq!(unix_config_root(&env), PathBuf::from("/x"));
        let env = getter(&[("XDG_CONFIG_HOME", ""), ("HOME", "/h")]);
        assert_eq!(unix_config_root(&env), PathBuf::from("/h/.config"));
        assert_eq!(unix_config_root(&getter(&[])), PathBuf::from("./.config"));
    }

    #[test]
    fn windows_prefers_appdata_then_userprofile() {
        let env = getter(&[
            ("APPDATA", "C:/Users/a/AppData/Roaming"),
            ("USERPROFILE", "C:/u"),
        ]);
        assert_eq!(
            windows_config_root(&env),
            PathBuf::from("C:/Users/a/AppData/Roaming")
        );
        let env = getter(&[("APPDATA", ""), ("USERPROFILE", "C:/u")]);
        assert_eq!(
            windows_config_root(&env),
            PathBuf::from("C:/u").join("AppData").join("Roaming")
        );
        assert_eq!(
            windows_config_root(&getter(&[])),
            PathBuf::from(".").join("AppData").join("Roaming")
        );
    }

    #[test]
    fn windows_home_falls_back_to_home() {
        let env = getter(&[("USERPROFILE", ""), ("HOME", "C:/msys/home/a")]);
        assert_eq!(
            windows_home_dir(&env),
            Some(PathBuf::from("C:/msys/home/a"))
        );
        assert_eq!(windows_home_dir(&getter(&[])), None);
    }

    #[test]
    fn unix_home_is_taken_verbatim() {
        assert_eq!(
            unix_home_dir(&getter(&[("HOME", "")])),
            Some(PathBuf::new())
        );
        assert_eq!(unix_home_dir(&getter(&[])), None);
    }

    #[test]
    fn tilde_expands_only_as_a_whole_leading_component() {
        let home = Some(PathBuf::from("/h"));
        let expand = |p: &str| expand_tilde_with(home.clone(), Path::new(p));
        assert_eq!(expand("~"), PathBuf::from("/h"));
        assert_eq!(expand("~/a/b"), PathBuf::from("/h/a/b"));
        assert_eq!(expand("~other/a"), PathBuf::from("~other/a"));
        assert_eq!(expand("/a/~"), PathBuf::from("/a/~"));
        assert_eq!(
            expand_tilde_with(None, Path::new("~/a")),
            PathBuf::from("~/a")
        );
    }

    #[test]
    fn verbatim_drive_and_unc_paths_lose_the_prefix() {
        let plain = |p: &str| simplify_verbatim(p);
        assert_eq!(
            plain(r"\\?\C:\Users\me\Projects\impro").as_deref(),
            Some(r"C:\Users\me\Projects\impro")
        );
        assert_eq!(plain(r"\\?\C:\").as_deref(), Some(r"C:\"));
        assert_eq!(plain(r"\\?\d:\a b\c.txt").as_deref(), Some(r"d:\a b\c.txt"));
        assert_eq!(
            plain(r"\\?\UNC\server\share\dir").as_deref(),
            Some(r"\\server\share\dir")
        );
        assert_eq!(
            plain(r"\\?\UNC\server\share").as_deref(),
            Some(r"\\server\share")
        );
    }

    #[test]
    fn paths_that_win32_would_reinterpret_stay_verbatim() {
        for path in [
            r"\\?\C:\a\CON",
            r"\\?\C:\a\nul.txt",
            r"\\?\C:\a\com1",
            r"\\?\C:\a\LPT9.log",
            r"\\?\C:\a\trailing.",
            r"\\?\C:\a\trailing ",
            r"\\?\C:\a\..\b",
            r"\\?\C:\a\.\b",
            r"\\?\C:\a\b?c",
            r"\\?\Volume{0b1c}\dir",
            r"\\?\GLOBALROOT\Device\x",
            r"\\?\UNC\server",
            r"\\?\UNC\\share",
            r"C:\already\plain",
            r"\\server\share",
            "",
        ] {
            assert_eq!(simplify_verbatim(path), None, "{path}");
        }
        assert!(simplify_verbatim(r"\\?\C:\a\com10").is_some());
        assert!(simplify_verbatim(r"\\?\C:\a\console").is_some());
        assert!(simplify_verbatim(r"\\?\C:\a\.git").is_some());
    }

    #[test]
    fn long_paths_keep_the_prefix() {
        // MAX_PATH (260) counts the terminating NUL, so a plain path holds 259 characters.
        let long = format!(r"\\?\C:\{}", "a".repeat(257));
        assert_eq!(simplify_verbatim(&long), None);
        let fits = format!(r"\\?\C:\{}", "a".repeat(256));
        assert_eq!(simplify_verbatim(&fits).map(|p| p.len()), Some(259));
    }

    #[test]
    fn canonicalize_matches_std_apart_from_the_prefix() {
        let dir = std::env::temp_dir();
        let ours = canonicalize(&dir).unwrap();
        let theirs = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(ours, simplified(&theirs));
        #[cfg(windows)]
        assert!(!ours.to_string_lossy().starts_with(r"\\?\"));
        #[cfg(not(windows))]
        assert_eq!(ours, theirs);
    }
}
