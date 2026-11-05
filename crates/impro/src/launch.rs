//! Launch resolution: which folder opens and which files open with it.
//!
//! `impro [path ...]`: a directory argument is the project root (the first one wins),
//! file arguments open in the editor, and the root of a file-only launch is the first
//! file's parent. With no path at all the launch is "ambient" when the working directory
//! is not a deliberate choice (a Windows shortcut starts inside the install directory, a
//! Linux desktop launcher in `$HOME`); impro then reopens the last used folder.
//!
//! Everything that decides is a pure function over injected values, so Windows rules run
//! in unit tests on any host. The small persisted state (`state.json` next to
//! `settings.json`) lives here too; it never touches the settings schema.
use impro_core::{identity, platform};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::HashSet,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{mpsc, Mutex},
    time::Duration,
};

/// Name of the state file inside the per-user impro config directory.
pub const STATE_FILE: &str = "state.json";
/// Recent folders kept on disk; the last used folder is always the first.
pub const MAX_RECENT: usize = 10;

/// Paths named on the command line, resolved against the file system.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Targets {
    /// The first directory argument (canonical).
    pub dir: Option<PathBuf>,
    /// File arguments in order (canonical, without duplicates).
    pub files: Vec<PathBuf>,
}

impl Targets {
    /// True when the command line named no path at all.
    pub fn is_empty(&self) -> bool {
        self.dir.is_none() && self.files.is_empty()
    }
}

/// `file://` URIs (a desktop launcher using `%U`) become plain paths; anything else is untouched.
fn from_uri(raw: &OsStr) -> PathBuf {
    match raw.to_str() {
        Some(text)
            if text
                .get(..7)
                .is_some_and(|s| s.eq_ignore_ascii_case("file://")) =>
        {
            glib::filename_from_uri(text)
                .map(|(path, _host)| path)
                .unwrap_or_else(|_| PathBuf::from(raw))
        }
        _ => PathBuf::from(raw),
    }
}

/// Resolve each argument against the file system. A missing or unusable path is an error
/// naming the argument as typed. `expand` rewrites a leading `~` (injected for tests).
pub fn classify(
    args: &[OsString],
    cwd: &Path,
    expand: impl Fn(&Path) -> PathBuf,
) -> Result<Targets, String> {
    let mut targets = Targets::default();
    for raw in args {
        let shown = Path::new(raw).display();
        let expanded = expand(&from_uri(raw));
        let full = if expanded.is_absolute() {
            expanded
        } else {
            cwd.join(expanded)
        };
        let meta = fs::metadata(&full).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => format!("no such file or directory: {shown}"),
            _ => format!("cannot open {shown}: {e}"),
        })?;
        let canonical = full
            .canonicalize()
            .map_err(|e| format!("cannot open {shown}: {e}"))?;
        if meta.is_dir() {
            targets.dir.get_or_insert(canonical);
        } else if meta.is_file() {
            if !targets.files.contains(&canonical) {
                targets.files.push(canonical);
            }
        } else {
            return Err(format!("not a file or directory: {shown}"));
        }
    }
    Ok(targets)
}

/// Where the opened root came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSource {
    /// A directory argument.
    Directory,
    /// The parent of the first file argument.
    FileParent,
    /// The working directory (a deliberate choice, or nothing better was known).
    WorkingDir,
    /// The last used folder, reopened by an ambient launch.
    LastFolder,
    /// `Documents` in the user profile (Windows ambient fallback).
    Documents,
    /// The user's home directory (ambient fallback).
    Home,
}

impl RootSource {
    /// Only folders the user actually chose belong in the recent list.
    pub fn recordable(self) -> bool {
        !matches!(self, Self::Documents | Self::Home)
    }
}

/// Injected environment of a launch. `windows` selects the Windows rules (backslashes,
/// case-insensitive paths, install and system directories count as ambient).
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    pub windows: bool,
    pub cwd: &'a Path,
    pub exe_dir: Option<&'a Path>,
    pub home: Option<&'a Path>,
    pub system_dir: Option<&'a Path>,
    pub last_folder: Option<&'a Path>,
}

/// Path components compared as the platform compares them. Windows paths accept both
/// separators, ignore case and a verbatim `\\?\` prefix, whatever the host is.
fn components(path: &Path, windows: bool) -> Vec<String> {
    let text = path.to_string_lossy();
    if !windows {
        return text
            .split('/')
            .filter(|c| !c.is_empty() && *c != ".")
            .map(str::to_owned)
            .collect();
    }
    let text = text.replace('\\', "/");
    let text = if let Some(rest) = text.strip_prefix("//?/UNC/") {
        format!("//{rest}")
    } else if let Some(rest) = text
        .strip_prefix("//?/")
        .or_else(|| text.strip_prefix("//./"))
    {
        rest.to_owned()
    } else {
        text
    };
    text.split('/')
        .filter(|c| !c.is_empty() && *c != ".")
        .map(str::to_lowercase)
        .collect()
}

/// Same directory, ignoring separators, trailing slashes and (on Windows) case.
pub fn same_path(a: &Path, b: &Path, windows: bool) -> bool {
    let a = components(a, windows);
    !a.is_empty() && a == components(b, windows)
}

/// `path` is `base` or lives below it. A bare drive or filesystem root only matches itself,
/// so an install in `C:\` does not make the whole drive ambient.
fn is_inside(path: &Path, base: &Path, windows: bool) -> bool {
    let base = components(base, windows);
    let path = components(path, windows);
    if base.is_empty() {
        return false;
    }
    if base.len() < 2 && path != base {
        return false;
    }
    path.starts_with(&base)
}

/// No path arguments and a working directory nobody chose.
fn is_ambient(ctx: &Context<'_>) -> bool {
    let at_home = ctx
        .home
        .is_some_and(|home| same_path(ctx.cwd, home, ctx.windows));
    if !ctx.windows {
        return at_home;
    }
    at_home
        || ctx.exe_dir.is_some_and(|dir| is_inside(ctx.cwd, dir, true))
        || ctx
            .system_dir
            .is_some_and(|dir| is_inside(ctx.cwd, dir, true))
}

/// The folder to open and where it came from. `is_dir` is injected so tests need no disk.
pub fn resolve_root(
    targets: &Targets,
    ctx: &Context<'_>,
    is_dir: impl Fn(&Path) -> bool,
) -> (PathBuf, RootSource) {
    if let Some(dir) = &targets.dir {
        return (dir.clone(), RootSource::Directory);
    }
    if let Some(parent) = targets.files.first().and_then(|f| f.parent()) {
        return (parent.to_path_buf(), RootSource::FileParent);
    }
    let working = || (ctx.cwd.to_path_buf(), RootSource::WorkingDir);
    if !is_ambient(ctx) {
        return working();
    }
    if let Some(last) = ctx.last_folder.filter(|p| is_dir(p)) {
        return (last.to_path_buf(), RootSource::LastFolder);
    }
    let Some(home) = ctx.home else {
        return working();
    };
    if ctx.windows {
        let documents = home.join("Documents");
        if is_dir(&documents) {
            return (documents, RootSource::Documents);
        }
    }
    if is_dir(home) {
        (home.to_path_buf(), RootSource::Home)
    } else {
        working()
    }
}

/// `%SystemRoot%`, then `%windir%`, from an injected environment lookup.
pub fn windows_system_dir(get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    ["SystemRoot", "windir"]
        .into_iter()
        .filter_map(get)
        .find(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// A path for display: verbatim prefix removed, and `~` for the home directory on Unix.
pub fn display_path(path: &Path, home: Option<&Path>, windows: bool) -> String {
    let text = path.to_string_lossy();
    if windows {
        let text = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else {
            text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
        };
        return text;
    }
    if let Some(home) = home {
        let home = components(home, false);
        let path_parts = components(path, false);
        if !home.is_empty() && path_parts.starts_with(&home) {
            let rest = &path_parts[home.len()..];
            return if rest.is_empty() {
                "~".to_owned()
            } else {
                format!("~/{}", rest.join("/"))
            };
        }
    }
    text.into_owned()
}

/// The last path component, or the whole path for a root.
pub fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Persisted launch state: the last used folder and a short most-recent-first list.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct State {
    pub last_folder: Option<PathBuf>,
    pub recent: Vec<PathBuf>,
}

impl State {
    pub fn path() -> PathBuf {
        platform::config_root()
            .join(identity::CONFIG_DIR)
            .join(STATE_FILE)
    }

    /// Missing, unreadable or corrupt files give an empty state; this never fails.
    pub fn load_from(path: &Path) -> Self {
        fs::read(path)
            .map(|bytes| Self::parse(&bytes))
            .unwrap_or_default()
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    /// Tolerant parse: wrong types, relative paths and duplicates are dropped.
    pub fn parse(bytes: &[u8]) -> Self {
        let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(bytes) else {
            return Self::default();
        };
        let path = |value: &Value| {
            value
                .as_str()
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        let mut state = Self {
            last_folder: map.get("last_folder").and_then(path),
            recent: Vec::new(),
        };
        for entry in map
            .get("recent")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(path)
        {
            if state.recent.len() < MAX_RECENT
                && !state
                    .recent
                    .iter()
                    .any(|p| same_path(p, &entry, cfg!(windows)))
            {
                state.recent.push(entry);
            }
        }
        state
    }

    /// Make `folder` the last used one and the first recent entry.
    pub fn record(&mut self, folder: &Path) {
        if !folder.is_absolute() {
            return;
        }
        self.recent.retain(|p| !same_path(p, folder, cfg!(windows)));
        self.recent.insert(0, folder.to_path_buf());
        self.recent.truncate(MAX_RECENT);
        self.last_folder = Some(folder.to_path_buf());
    }

    /// Recent folders other than `current`.
    pub fn recent_excluding(&self, current: &Path) -> Vec<PathBuf> {
        self.recent
            .iter()
            .filter(|p| !same_path(p, current, cfg!(windows)))
            .cloned()
            .collect()
    }

    fn to_json(&self) -> Value {
        // Paths that are not valid Unicode cannot be written to JSON; they are skipped.
        let text = |p: &PathBuf| p.to_str().map(str::to_owned);
        json!({
            "version": 1,
            "last_folder": self.last_folder.as_ref().and_then(text),
            "recent": self.recent.iter().filter_map(text).collect::<Vec<_>>(),
        })
    }

    /// Write through a temporary file in the same directory, then rename over the target.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "state path needs a parent")
        })?;
        fs::create_dir_all(parent)?;
        let tmp = parent.join(format!(".{STATE_FILE}.{}.tmp", std::process::id()));
        let result = (|| {
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&tmp)?;
            let mut bytes = serde_json::to_vec_pretty(&self.to_json())?;
            bytes.push(b'\n');
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

/// A fully resolved launch.
#[derive(Debug, Clone)]
pub struct Launch {
    pub root: PathBuf,
    /// Files to open once the window is ready.
    pub files: Vec<PathBuf>,
    /// The command line named at least one path.
    pub has_paths: bool,
    pub source: RootSource,
    /// Loaded once here; already includes `root` when that is worth remembering.
    pub state: State,
}

impl Launch {
    /// The folder to write to `state.json` once a window shows it.
    pub fn record_target(&self) -> Option<&Path> {
        self.source.recordable().then_some(self.root.as_path())
    }
}

fn canonical(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

/// Resolve against the real process environment and read `state.json` (one small file).
pub fn prepare(targets: Targets, cwd: &Path) -> Launch {
    let mut state = State::load();
    let windows = cfg!(windows);
    let cwd = canonical(cwd.to_path_buf());
    let home = platform::home_dir()
        .filter(|h| !h.as_os_str().is_empty())
        .map(canonical);
    let exe_dir = if windows {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .map(canonical)
    } else {
        None
    };
    let system_dir = if windows {
        windows_system_dir(|key| std::env::var_os(key))
            .or_else(|| Some(PathBuf::from(r"C:\Windows")))
            .map(canonical)
    } else {
        None
    };
    let ctx = Context {
        windows,
        cwd: &cwd,
        exe_dir: exe_dir.as_deref(),
        home: home.as_deref(),
        system_dir: system_dir.as_deref(),
        last_folder: state.last_folder.as_deref(),
    };
    let (root, source) = resolve_root(&targets, &ctx, |p| p.is_dir());
    if source.recordable() {
        state.record(&root);
    }
    Launch {
        has_paths: !targets.is_empty(),
        files: targets.files,
        root,
        source,
        state,
    }
}

// ---- Shared in-memory view (GTK main thread) and background persistence -------------

#[derive(Default)]
struct Shared {
    state: State,
    /// Recent folders found missing on disk this session; hidden, never deleted.
    unavailable: HashSet<PathBuf>,
    observers: Vec<Box<dyn Fn() -> bool>>,
}

thread_local! { static SHARED: RefCell<Shared> = RefCell::new(Shared::default()); }

/// Serialises read-modify-write cycles of `state.json` within this process.
static WRITE_LOCK: Mutex<()> = Mutex::new(());
/// Completion signals of the background writes that may still be running.
static PENDING: Mutex<Vec<mpsc::Receiver<()>>> = Mutex::new(Vec::new());

/// Install the state loaded by [`prepare`] as the in-memory view.
pub fn seed(state: State) {
    SHARED.with(|s| s.borrow_mut().state = state);
    notify();
}

/// Recent folders, newest first, without those known to be gone.
pub fn recent_folders() -> Vec<PathBuf> {
    SHARED.with(|s| {
        let s = s.borrow();
        s.state
            .recent
            .iter()
            .filter(|p| !s.unavailable.contains(*p))
            .cloned()
            .collect()
    })
}

/// Recent folders other than `current`.
pub fn recent_excluding(current: &Path) -> Vec<PathBuf> {
    recent_folders()
        .into_iter()
        .filter(|p| !same_path(p, current, cfg!(windows)))
        .collect()
}

/// Call `f` whenever the recent list changes; return false from `f` to unsubscribe.
pub fn connect_recent_changed(f: impl Fn() -> bool + 'static) {
    SHARED.with(|s| s.borrow_mut().observers.push(Box::new(f)));
}

fn notify() {
    let mut observers = SHARED.with(|s| std::mem::take(&mut s.borrow_mut().observers));
    observers.retain(|f| f());
    SHARED.with(|s| {
        let mut s = s.borrow_mut();
        observers.append(&mut s.observers);
        s.observers = observers;
    });
}

/// Read-modify-write `state.json`; a failure (read-only profile, full disk) is ignored.
fn persist(path: &Path, folder: Option<&Path>) -> State {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = State::load_from(path);
    if let Some(folder) = folder {
        state.record(folder);
        let _ = state.save_to(path);
    }
    state
}

fn spawn_persist(folder: Option<PathBuf>, check_missing: bool) {
    let path = State::path();
    let (done, wait) = mpsc::channel();
    {
        let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        // Forget writes that already finished.
        pending.retain(|w| matches!(w.try_recv(), Err(mpsc::TryRecvError::Empty)));
        pending.push(wait);
    }
    let _ = std::thread::Builder::new()
        .name("impro-state".into())
        .spawn(move || {
            let state = persist(&path, folder.as_deref());
            let _ = done.send(());
            if !check_missing {
                return;
            }
            // Directories that are gone are hidden for this session (never deleted from
            // the file: a removable drive may come back). Checked here, off the UI thread.
            let missing: Vec<PathBuf> = state.recent.into_iter().filter(|p| !p.is_dir()).collect();
            if !missing.is_empty() {
                glib::MainContext::default().invoke(move || {
                    SHARED.with(|s| s.borrow_mut().unavailable.extend(missing));
                    notify();
                });
            }
        });
}

/// Give writes that are still in flight a moment to finish; call just before the process
/// exits so closing the window right after a folder change cannot lose it.
pub fn flush() {
    let pending = std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()));
    for wait in pending {
        let _ = wait.recv_timeout(Duration::from_millis(400));
    }
}

/// A window now shows `folder`: update the in-memory list and persist it off the UI thread.
pub fn note_folder(folder: &Path) {
    SHARED.with(|s| {
        let mut s = s.borrow_mut();
        s.state.record(folder);
        s.unavailable.remove(folder);
    });
    notify();
    spawn_persist(Some(folder.to_path_buf()), false);
}

/// Startup bookkeeping once the first window exists: remember the folder (when it is a real
/// choice) and hide recent folders that no longer exist. All file system work runs on a thread.
pub fn start_background(folder: Option<&Path>) {
    spawn_persist(folder.map(Path::to_path_buf), true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    fn p(text: &str) -> PathBuf {
        PathBuf::from(text)
    }
    fn unix<'a>(cwd: &'a Path, home: Option<&'a Path>, last: Option<&'a Path>) -> Context<'a> {
        Context {
            windows: false,
            cwd,
            exe_dir: None,
            home,
            system_dir: None,
            last_folder: last,
        }
    }
    struct Win {
        cwd: PathBuf,
        exe: PathBuf,
        home: PathBuf,
        system: PathBuf,
        last: Option<PathBuf>,
    }
    impl Win {
        fn new(cwd: &str) -> Self {
            Self {
                cwd: p(cwd),
                exe: p(r"C:\Users\me\AppData\Local\Programs\impro"),
                home: p(r"C:\Users\me"),
                system: p(r"C:\Windows"),
                last: None,
            }
        }
        fn ctx(&self) -> Context<'_> {
            Context {
                windows: true,
                cwd: &self.cwd,
                exe_dir: Some(&self.exe),
                home: Some(&self.home),
                system_dir: Some(&self.system),
                last_folder: self.last.as_deref(),
            }
        }
    }
    fn nothing(_: &Path) -> bool {
        false
    }
    fn everything(_: &Path) -> bool {
        true
    }
    fn root(targets: &Targets, ctx: &Context<'_>, dirs: impl Fn(&Path) -> bool) -> PathBuf {
        resolve_root(targets, ctx, dirs).0
    }

    // ---- classify ----

    #[test]
    fn classify_splits_directories_and_files_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        for name in ["a.txt", "b.txt"] {
            fs::write(base.join(name), "x").unwrap();
        }
        fs::create_dir(base.join("one")).unwrap();
        fs::create_dir(base.join("two")).unwrap();
        let got = classify(
            &args(&["b.txt", "one", "a.txt", "two", "b.txt"]),
            &base,
            |p| p.to_path_buf(),
        )
        .unwrap();
        assert_eq!(got.dir, Some(base.join("one")), "first directory wins");
        assert_eq!(got.files, vec![base.join("b.txt"), base.join("a.txt")]);
        assert!(!got.is_empty());
    }

    #[test]
    fn classify_without_arguments_is_empty() {
        let got = classify(&[], Path::new("/"), |p| p.to_path_buf()).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn classify_reports_missing_paths_as_typed() {
        let dir = tempfile::tempdir().unwrap();
        let err = classify(&args(&["nope/x.txt"]), dir.path(), |p| p.to_path_buf()).unwrap_err();
        assert_eq!(err, "no such file or directory: nope/x.txt");
        let err =
            classify(&args(&["/does-not-exist"]), dir.path(), |p| p.to_path_buf()).unwrap_err();
        assert_eq!(err, "no such file or directory: /does-not-exist");
    }

    #[test]
    fn classify_stops_at_the_first_bad_argument() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ok.txt"), "x").unwrap();
        let err = classify(
            &args(&["ok.txt", "missing-1", "missing-2"]),
            dir.path(),
            |p| p.to_path_buf(),
        )
        .unwrap_err();
        assert!(err.ends_with("missing-1"), "{err}");
    }

    #[test]
    fn classify_passes_every_argument_through_the_expander_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let expected = dir.path().canonicalize().unwrap();
        for raw in [
            r"C:\Users\x\proj",
            "C:/Users/x/proj",
            r"\\server\share\proj",
            r"~\proj",
            "~/proj",
        ] {
            let got = classify(&args(&[raw]), dir.path(), |seen| {
                assert_eq!(seen.as_os_str(), OsStr::new(raw));
                dir.path().to_path_buf()
            })
            .unwrap();
            assert_eq!(got.dir, Some(expected.clone()));
        }
    }

    #[test]
    fn classify_accepts_file_uris() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let file = base.join("a b.txt");
        fs::write(&file, "x").unwrap();
        let uri = format!("file://{}", base.join("a%20b.txt").display());
        let got = classify(&args(&[&uri]), &base, |p| p.to_path_buf()).unwrap();
        assert_eq!(got.files, vec![file]);
        // Not a URI: stays a (relative, missing) path.
        let err = classify(&args(&["file:nope"]), &base, |p| p.to_path_buf()).unwrap_err();
        assert!(err.contains("file:nope"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn classify_rejects_special_files() {
        let err = classify(&args(&["/dev/null"]), Path::new("/"), |p| p.to_path_buf()).unwrap_err();
        assert_eq!(err, "not a file or directory: /dev/null");
    }

    // ---- resolve_root: explicit arguments ----

    #[test]
    fn a_directory_argument_is_the_root_even_in_an_ambient_directory() {
        let home = p("/home/me");
        let ctx = unix(&home, Some(&home), Some(Path::new("/work/last")));
        let targets = Targets {
            dir: Some(p("/srv/project")),
            files: vec![],
        };
        assert_eq!(
            resolve_root(&targets, &ctx, everything),
            (p("/srv/project"), RootSource::Directory)
        );
        // `impro .` in the home directory keeps opening home.
        let targets = Targets {
            dir: Some(home.clone()),
            files: vec![],
        };
        assert_eq!(root(&targets, &ctx, everything), home);
    }

    #[test]
    fn a_directory_with_files_keeps_the_directory_root() {
        let cwd = p("/elsewhere");
        let targets = Targets {
            dir: Some(p("/srv/project")),
            files: vec![p("/other/a.txt")],
        };
        assert_eq!(
            root(&targets, &unix(&cwd, None, None), nothing),
            p("/srv/project")
        );
    }

    #[test]
    fn files_only_use_the_first_files_parent() {
        let cwd = p("/home/me");
        let targets = Targets {
            dir: None,
            files: vec![p("/srv/project/src/a.rs"), p("/elsewhere/b.rs")],
        };
        assert_eq!(
            resolve_root(&targets, &unix(&cwd, Some(&cwd), None), nothing),
            (p("/srv/project/src"), RootSource::FileParent)
        );
    }

    // ---- resolve_root: ambient Unix launches ----

    #[test]
    fn unix_terminal_directory_is_kept() {
        let cwd = p("/home/me/code/app");
        let home = p("/home/me");
        let last = p("/work/last");
        let ctx = unix(&cwd, Some(&home), Some(&last));
        assert_eq!(
            resolve_root(&Targets::default(), &ctx, everything),
            (cwd.clone(), RootSource::WorkingDir)
        );
    }

    #[test]
    fn unix_home_reopens_the_last_folder_when_it_exists() {
        let home = p("/home/me");
        let last = p("/work/last");
        let ctx = unix(&home, Some(&home), Some(&last));
        assert_eq!(
            resolve_root(&Targets::default(), &ctx, everything),
            (last.clone(), RootSource::LastFolder)
        );
    }

    #[test]
    fn unix_home_falls_back_to_home_when_the_last_folder_is_gone() {
        let home = p("/home/me");
        let last = p("/work/last");
        let ctx = unix(&home, Some(&home), Some(&last));
        let only_home = |p: &Path| p == Path::new("/home/me");
        assert_eq!(
            resolve_root(&Targets::default(), &ctx, only_home),
            (home.clone(), RootSource::Home)
        );
        let ctx = unix(&home, Some(&home), None);
        assert_eq!(
            resolve_root(&Targets::default(), &ctx, everything),
            (home, RootSource::Home)
        );
    }

    #[test]
    fn unix_home_comparison_ignores_trailing_separators_and_dots() {
        let cwd = p("/home/me");
        let home = p("/home/me/");
        let last = p("/work/last");
        assert_eq!(
            root(
                &Targets::default(),
                &unix(&cwd, Some(&home), Some(&last)),
                everything
            ),
            last
        );
        let home = p("/home/./me");
        assert!(is_ambient(&unix(&cwd, Some(&home), None)));
    }

    #[test]
    fn unix_home_subdirectories_and_siblings_are_not_ambient() {
        let home = p("/home/me");
        for cwd in ["/home/me/code", "/home/mellow", "/home", "/"] {
            let cwd = p(cwd);
            assert!(!is_ambient(&unix(&cwd, Some(&home), None)), "{cwd:?}");
        }
    }

    #[test]
    fn unix_without_a_home_uses_the_working_directory() {
        let cwd = p("/somewhere");
        assert_eq!(
            resolve_root(&Targets::default(), &unix(&cwd, None, None), everything),
            (cwd, RootSource::WorkingDir)
        );
    }

    #[test]
    fn unix_missing_home_falls_back_to_the_working_directory() {
        let home = p("/home/me");
        let ctx = unix(&home, Some(&home), None);
        assert_eq!(
            resolve_root(&Targets::default(), &ctx, nothing),
            (home, RootSource::WorkingDir)
        );
    }

    #[test]
    fn unix_ignores_install_and_system_directories() {
        let cwd = p("/usr/bin");
        let ctx = Context {
            windows: false,
            cwd: &cwd,
            exe_dir: Some(Path::new("/usr/bin")),
            home: Some(Path::new("/home/me")),
            system_dir: Some(Path::new("/usr")),
            last_folder: Some(Path::new("/work/last")),
        };
        assert_eq!(
            resolve_root(&Targets::default(), &ctx, everything),
            (cwd, RootSource::WorkingDir)
        );
    }

    #[test]
    fn unix_paths_are_case_sensitive() {
        let cwd = p("/home/Me");
        assert!(!is_ambient(&unix(&cwd, Some(Path::new("/home/me")), None)));
    }

    // ---- resolve_root: Windows rules on injected paths ----

    #[test]
    fn windows_install_directory_is_ambient() {
        for cwd in [
            r"C:\Users\me\AppData\Local\Programs\impro",
            r"C:\Users\me\AppData\Local\Programs\impro\bin",
            r"c:\users\ME\appdata\local\programs\IMPRO",
            "C:/Users/me/AppData/Local/Programs/impro/share",
            r"\\?\C:\Users\me\AppData\Local\Programs\impro",
            r"C:\Users\me\AppData\Local\Programs\impro\",
        ] {
            let mut win = Win::new(cwd);
            win.last = Some(p(r"D:\work\last"));
            assert_eq!(
                resolve_root(&Targets::default(), &win.ctx(), everything),
                (p(r"D:\work\last"), RootSource::LastFolder),
                "{cwd}"
            );
        }
    }

    #[test]
    fn windows_system_directory_is_ambient() {
        for cwd in [
            r"C:\Windows",
            r"C:\Windows\System32",
            r"c:\WINDOWS\system32\",
        ] {
            let win = Win::new(cwd);
            assert!(is_ambient(&win.ctx()), "{cwd}");
        }
    }

    #[test]
    fn windows_user_profile_is_ambient_but_its_children_are_not() {
        assert!(is_ambient(&Win::new(r"C:\Users\me").ctx()));
        assert!(is_ambient(&Win::new(r"c:/users/ME/").ctx()));
        assert!(is_ambient(&Win::new(r"\\?\C:\Users\me").ctx()));
        for cwd in [
            r"C:\Users\me\source\app",
            r"C:\Users\me\Documents",
            r"C:\Users\mellow",
            r"C:\Users",
            r"D:\Users\me",
        ] {
            assert!(!is_ambient(&Win::new(cwd).ctx()), "{cwd}");
        }
    }

    #[test]
    fn windows_sibling_with_a_shared_name_prefix_is_not_inside() {
        let mut win = Win::new(r"C:\Users\me\AppData\Local\Programs\impro-extras");
        assert!(!is_ambient(&win.ctx()));
        win.cwd = p(r"C:\Windowsx");
        assert!(!is_ambient(&win.ctx()));
        win.cwd = p(r"C:\Users\me\AppData\Local\Programs");
        assert!(
            !is_ambient(&win.ctx()),
            "a parent of the install is a choice"
        );
    }

    #[test]
    fn windows_terminal_directory_is_kept() {
        let mut win = Win::new(r"D:\src\app");
        win.last = Some(p(r"D:\work\last"));
        assert_eq!(
            resolve_root(&Targets::default(), &win.ctx(), everything),
            (p(r"D:\src\app"), RootSource::WorkingDir)
        );
    }

    #[test]
    fn windows_ambient_prefers_last_then_documents_then_profile() {
        let mut win = Win::new(r"C:\Windows\System32");
        win.last = Some(p(r"D:\work\last"));
        assert_eq!(
            resolve_root(&Targets::default(), &win.ctx(), everything).1,
            RootSource::LastFolder
        );
        let last_gone = |path: &Path| path != Path::new(r"D:\work\last");
        let (documents, source) = resolve_root(&Targets::default(), &win.ctx(), last_gone);
        assert_eq!(source, RootSource::Documents);
        assert_eq!(documents, p(r"C:\Users\me").join("Documents"));
        let no_documents = |path: &Path| path == Path::new(r"C:\Users\me");
        assert_eq!(
            resolve_root(&Targets::default(), &win.ctx(), no_documents),
            (p(r"C:\Users\me"), RootSource::Home)
        );
        win.last = None;
        assert_eq!(
            resolve_root(&Targets::default(), &win.ctx(), everything).1,
            RootSource::Documents
        );
    }

    #[test]
    fn windows_explicit_arguments_override_the_install_directory() {
        let mut win = Win::new(r"C:\Users\me\AppData\Local\Programs\impro");
        win.last = Some(p(r"D:\work\last"));
        let dir = Targets {
            dir: Some(p(r"C:\Users\me\AppData\Local\Programs\impro")),
            files: vec![],
        };
        assert_eq!(
            resolve_root(&dir, &win.ctx(), everything),
            (
                p(r"C:\Users\me\AppData\Local\Programs\impro"),
                RootSource::Directory
            )
        );
        let file = Targets {
            dir: None,
            files: vec![p(r"E:\notes\todo.md")],
        };
        let (root, source) = resolve_root(&file, &win.ctx(), everything);
        assert_eq!(source, RootSource::FileParent);
        assert_eq!(root, p(r"E:\notes\todo.md").parent().unwrap());
    }

    #[test]
    fn a_bare_root_install_only_matches_itself() {
        let mut win = Win::new(r"C:\projects\app");
        win.exe = p(r"C:\");
        assert!(!is_ambient(&win.ctx()));
        win.cwd = p(r"C:\");
        assert!(is_ambient(&win.ctx()));
    }

    #[test]
    fn windows_unc_paths_compare_without_the_verbatim_prefix() {
        let mut win = Win::new(r"\\?\UNC\server\share\home\me");
        win.home = p(r"\\server\share\home\me");
        assert!(is_ambient(&win.ctx()));
        win.cwd = p(r"\\server\share\home\me\code");
        assert!(!is_ambient(&win.ctx()));
    }

    #[test]
    fn windows_without_injected_directories_never_panics() {
        let cwd = p(r"C:\x");
        let ctx = Context {
            windows: true,
            cwd: &cwd,
            exe_dir: None,
            home: None,
            system_dir: None,
            last_folder: None,
        };
        assert_eq!(
            resolve_root(&Targets::default(), &ctx, everything),
            (cwd, RootSource::WorkingDir)
        );
    }

    #[test]
    fn same_path_edge_cases() {
        assert!(!same_path(Path::new(""), Path::new(""), false));
        assert!(same_path(Path::new("/a/b"), Path::new("/a/b/"), false));
        assert!(!same_path(Path::new("/a/b"), Path::new("/a"), false));
        assert!(same_path(Path::new(r"C:\A\B"), Path::new("c:/a/b"), true));
        assert!(!same_path(Path::new(r"C:\A\B"), Path::new("c:/a/b"), false));
    }

    #[test]
    fn system_directory_prefers_systemroot_then_windir() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        assert_eq!(
            windows_system_dir(env(&[("SystemRoot", r"C:\Windows"), ("windir", r"D:\W")])),
            Some(p(r"C:\Windows"))
        );
        assert_eq!(
            windows_system_dir(env(&[("SystemRoot", ""), ("windir", r"D:\W")])),
            Some(p(r"D:\W"))
        );
        assert_eq!(windows_system_dir(env(&[])), None);
    }

    #[test]
    fn only_chosen_folders_are_recorded() {
        for (source, recorded) in [
            (RootSource::Directory, true),
            (RootSource::FileParent, true),
            (RootSource::WorkingDir, true),
            (RootSource::LastFolder, true),
            (RootSource::Documents, false),
            (RootSource::Home, false),
        ] {
            assert_eq!(source.recordable(), recorded, "{source:?}");
        }
    }

    // ---- display ----

    #[test]
    fn display_paths_use_tilde_and_hide_verbatim_prefixes() {
        let home = p("/home/me");
        assert_eq!(display_path(Path::new("/home/me"), Some(&home), false), "~");
        assert_eq!(
            display_path(Path::new("/home/me/code/app"), Some(&home), false),
            "~/code/app"
        );
        assert_eq!(
            display_path(Path::new("/home/mellow/x"), Some(&home), false),
            "/home/mellow/x"
        );
        assert_eq!(display_path(Path::new("/srv/x"), None, false), "/srv/x");
        assert_eq!(
            display_path(Path::new(r"\\?\C:\Users\me\app"), None, true),
            r"C:\Users\me\app"
        );
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share\app"), None, true),
            r"\\server\share\app"
        );
        assert_eq!(display_path(Path::new(r"D:\app"), None, true), r"D:\app");
    }

    #[test]
    fn folder_names() {
        assert_eq!(folder_name(Path::new("/srv/project")), "project");
        assert_eq!(folder_name(Path::new("/")), "/");
    }

    // ---- state ----

    #[test]
    fn record_moves_to_the_front_without_duplicates_and_caps_the_list() {
        let mut state = State::default();
        for n in 0..14 {
            state.record(Path::new(&format!("/work/{n}")));
        }
        assert_eq!(state.recent.len(), MAX_RECENT);
        assert_eq!(state.recent[0], p("/work/13"));
        assert_eq!(state.last_folder, Some(p("/work/13")));
        state.record(Path::new("/work/9/"));
        assert_eq!(state.recent[0], p("/work/9/"));
        assert_eq!(
            state
                .recent
                .iter()
                .filter(|path| same_path(path, Path::new("/work/9"), false))
                .count(),
            1
        );
        assert_eq!(state.recent.len(), MAX_RECENT);
    }

    #[test]
    fn record_ignores_relative_paths() {
        let mut state = State::default();
        state.record(Path::new("relative/dir"));
        assert_eq!(state, State::default());
    }

    #[test]
    fn recent_excluding_drops_the_current_folder_only() {
        let mut state = State::default();
        for dir in ["/a", "/b", "/c"] {
            state.record(Path::new(dir));
        }
        assert_eq!(
            state.recent_excluding(Path::new("/b/")),
            vec![p("/c"), p("/a")]
        );
    }

    #[test]
    fn parse_tolerates_garbage() {
        for bytes in [
            &b""[..],
            b"not json",
            b"[1,2]",
            b"null",
            b"{\"recent\": 5}",
            b"{\"recent\": [1, null, {}], \"last_folder\": 7}",
            b"\xff\xfe\x00",
        ] {
            assert_eq!(State::parse(bytes), State::default(), "{bytes:?}");
        }
    }

    #[test]
    fn parse_keeps_valid_entries_only() {
        let state =
            State::parse(br#"{"last_folder":"/a","recent":["/a","rel","/b","/a/",7,"/c"]}"#);
        assert_eq!(state.last_folder, Some(p("/a")));
        assert_eq!(state.recent, vec![p("/a"), p("/b"), p("/c")]);
        let long: Vec<String> = (0..30).map(|n| format!("/d/{n}")).collect();
        let state = State::parse(json!({ "recent": long }).to_string().as_bytes());
        assert_eq!(state.recent.len(), MAX_RECENT);
        assert_eq!(state.recent[0], p("/d/0"));
        assert_eq!(state.last_folder, None);
    }

    #[test]
    fn state_round_trips_through_an_atomic_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/impro/state.json");
        let mut state = State::default();
        state.record(Path::new("/one"));
        state.record(Path::new("/two"));
        state.save_to(&path).unwrap();
        assert_eq!(State::load_from(&path), state);
        // No temporary file is left behind and a rewrite replaces the content.
        state.record(Path::new("/three"));
        state.save_to(&path).unwrap();
        assert_eq!(State::load_from(&path), state);
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![OsString::from("state.json")]);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 1"), "{text}");
    }

    #[cfg(unix)]
    #[test]
    fn state_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        State::default().save_to(&path).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn load_survives_a_missing_unreadable_or_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            State::load_from(&dir.path().join("absent.json")),
            State::default()
        );
        let corrupt = dir.path().join("corrupt.json");
        fs::write(&corrupt, "{ torn").unwrap();
        assert_eq!(State::load_from(&corrupt), State::default());
        // A directory where the file should be is simply "no state".
        let blocked = dir.path().join("blocked.json");
        fs::create_dir(&blocked).unwrap();
        assert_eq!(State::load_from(&blocked), State::default());
        assert!(State::default().save_to(&blocked).is_err());
        let leftovers = fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            })
            .count();
        assert_eq!(leftovers, 0, "a failed write cleans up its temporary file");
    }

    #[test]
    fn persist_merges_with_the_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut other = State::default();
        other.record(Path::new("/from-another-window"));
        other.save_to(&path).unwrap();
        let merged = persist(&path, Some(Path::new("/mine")));
        assert_eq!(merged.recent, vec![p("/mine"), p("/from-another-window")]);
        assert_eq!(State::load_from(&path), merged);
        // Without a folder nothing is written.
        fs::remove_file(&path).unwrap();
        persist(&path, None);
        assert!(!path.exists());
    }

    #[test]
    fn persist_into_an_unwritable_location_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "x").unwrap();
        // The parent "directory" is a regular file.
        let state = persist(&file.join("state.json"), Some(Path::new("/x")));
        assert_eq!(state.recent, vec![p("/x")]);
        assert_eq!(fs::read_to_string(&file).unwrap(), "x");
    }

    // ---- in-memory view ----

    #[test]
    fn shared_view_tracks_notes_and_hides_unavailable_entries() {
        let mut state = State::default();
        state.record(Path::new("/a"));
        state.record(Path::new("/b"));
        seed(state);
        assert_eq!(recent_folders(), vec![p("/b"), p("/a")]);
        assert_eq!(recent_excluding(Path::new("/b")), vec![p("/a")]);
        SHARED.with(|s| s.borrow_mut().unavailable.insert(p("/a")));
        assert_eq!(recent_folders(), vec![p("/b")]);
    }

    #[test]
    fn observers_run_on_change_and_unsubscribe_by_returning_false() {
        use std::{cell::Cell, rc::Rc};
        let calls = Rc::new(Cell::new(0));
        let seen = calls.clone();
        connect_recent_changed(move || {
            seen.set(seen.get() + 1);
            seen.get() < 2
        });
        seed(State::default());
        seed(State::default());
        seed(State::default());
        assert_eq!(calls.get(), 2, "removed after returning false");
    }

    #[test]
    fn observers_may_read_the_shared_view_and_subscribe_again() {
        use std::{cell::Cell, rc::Rc};
        let reads = Rc::new(Cell::new(0));
        let seen = reads.clone();
        connect_recent_changed(move || {
            let _ = recent_folders();
            seen.set(seen.get() + 1);
            connect_recent_changed(|| false);
            true
        });
        seed(State::default());
        seed(State::default());
        assert_eq!(reads.get(), 2);
    }
}
