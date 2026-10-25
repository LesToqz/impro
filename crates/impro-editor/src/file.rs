//! Blocking file helpers. Panel operations call these on workers.
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};
pub const MAX_FILE_SIZE: usize = 5 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct Document {
    pub text: String,
    pub ending: &'static str,
    pub mtime: SystemTime,
    pub resolved_path: PathBuf,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub fn detect_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "CRLF"
    } else {
        "LF"
    }
}
pub fn read_file(path: impl AsRef<Path>) -> io::Result<Document> {
    let path = path.as_ref();
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NONBLOCK);
    let stream = options.open(path)?;
    let meta = stream.metadata()?;
    if !meta.is_file() {
        return Err(invalid("Cannot open a non-regular file"));
    }
    if meta.len() > MAX_FILE_SIZE as u64 {
        return Err(invalid("Cannot open files larger than 5 MB"));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    stream
        .take(MAX_FILE_SIZE as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FILE_SIZE {
        return Err(invalid("Cannot open files larger than 5 MB"));
    }
    if bytes.contains(&0) {
        return Err(invalid("cannot open binary or non-UTF-8 file"));
    }
    let text =
        String::from_utf8(bytes).map_err(|_| invalid("cannot open binary or non-UTF-8 file"))?;
    Ok(Document {
        ending: detect_line_ending(&text),
        text: text.replace("\r\n", "\n"),
        mtime: meta.modified()?,
        resolved_path: path.canonicalize().unwrap_or_else(|_| absolute(path)),
    })
}
/// Follow symlinks, preserve mode, sync and replace in the destination directory.
pub fn atomic_write(path: impl AsRef<Path>, text: &str, ending: &str) -> io::Result<SystemTime> {
    let path = path.as_ref();
    let target = resolve_target(path)?;
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    #[cfg(unix)]
    let mode = match target.metadata() {
        Ok(m) => m.permissions().mode(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => 0o644,
        Err(e) => return Err(e),
    };
    #[cfg(windows)]
    let permissions = match target.metadata() {
        Ok(meta) => Some(meta.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    // MoveFileExW cannot replace a read-only destination. Leave it untouched,
    // rather than clearing the user's read-only attribute to force a save.
    #[cfg(windows)]
    if permissions.as_ref().is_some_and(fs::Permissions::readonly) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "File is read-only",
        ));
    }
    let mut temp = tempfile::Builder::new()
        .prefix(".impro-")
        .tempfile_in(parent)?;
    if ending == "CRLF" {
        temp.write_all(text.replace('\n', "\r\n").as_bytes())?;
    } else {
        temp.write_all(text.as_bytes())?;
    }
    #[cfg(unix)]
    temp.as_file()
        .set_permissions(fs::Permissions::from_mode(mode))?;
    temp.as_file().sync_all()?;
    let _saved = temp.persist(&target).map_err(|e| e.error)?;
    // tempfile clears FILE_ATTRIBUTE_TEMPORARY while persisting on Windows;
    // apply the preserved permissions afterwards so they are not overwritten.
    #[cfg(windows)]
    if let Some(permissions) = permissions {
        _saved.set_permissions(permissions)?;
    }
    // Opening directories as files for fsync is a Unix operation. persist uses
    // MoveFileExW(REPLACE_EXISTING) on Windows and replaces an existing file.
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    target.metadata()?.modified()
}
fn resolve_target(path: &Path) -> io::Result<PathBuf> {
    // canonicalize alone cannot resolve a dangling symlink to a new file.
    let mut target = absolute(path);
    for _ in 0..40 {
        match fs::read_link(&target) {
            Ok(link) => {
                target = if link.is_absolute() {
                    link
                } else {
                    target.parent().unwrap_or(Path::new("/")).join(link)
                }
            }
            Err(e)
                if e.kind() == io::ErrorKind::InvalidInput
                    || e.kind() == io::ErrorKind::NotFound
                    // Windows read_link reports ERROR_NOT_A_REPARSE_POINT for
                    // ordinary files, rather than Unix's InvalidInput.
                    || (cfg!(windows) && e.raw_os_error() == Some(4390)) =>
            {
                let parent = target.parent().unwrap_or(Path::new("/"));
                return Ok(parent
                    .canonicalize()?
                    .join(target.file_name().unwrap_or_default()));
            }
            Err(e) => return Err(e),
        }
    }
    Err(invalid("Too many symbolic links"))
}
pub(crate) fn absolute(path: &Path) -> PathBuf {
    let source = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| "/".into())
            .join(path)
    };
    let mut result = PathBuf::new();
    for part in source.components() {
        match part {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            _ => result.push(part),
        }
    }
    result
}
/// Lossless local path representation for GObject string signals, including non-UTF-8 names.
pub(crate) fn path_signal(path: &Path) -> String {
    glib::filename_to_uri(path, None)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}
pub fn leading_whitespace(line: &str) -> &str {
    &line[..line
        .bytes()
        .take_while(|c| *c == b' ' || *c == b'\t')
        .count()]
}
pub fn unindent(line: &str, width: usize) -> &str {
    if let Some(rest) = line.strip_prefix('\t') {
        rest
    } else {
        line.get(line.bytes().take_while(|c| *c == b' ').count().min(width)..)
            .unwrap_or(line)
    }
}
pub fn toggle_comments(text: &str) -> String {
    let populated: Vec<_> = text.split('\n').filter(|s| !s.trim().is_empty()).collect();
    let uncomment =
        !populated.is_empty() && populated.iter().all(|s| s.trim_start().starts_with('#'));
    text.split('\n')
        .map(|line| {
            let prefix = leading_whitespace(line);
            let body = &line[prefix.len()..];
            if body.is_empty() {
                line.to_string()
            } else if uncomment {
                format!(
                    "{prefix}{}",
                    body.strip_prefix("# ")
                        .or_else(|| body.strip_prefix('#'))
                        .unwrap_or(body)
                )
            } else {
                format!("{prefix}# {body}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod line_ending_tests {
    #[cfg(windows)]
    // The lint targets Unix world-writable files; on Windows this only clears the read-only attribute.
    #[allow(clippy::permissions_set_readonly_false)]
    #[test]
    fn read_only_save_preserves_original_attribute_and_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("read-only.txt");
        std::fs::write(&path, "original").unwrap();
        let mut permissions = path.metadata().unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions.clone()).unwrap();
        let result = super::atomic_write(&path, "replacement", "LF");
        let still_read_only = path.metadata().unwrap().permissions().readonly();
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        assert_eq!(
            result.unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert!(still_read_only);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
    }
    #[test]
    fn crlf_is_normalized_for_editing_and_restored_on_atomic_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lines.txt");
        std::fs::write(&path, b"first\r\nsecond\r\n").unwrap();
        let doc = super::read_file(&path).unwrap();
        assert_eq!(doc.text, "first\nsecond\n");
        assert_eq!(doc.ending, "CRLF");
        super::atomic_write(&path, &doc.text, doc.ending).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first\r\nsecond\r\n");
    }
}
