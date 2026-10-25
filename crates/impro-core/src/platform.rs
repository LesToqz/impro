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
}
