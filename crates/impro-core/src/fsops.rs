//! Explorer filesystem actions, race-free rename, root guards and freedesktop trash.
use std::os::unix::{
    ffi::OsStrExt,
    fs::{MetadataExt, PermissionsExt},
};
use std::{
    ffi::CString,
    fs,
    io::{self, Write},
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
    if target == Path::new("/")
        || project.as_ref() == Some(&target)
        || !link
            && (resolved == Path::new("/")
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
/// Explicit destination for tests; follows the freedesktop .trashinfo format.
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
pub(crate) fn local_date(format: &str) -> String {
    let Ok(format) = CString::new(format) else {
        return String::new();
    };
    let mut buffer = [0u8; 64];
    // SAFETY: time/localtime_r/strftime receive valid writable buffers of the stated sizes.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::new();
        }
        let length = libc::strftime(
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            format.as_ptr(),
            &tm,
        );
        String::from_utf8_lossy(&buffer[..length]).into_owned()
    }
}
