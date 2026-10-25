//! Explorer filesystem actions, race-free rename, root guards and trash (freedesktop on
//! Unix, the Recycle Bin on Windows).
#[cfg(unix)]
use std::os::unix::{
    ffi::OsStrExt,
    fs::{MetadataExt, PermissionsExt},
};
#[cfg(unix)]
use std::{ffi::CString, io::Write};
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FsOpError {
    Invalid(String),
    Io(String),
    TrashUnavailable(String),
}
impl std::fmt::Display for FsOpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) | Self::Io(s) | Self::TrashUnavailable(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for FsOpError {}
pub fn validate_name(name: &str) -> Result<(), FsOpError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
        return Err(FsOpError::Invalid(
            "Invalid name: enter a name without '/' or NUL; '.' and '..' are not allowed.".into(),
        ));
    }
    Ok(())
}
pub fn absolute(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            part => normalized.push(part.as_os_str()),
        }
    }
    normalized
}
fn error(e: io::Error, path: &Path) -> FsOpError {
    FsOpError::Io(match e.kind() {
        io::ErrorKind::AlreadyExists => format!("Already exists: {}", path.display()),
        io::ErrorKind::PermissionDenied => format!("Permission denied: {}", path.display()),
        io::ErrorKind::NotFound => format!("Not found: {}", path.display()),
        _ => format!("Cannot modify {}: {e}", path.display()),
    })
}
pub fn create_file(parent: impl AsRef<Path>, name: &str) -> Result<PathBuf, FsOpError> {
    validate_name(name)?;
    let path = absolute(parent.as_ref().join(name));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| error(e, &path))?;
    Ok(path)
}
pub fn create_folder(parent: impl AsRef<Path>, name: &str) -> Result<PathBuf, FsOpError> {
    validate_name(name)?;
    let path = absolute(parent.as_ref().join(name));
    fs::create_dir(&path).map_err(|e| error(e, &path))?;
    Ok(path)
}
/// Linux RENAME_NOREPLACE atomically refuses files, directories and dangling links.
#[cfg(unix)]
pub fn rename_no_replace(source: &Path, target: &Path) -> io::Result<()> {
    let source = CString::new(source.as_os_str().as_bytes())?;
    let target = CString::new(target.as_os_str().as_bytes())?;
    // SAFETY: NUL-terminated paths live for the syscall; no borrowed FD escapes.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
/// `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` fails with `ERROR_ALREADY_EXISTS` /
/// `ERROR_FILE_EXISTS` (mapped to `ErrorKind::AlreadyExists`) when the target exists, and
/// without `MOVEFILE_COPY_ALLOWED` it never degrades to copy+delete across volumes.
#[cfg(windows)]
pub fn rename_no_replace(source: &Path, target: &Path) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
    let source = wide(source)?;
    let target = wide(target)?;
    // SAFETY: both buffers are NUL-terminated UTF-16 and outlive the call.
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 0) } != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
/// NUL-terminated UTF-16 for Win32 calls; interior NULs are rejected like `CString` does.
#[cfg(windows)]
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let mut buffer: Vec<u16> = path.as_os_str().encode_wide().collect();
    if buffer.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains a NUL character",
        ));
    }
    buffer.push(0);
    Ok(buffer)
}
pub fn rename(path: impl AsRef<Path>, new_name: &str) -> Result<PathBuf, FsOpError> {
    validate_name(new_name)?;
    let source = absolute(path);
    let target = source.with_file_name(new_name);
    rename_no_replace(&source, &target).map_err(|e| error(e, &target))?;
    Ok(target)
}
pub fn removal_target(
    path: impl AsRef<Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf, FsOpError> {
    let target = absolute(path);
    let link = fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink());
    let resolved = fs::canonicalize(&target).unwrap_or_else(|_| target.clone());
    let project = project_root.map(absolute);
    if is_fs_root(&target)
        || project.as_ref() == Some(&target)
        || !link
            && (is_fs_root(&resolved)
                || project
                    .as_ref()
                    .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
                    .as_ref()
                    == Some(&resolved))
    {
        return Err(FsOpError::Invalid(format!(
            "Cannot delete a filesystem or project root: {}",
            target.display()
        )));
    }
    Ok(target)
}
/// `/` on Unix, a drive or UNC root on Windows: an absolute path with no parent.
fn is_fs_root(path: &Path) -> bool {
    path.parent().is_none()
}
pub fn delete(path: impl AsRef<Path>, project_root: Option<&Path>) -> Result<PathBuf, FsOpError> {
    let target = removal_target(path, project_root)?;
    let metadata = fs::symlink_metadata(&target).map_err(|e| error(e, &target))?;
    if metadata.is_dir() {
        fs::remove_dir_all(&target)
    } else {
        fs::remove_file(&target)
    }
    .map_err(|e| error(e, &target))?;
    Ok(target)
}
/// Home trash on the same filesystem; unsupported mounts require explicit consent elsewhere.
#[cfg(unix)]
pub fn trash(path: impl AsRef<Path>, project_root: Option<&Path>) -> Result<PathBuf, FsOpError> {
    let target = removal_target(path, project_root)?;
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or_else(|| {
            FsOpError::TrashUnavailable(format!("Trash is unavailable for: {}", target.display()))
        })?;
    trash_into(&target, &data.join("Trash"))?;
    Ok(target)
}
/// Moves to the Recycle Bin. `FOF_WANTNUKEWARNING` makes the shell ask before it would
/// delete permanently (item too large, volume without a bin) instead of doing it silently;
/// a declined or failed operation is `TrashUnavailable`, so nothing is ever deleted unasked.
#[cfg(windows)]
pub fn trash(path: impl AsRef<Path>, project_root: Option<&Path>) -> Result<PathBuf, FsOpError> {
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_WANTNUKEWARNING,
        FO_DELETE, SHFILEOPSTRUCTW,
    };
    let target = removal_target(path, project_root)?;
    let unavailable =
        || FsOpError::TrashUnavailable(format!("Trash is unavailable for: {}", target.display()));
    // pFrom is a list of NUL-separated paths ended by an extra NUL; `wide` gave one NUL.
    let mut from = wide(&target).map_err(|_| unavailable())?;
    from.push(0);
    let mut operation: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    operation.wFunc = FO_DELETE;
    operation.pFrom = from.as_ptr();
    operation.fFlags =
        (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_WANTNUKEWARNING) as _;
    // SAFETY: `operation` is fully initialised and `from` outlives the synchronous call.
    let code = unsafe { SHFileOperationW(&mut operation) };
    if code != 0 || operation.fAnyOperationsAborted != 0 {
        return Err(unavailable());
    }
    Ok(target)
}
/// Explicit destination for tests; follows the freedesktop .trashinfo format.
#[cfg(unix)]
pub fn trash_into(target: &Path, trash_dir: &Path) -> Result<PathBuf, FsOpError> {
    let target = removal_target(target, None)?;
    for dir in [
        trash_dir.to_path_buf(),
        trash_dir.join("files"),
        trash_dir.join("info"),
    ] {
        fs::create_dir_all(&dir).map_err(|e| error(e, &dir))?;
        let metadata = fs::symlink_metadata(&dir).map_err(|e| error(e, &dir))?;
        // Never use attacker-controlled symlinks or another user's trash directory.
        if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(FsOpError::TrashUnavailable(format!(
                "Trash is unavailable for: {}",
                target.display()
            )));
        }
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| error(e, &dir))?;
    }
    // A symlink belongs to its parent filesystem, irrespective of its destination.
    let device = fs::metadata(target.parent().unwrap_or(Path::new("/")))
        .map_err(|e| error(e, &target))?
        .dev();
    if device
        != fs::metadata(trash_dir)
            .map_err(|e| error(e, trash_dir))?
            .dev()
    {
        return Err(FsOpError::TrashUnavailable(format!(
            "Trash is unavailable for: {}",
            target.display()
        )));
    }
    let name = target
        .file_name()
        .ok_or_else(|| FsOpError::Invalid("Invalid trash entry".into()))?;
    for number in 0.. {
        let mut entry = name.to_os_string();
        if number > 0 {
            entry.push(format!(".{number}"));
        }
        let destination = trash_dir.join("files").join(&entry);
        let mut info_name = entry;
        info_name.push(".trashinfo");
        let info = trash_dir.join("info").join(info_name);
        if fs::symlink_metadata(&destination).is_ok() {
            continue;
        }
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&info)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(error(e, &info)),
        };
        let result = (|| -> io::Result<()> {
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            writeln!(
                file,
                "[Trash Info]\nPath={}\nDeletionDate={}",
                percent_encode(target.as_os_str().as_bytes()),
                local_date("%Y-%m-%dT%H:%M:%S")
            )?;
            file.sync_all()?;
            rename_no_replace(&target, &destination)
        })();
        if let Err(e) = result {
            let _ = fs::remove_file(&info);
            if e.kind() == io::ErrorKind::AlreadyExists {
                continue;
            }
            return Err(error(e, &target));
        }
        return Ok(destination);
    }
    unreachable!("unbounded trash suffix iterator")
}
#[cfg(unix)]
fn percent_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::new();
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            result.push(b as char);
        } else {
            let _ = write!(result, "%{b:02X}");
        }
    }
    result
}
/// Wall-clock fields in the local time zone, formatted without libc `strftime` so the
/// result is identical on every platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}
impl LocalTime {
    /// Supports the numeric `strftime` subset: `%Y %m %d %H %M %S %F %T %%`; any other
    /// specifier is copied through unchanged.
    pub(crate) fn format(&self, format: &str) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let mut chars = format.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            let _ = match chars.next() {
                Some('Y') => write!(out, "{:04}", self.year),
                Some('m') => write!(out, "{:02}", self.month),
                Some('d') => write!(out, "{:02}", self.day),
                Some('H') => write!(out, "{:02}", self.hour),
                Some('M') => write!(out, "{:02}", self.minute),
                Some('S') => write!(out, "{:02}", self.second),
                Some('F') => write!(out, "{:04}-{:02}-{:02}", self.year, self.month, self.day),
                Some('T') => write!(
                    out,
                    "{:02}:{:02}:{:02}",
                    self.hour, self.minute, self.second
                ),
                Some('%') => write!(out, "%"),
                Some(other) => write!(out, "%{other}"),
                None => write!(out, "%"),
            };
        }
        out
    }
}
#[cfg(unix)]
pub(crate) fn local_now() -> Option<LocalTime> {
    // SAFETY: time/localtime_r receive valid writable buffers.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return None;
        }
        Some(LocalTime {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            minute: tm.tm_min as u32,
            second: tm.tm_sec as u32,
        })
    }
}
#[cfg(windows)]
pub(crate) fn local_now() -> Option<LocalTime> {
    use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
    let mut time = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: `time` is a valid writable SYSTEMTIME.
    unsafe { GetLocalTime(&mut time) };
    Some(LocalTime {
        year: time.wYear as i32,
        month: time.wMonth as u32,
        day: time.wDay as u32,
        hour: time.wHour as u32,
        minute: time.wMinute as u32,
        second: time.wSecond as u32,
    })
}
pub(crate) fn local_date(format: &str) -> String {
    local_now().map(|t| t.format(format)).unwrap_or_default()
}
#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: LocalTime = LocalTime {
        year: 2026,
        month: 3,
        day: 7,
        hour: 4,
        minute: 5,
        second: 9,
    };
    #[test]
    fn formats_the_numeric_subset() {
        assert_eq!(SAMPLE.format("%Y-%m-%d"), "2026-03-07");
        assert_eq!(SAMPLE.format("%Y-%m-%dT%H:%M:%S"), "2026-03-07T04:05:09");
        assert_eq!(SAMPLE.format("%F %T 100%%"), "2026-03-07 04:05:09 100%");
        assert_eq!(SAMPLE.format("%Q%"), "%Q%");
    }
    #[test]
    fn short_years_are_zero_padded_like_strftime() {
        let ancient = LocalTime {
            year: 987,
            ..SAMPLE
        };
        assert_eq!(ancient.format("%Y"), "0987");
    }
    #[cfg(unix)]
    #[test]
    fn matches_libc_strftime_byte_for_byte() {
        for format in ["%Y-%m-%d", "%Y-%m-%dT%H:%M:%S", "%F %T %% %d"] {
            let mut tm: libc::tm = unsafe { std::mem::zeroed() };
            tm.tm_year = SAMPLE.year - 1900;
            tm.tm_mon = SAMPLE.month as i32 - 1;
            tm.tm_mday = SAMPLE.day as i32;
            tm.tm_hour = SAMPLE.hour as i32;
            tm.tm_min = SAMPLE.minute as i32;
            tm.tm_sec = SAMPLE.second as i32;
            let c_format = CString::new(format).unwrap();
            let mut buffer = [0u8; 64];
            // SAFETY: valid NUL-terminated format, writable buffer of the stated size.
            let length = unsafe {
                libc::strftime(
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    c_format.as_ptr(),
                    &tm,
                )
            };
            assert_eq!(
                SAMPLE.format(format),
                String::from_utf8_lossy(&buffer[..length])
            );
        }
    }
    #[test]
    fn local_date_has_the_requested_shape() {
        let date = local_date("%Y-%m-%d");
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
    }
}
