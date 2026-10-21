//! Command recency and file-list adapters. File enumeration belongs to the caller.
use crate::{
    fuzzy::{self, FuzzyIndex, MAX_RESULTS},
    Prompt, PromptItem,
};
use gtk::prelude::*;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
#[derive(Clone)]
pub struct Command {
    pub label: String,
    pub callback: Rc<dyn Fn()>,
    pub hotkey: Vec<String>,
    pub id: String,
}
impl Command {
    pub fn new(
        label: impl Into<String>,
        callback: impl Fn() + 'static,
        hotkey: Vec<String>,
    ) -> Self {
        let label = label.into();
        Self {
            id: label.clone(),
            label,
            callback: Rc::new(callback),
            hotkey,
        }
    }
}
type CommandRun = Rc<dyn Fn(&Command)>;
#[derive(Clone)]
pub struct CommandPalette {
    prompt: Prompt,
    recent: Rc<RefCell<Vec<String>>>,
    pub recent_limit: usize,
    run: Rc<RefCell<Vec<CommandRun>>>,
}
impl std::ops::Deref for CommandPalette {
    type Target = Prompt;
    fn deref(&self) -> &Prompt {
        &self.prompt
    }
}
impl CommandPalette {
    pub fn new(commands_provider: impl Fn() -> Vec<Command> + 'static) -> Self {
        let index = Rc::new(RefCell::new(FuzzyIndex::new(
            vec![],
            |c: &Command| c.label.clone(),
            false,
        )));
        let recent = Rc::new(RefCell::new(Vec::<String>::new()));
        let idx = index.clone();
        let rec = recent.clone();
        let prompt = Prompt::new(
            move |query| {
                let values = if query.trim().is_empty() {
                    let mut commands = idx.borrow().candidates.clone();
                    let recent = rec.borrow().clone();
                    commands.sort_by_key(|c| {
                        recent
                            .iter()
                            .position(|s| s == &c.id)
                            .unwrap_or(recent.len())
                    });
                    commands
                        .into_iter()
                        .take(MAX_RESULTS)
                        .map(|c| (c, 0., vec![]))
                        .collect()
                } else {
                    idx.borrow_mut().search(query, Some(MAX_RESULTS))
                };
                values
                    .into_iter()
                    .map(|(c, _, ranges)| {
                        PromptItem {
                            label: c.label.clone(),
                            hotkey: c.hotkey.clone(),
                            label_ranges: ranges,
                            ..Default::default()
                        }
                        .with_data(c)
                    })
                    .collect()
            },
            "Select a command...",
        );
        prompt.set_empty_text("No commands found.");
        prompt.set_loader(move || {
            let commands = commands_provider();
            *index.borrow_mut() = FuzzyIndex::new(commands, |c| c.label.clone(), false);
        });
        let this = Self {
            prompt,
            recent,
            recent_limit: 20,
            run: Rc::new(RefCell::new(vec![])),
        };
        let recent = this.recent.clone();
        let run = this.run.clone();
        let limit = this.recent_limit;
        this.prompt.connect_chosen(move |item, _| {
            if let Some(c) = item.data::<Command>() {
                {
                    let mut rec = recent.borrow_mut();
                    rec.retain(|id| id != &c.id);
                    rec.insert(0, c.id.clone());
                    rec.truncate(limit);
                }
                (c.callback)();
                let callbacks = run.borrow().clone();
                for f in callbacks {
                    f(c);
                }
            }
        });
        this
    }
    pub fn recent(&self) -> Vec<String> {
        self.recent.borrow().clone()
    }
    pub fn set_recent(&self, recent: Vec<String>) {
        *self.recent.borrow_mut() = recent;
    }
    pub fn connect_command_run(&self, callback: impl Fn(&Command) + 'static) {
        self.run.borrow_mut().push(Rc::new(callback));
    }
}
#[derive(Clone)]
struct FileEntry {
    shown: String,
    path: PathBuf,
}
fn file_item(file: &FileEntry, ranges: Vec<fuzzy::Range>) -> PromptItem {
    let start = fuzzy::name_start(&file.shown);
    let chars: Vec<char> = file.shown.chars().collect();
    let (folder, name) = fuzzy::split_path_ranges(&file.shown, &ranges);
    PromptItem {
        label: chars[start..].iter().collect(),
        detail: chars[..start.saturating_sub(1)].iter().collect(),
        label_ranges: name,
        detail_ranges: folder,
        ..Default::default()
    }
    .with_data(file.path.clone())
}
struct Files {
    index: Mutex<FuzzyIndex<FileEntry>>,
    empty: Vec<FileEntry>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}
impl Drop for Files {
    fn drop(&mut self) {
        if let Ok(c) = self.cancel.get_mut() {
            if let Some(c) = c {
                c.store(true, Ordering::Relaxed);
            }
        }
    }
}
#[derive(Clone)]
pub struct QuickSwitcher {
    prompt: Prompt,
    root: Rc<RefCell<PathBuf>>,
    files: Rc<RefCell<Arc<Files>>>,
}
impl std::ops::Deref for QuickSwitcher {
    type Target = Prompt;
    fn deref(&self) -> &Prompt {
        &self.prompt
    }
}
impl QuickSwitcher {
    /// File paths are supplied explicitly, with no filesystem work on GTK's thread.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let files = Arc::new(Files {
            index: Mutex::new(FuzzyIndex::new(
                vec![],
                |f: &FileEntry| f.shown.clone(),
                true,
            )),
            empty: vec![],
            cancel: Mutex::new(None),
        });
        let state = Rc::new(RefCell::new(files));
        let prompt = Prompt::new(|_| vec![], "Find or open a file...");
        prompt.set_empty_text("No files found.");
        prompt.set_instructions(&[
            ("↑↓", "to navigate"),
            ("↵", "to open"),
            ("ctrl ↵", "to open in new tab"),
            ("esc", "to dismiss"),
        ]);
        let this = Self {
            prompt,
            root: Rc::new(RefCell::new(root.into())),
            files: state,
        };
        this.install_provider();
        this
    }
    pub fn root(&self) -> PathBuf {
        self.root.borrow().clone()
    }
    /// Set before `set_files` when the project changes; supplied paths stay intact.
    pub fn set_root(&self, root: impl Into<PathBuf>) {
        *self.root.borrow_mut() = root.into();
    }
    pub fn display_path(&self, path: &Path) -> String {
        display_path(path, &self.root.borrow())
    }
    /// Replace the index from the caller's file list. Reindexing happens on a worker
    /// for large lists; the first empty-query page is available immediately.
    pub fn set_files(&self, paths: Vec<PathBuf>, recent: Vec<PathBuf>) {
        let root = self.root();
        let mut seen = HashSet::new();
        let mut entries: Vec<_> = paths
            .into_iter()
            .filter_map(|path| {
                let shown = display_path(&path, &root);
                seen.insert(shown.clone())
                    .then_some(FileEntry { shown, path })
            })
            .collect();
        entries.sort_by(|a, b| {
            fuzzy::fold(&a.shown)
                .cmp(&fuzzy::fold(&b.shown))
                .then(a.shown.cmp(&b.shown))
        });
        let lookup: HashMap<_, _> = entries
            .iter()
            .enumerate()
            .map(|(i, f)| (f.shown.clone(), i))
            .collect();
        let mut empty = vec![];
        let mut seen = HashSet::new();
        for path in recent {
            let shown = display_path(&path, &root);
            if let Some(&i) = lookup.get(&shown) {
                if seen.insert(i) {
                    empty.push(entries[i].clone());
                }
            }
            if empty.len() == MAX_RESULTS {
                break;
            }
        }
        for (i, f) in entries.iter().enumerate() {
            if empty.len() == MAX_RESULTS {
                break;
            }
            if seen.insert(i) {
                empty.push(f.clone());
            }
        }
        let files = Arc::new(Files {
            index: Mutex::new(FuzzyIndex::new(
                vec![],
                |f: &FileEntry| f.shown.clone(),
                true,
            )),
            empty,
            cancel: Mutex::new(None),
        });
        if let Ok(cancel) = self.files.borrow().cancel.lock() {
            if let Some(c) = cancel.as_ref() {
                c.store(true, Ordering::Relaxed);
            }
        }
        // Acquire before spawning so a query cannot observe an unbuilt index.
        let (send, recv) = std::sync::mpsc::sync_channel(1);
        let worker = files.clone();
        let large = entries.len() > 5000;
        if large {
            std::thread::spawn(move || {
                if let Ok(mut index) = worker.index.lock() {
                    let _ = send.send(());
                    *index = FuzzyIndex::new(entries, |f| f.shown.clone(), true);
                }
            });
            let _ = recv.recv();
        } else if let Ok(mut index) = files.index.lock() {
            *index = FuzzyIndex::new(entries, |f| f.shown.clone(), true);
        }
        *self.files.borrow_mut() = files;
        self.install_provider();
    }
    fn install_provider(&self) {
        let state = self.files.clone();
        let widget = self.prompt.widget().downgrade();
        let weak_prompt = self.prompt.downgrade();
        self.prompt.set_provider(move |query| {
            let files = state.borrow().clone();
            if let Ok(mut old) = files.cancel.lock() {
                if let Some(c) = old.take() {
                    c.store(true, Ordering::Relaxed);
                }
            }
            if query.trim().is_empty() {
                return files.empty.iter().map(|f| file_item(f, vec![])).collect();
            }
            {
                let sync = files.index.try_lock().ok().filter(|idx| idx.len() <= 5000);
                if let Some(mut index) = sync {
                    return index
                        .search(query, Some(MAX_RESULTS))
                        .into_iter()
                        .map(|(f, _, r)| file_item(&f, r))
                        .collect();
                }
            }
            let cancel = Arc::new(AtomicBool::new(false));
            if let Ok(mut current) = files.cancel.lock() {
                *current = Some(cancel.clone());
            }
            let query = query.to_owned();
            let expected = query.clone();
            #[allow(deprecated)]
            let (sender, receiver) = glib::MainContext::channel::<
                Vec<(FileEntry, Vec<fuzzy::Range>)>,
            >(glib::Priority::DEFAULT);
            let weak = weak_prompt.clone();
            let w = widget.clone();
            receiver.attach(None, move |results| {
                if w.upgrade().is_some() {
                    if let Some(p) = weak.upgrade() {
                        if p.query() == expected && p.is_open() {
                            p.set_items(
                                results.into_iter().map(|(f, r)| file_item(&f, r)).collect(),
                            );
                        }
                    }
                }
                glib::ControlFlow::Break
            });
            std::thread::spawn(move || {
                let results = files
                    .index
                    .lock()
                    .map(|mut i| i.search_cancelled(&query, Some(MAX_RESULTS), Some(&cancel)))
                    .unwrap_or_default();
                if !cancel.load(Ordering::Relaxed) {
                    let _ = sender.send(results.into_iter().map(|(f, _, r)| (f, r)).collect());
                }
            });
            vec![]
        });
    }
}
fn display_path(path: &Path, root: &Path) -> String {
    let absolute = |p: &Path| {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        }
    };
    let path_abs = absolute(path);
    let root_abs = absolute(root);
    if !root.as_os_str().is_empty() {
        if let Ok(relative) = path_abs.strip_prefix(root_abs) {
            return if relative.as_os_str().is_empty() {
                ".".into()
            } else {
                relative.to_string_lossy().into()
            };
        }
    }
    path.to_string_lossy().into()
}
