//! Blocking file helpers. Panel operations call these on workers.
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{OpenOptionsExt, PermissionsExt},
    },
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
    let stream = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
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
    let mode = match target.metadata() {
        Ok(m) => m.permissions().mode(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => 0o644,
        Err(e) => return Err(e),
    };
    let mut temp = tempfile::Builder::new()
        .prefix(".impro-")
        .tempfile_in(parent)?;
    if ending == "CRLF" {
        temp.write_all(text.replace('\n', "\r\n").as_bytes())?;
    } else {
        temp.write_all(text.as_bytes())?;
    }
    temp.as_file()
        .set_permissions(fs::Permissions::from_mode(mode))?;
    temp.as_file().sync_all()?;
    temp.persist(&target).map_err(|e| e.error)?;
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
                    || e.kind() == io::ErrorKind::NotFound =>
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
        .unwrap_or_else(|_| String::from_utf8_lossy(path.as_os_str().as_bytes()).into_owned())
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
