//! Code cards: copy, the language chooser and worker-side highlighting.
use super::{CodeRun, Inner};
use crate::{blockedit, mdview};
use gtk::prelude::*;
use std::hash::{Hash, Hasher};

const LANGUAGES: [&str; 19] = [
    "", "python", "javascript", "typescript", "json", "html", "css", "bash", "sql", "markdown",
    "yaml", "toml", "rust", "go", "c", "cpp", "java", "ruby", "mermaid",
];

fn hash_of(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

impl Inner {
    /// Ask the worker to highlight the fenced code of block `id` (once per text).
    pub(super) fn request_highlight(&self, id: u64) {
        let Some(highlighter) = self.highlighter.borrow().clone() else {
            return;
        };
        let (language, content, hash) = {
            let blocks = self.blocks.borrow();
            let Some(index) = blocks.find_id(id) else {
                return;
            };
            let deco = &blocks.items[index];
            let Some(info) = &deco.code else {
                return;
            };
            if info.language.is_empty() {
                return;
            }
            let hash = hash_of(&deco.text);
            if info.highlighted == Some(hash) {
                return;
            }
            let lines: Vec<&str> = deco.text.split_inclusive('\n').collect();
            let take = if deco.closed {
                lines.len().saturating_sub(2)
            } else {
                lines.len().saturating_sub(1)
            };
            let content: String = lines.iter().skip(1).take(take).copied().collect();
            (info.language.clone(), content, hash)
        };
        if content.is_empty() {
            return;
        }
        {
            let mut blocks = self.blocks.borrow_mut();
            if let Some(index) = blocks.find_id(id) {
                if let Some(info) = &mut blocks.items[index].code {
                    info.highlighted = Some(hash);
                }
            }
        }
        self.jobs.submit(
            &self.weak,
            super::images::Job::Code {
                id,
                hash,
                language,
                content,
                highlighter,
            },
        );
    }

    /// Apply worker runs: tags over the block's content lines.
    pub(super) fn highlight_done(&self, id: u64, hash: u64, runs: Vec<CodeRun>) {
        let (start, content_start) = {
            let blocks = self.blocks.borrow();
            let Some(index) = blocks.find_id(id) else {
                return;
            };
            let deco = &blocks.items[index];
            if hash_of(&deco.text) != hash || !deco.painted {
                return;
            }
            let Some(info) = &deco.code else {
                return;
            };
            (blocks.spans[index].0, blocks.spans[index].0 + info.first_len)
        };
        let _ = start;
        let mut applied = Vec::new();
        for run in runs {
            if run.end <= run.start {
                continue;
            }
            let tag = self.tag(&format!("syntax-{}", run.kind));
            let a = content_start + run.start as i32;
            let b = content_start + run.end as i32;
            self.buffer.apply_tag(&tag, &self.iter(a), &self.iter(b));
            if !applied.contains(&tag) {
                applied.push(tag);
            }
        }
        let mut blocks = self.blocks.borrow_mut();
        if let Some(index) = blocks.find_id(id) {
            for tag in applied {
                if !blocks.items[index].applied.contains(&tag) {
                    blocks.items[index].applied.push(tag);
                }
            }
        }
    }

    pub(super) fn copy_code(&self, button: &gtk::Button, id: u64, first_len: i32, end_rel: i32) {
        let Some(start) = self
            .blocks
            .borrow()
            .find_id(id)
            .map(|i| self.blocks.borrow().spans[i].0)
        else {
            return;
        };
        let text = self
            .buffer
            .text(&self.iter(start + first_len), &self.iter(start + end_rel), true)
            .map(|s| s.to_string())
            .unwrap_or_default();
        if let Some(display) = gdk::Display::default() {
            if let Some(clipboard) = gtk::Clipboard::default(&display) {
                clipboard.set_text(&text);
            }
        }
        button.set_label("Copied");
        button.set_tooltip_text(Some("Copied!"));
        let weak_button = button.downgrade();
        let weak = self.weak.clone();
        let slot = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let mine = slot.clone();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(1200), move || {
            if let Some(button) = weak_button.upgrade() {
                button.set_label("Copy");
                button.set_tooltip_text(Some("Copy code"));
            }
            // The source is gone once it has run: forget it so clear() never removes it.
            if let Some(s) = weak.upgrade() {
                s.feedback.borrow_mut().retain(|i| i.as_raw() != mine.get());
            }
        });
        slot.set(id.as_raw());
        self.feedback.borrow_mut().push(id);
    }

    pub(super) fn choose_language(&self, anchor: &gtk::Widget, id: u64) -> bool {
        let old = self.language_popover.borrow_mut().take();
        if let Some(old) = old {
            unsafe { old.destroy() };
        }
        let popover = gtk::Popover::new(Some(anchor));
        popover.set_property("transitions-enabled", impro_ui::motion::animations_enabled());
        *self.language_popover.borrow_mut() = Some(popover.clone());
        let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
        container.set_border_width(8);
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Search languages"));
        container.pack_start(&search, false, false, 0);
        let choices = gtk::ListBox::new();
        choices.style_context().add_class("live-md-languages");
        choices.set_selection_mode(gtk::SelectionMode::Single);
        for language in LANGUAGES {
            let row = gtk::ListBoxRow::new();
            unsafe { row.set_data("language", language.to_string()) };
            let label = gtk::Label::new(Some(if language.is_empty() { "Plain text" } else { language }));
            label.set_xalign(0.0);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_margin_top(5);
            label.set_margin_bottom(5);
            row.add(&label);
            choices.add(&row);
        }
        let query = search.clone();
        choices.set_filter_func(Some(Box::new(move |row| {
            let language = row_language(row);
            let shown = if language.is_empty() { "Plain text" } else { &language };
            shown.to_lowercase().contains(&query.text().to_lowercase())
        })));
        let list = choices.clone();
        search.connect_search_changed(move |_| list.invalidate_filter());
        let weak = self.weak.clone();
        let pop = popover.clone();
        choices.connect_row_activated(move |_, row| {
            if let Some(s) = weak.upgrade() {
                s.apply_language(&pop, id, &row_language(row));
            }
        });
        let (weak, pop, list) = (self.weak.clone(), popover.clone(), choices.clone());
        search.connect_key_press_event(move |_, event| {
            use gdk::keys::constants as k;
            let key = event.keyval();
            if key == k::Return || key == k::KP_Enter || key == k::Down {
                list.invalidate_filter();
                let rows: Vec<gtk::ListBoxRow> = list
                    .children()
                    .into_iter()
                    .filter(|c| c.is_child_visible())
                    .filter_map(|c| c.downcast::<gtk::ListBoxRow>().ok())
                    .collect();
                if let Some(first) = rows.first() {
                    if key == k::Down {
                        list.select_row(Some(first));
                        first.grab_focus();
                    } else if let Some(s) = weak.upgrade() {
                        s.apply_language(&pop, id, &row_language(first));
                    }
                }
                return glib::Propagation::Stop;
            }
            if key == k::Escape {
                pop.popdown();
                if let Some(s) = weak.upgrade() {
                    s.view.grab_focus();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(220);
        scroll.set_max_content_height(220);
        scroll.add(&choices);
        container.pack_start(&scroll, true, true, 0);
        popover.add(&container);
        self.provide(popover.upcast_ref());
        popover.show_all();
        popover.popup();
        search.grab_focus();
        true
    }

    fn apply_language(&self, popover: &gtk::Popover, id: u64, language: &str) {
        let Some(start) = self
            .blocks
            .borrow()
            .find_id(id)
            .map(|i| self.blocks.borrow().spans[i].0)
        else {
            return;
        };
        let mut line_end = self.iter(start);
        if !line_end.ends_line() {
            line_end.forward_to_line_end();
        }
        let first = self
            .buffer
            .text(&self.iter(start), &line_end, true)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let Some(fence) = mdview::_FENCE.captures(&first) else {
            return;
        };
        let a = start + first[..fence.get(1).expect("fence").end()].chars().count() as i32;
        let b = start + first.chars().count() as i32;
        let cursor = self.caret();
        popover.popdown();
        let span = (b - a) as i64;
        let cursor_after = (start as i64).max(cursor as i64 + language.chars().count() as i64 - span);
        self.apply_edit(&blockedit::Edit {
            start: a as usize,
            end: b as usize,
            text: language.to_string(),
            cursor: cursor_after as usize,
        });
        self.view.grab_focus();
    }
}

fn row_language(row: &gtk::ListBoxRow) -> String {
    unsafe {
        row.data::<String>("language")
            .map(|p| p.as_ref().clone())
            .unwrap_or_default()
    }
}

