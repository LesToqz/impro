//! Pointer, clipboard and key handling.
use super::{widgets::Tools, Inner, Kind, Role};
use crate::{
    blockedit::{self, Edit},
    mdview::{self, LinkTarget},
};
use gtk::prelude::*;
use std::rc::Rc;

impl Inner {
    fn tools_of(widget: &gtk::Widget) -> Option<Rc<Tools>> {
        unsafe {
            widget
                .data::<Rc<Tools>>("impro-live-tools")
                .map(|t| t.as_ref().clone())
        }
    }

    /// The block on the line at buffer `y`. Line lookup only walks the text
    /// tree; it never hit-tests a layout.
    fn block_at(&self, y: i32) -> Option<usize> {
        if self.blocks.borrow().len() == 0 {
            return None;
        }
        let (iter, _) = self.view.line_at_y(y);
        Some(self.blocks.borrow().index(iter.offset()))
    }

    pub(super) fn link_at(&self, x: f64, y: f64) -> Option<(String, bool)> {
        let (bx, by) = self
            .view
            .window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        let index = self.block_at(by)?;
        if self.blocks.borrow().items[index].links.is_empty() {
            return None;
        }
        let iter = self.view.iter_at_location(bx, by)?;
        let blocks = self.blocks.borrow();
        let relative = iter.offset() - blocks.spans[index].0;
        blocks.items[index]
            .links
            .iter()
            .find(|l| l.a <= relative && relative < l.b)
            .map(|l| (l.target.clone(), l.revealed))
    }

    pub(super) fn press(&self, event: &gdk::EventButton) -> bool {
        if !self.enabled.get() || event.button() != 1 {
            return false;
        }
        let (x, y) = event.position();
        let link = self.link_at(x, y);
        self.press_with(link, event.state().contains(gdk::ModifierType::CONTROL_MASK))
    }

    pub(super) fn press_with(&self, link: Option<(String, bool)>, ctrl: bool) -> bool {
        if let Some((target, revealed)) = link {
            if !revealed || ctrl {
                return self.activate_link(&target);
            }
        }
        !self.view.is_editable()
    }

    pub(super) fn motion(&self, x: f64, y: f64) {
        if !self.enabled.get() || !(self.hot.get() > 0 || self.pointer.get()) {
            return;
        }
        let by = self
            .view
            .window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32)
            .1;
        let tools = self.block_at(by).and_then(|index| {
            self.blocks.borrow().items[index]
                .widgets
                .iter()
                .find(|s| s.role == Role::Tools)
                .map(|s| s.widget.clone())
        });
        let current = self.hover_tools.borrow().clone();
        if tools != current {
            *self.hover_tools.borrow_mut() = tools.clone();
            if let Some(previous) = current.as_ref().and_then(Self::tools_of) {
                self.reveal_copy(&previous, false);
            }
            if let Some(next) = tools.as_ref().and_then(Self::tools_of) {
                self.reveal_copy(&next, true);
            }
        }
        let link = self.link_at(x, y);
        if let Some(window) = TextViewExt::window(&self.view, gtk::TextWindowType::Text) {
            if link.is_some() || self.pointer.get() {
                let name = if link.is_some() { "pointer" } else { "text" };
                let cursor = gdk::Cursor::from_name(&window.display(), name);
                window.set_cursor(cursor.as_ref());
            }
        }
        self.pointer.set(link.is_some());
    }

    pub(super) fn leave(&self, inferior: bool) {
        if inferior {
            return;
        }
        let tools = self.hover_tools.borrow_mut().take();
        if let Some(tools) = tools.as_ref().and_then(Self::tools_of) {
            self.reveal_copy(&tools, false);
        }
    }

    /// Open a link target: local files through the host, web links externally.
    pub(super) fn activate_link(&self, target: &str) -> bool {
        let source = (self.source_path)().unwrap_or_default();
        match mdview::link_target(target, &source) {
            None => false,
            Some(LinkTarget::File(path)) => {
                (self.open_file)(&path);
                true
            }
            Some(LinkTarget::Uri(uri)) => {
                let opener = self.uri_opener.borrow().clone();
                if let Some(opener) = opener {
                    opener(&uri);
                    return true;
                }
                let top = self.view.toplevel().and_then(|t| t.downcast::<gtk::Window>().ok());
                gtk::show_uri_on_window(top.as_ref(), &uri, 0).is_ok()
            }
        }
    }

    pub(super) fn control_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as k;
        // This handler also receives Ctrl+Enter, before the panel's shortcuts.
        let key = event.keyval();
        if event.state().contains(gdk::ModifierType::CONTROL_MASK)
            && (key == k::Return || key == k::KP_Enter)
        {
            return self.edit_key(event, false, 4);
        }
        false
    }

    pub(super) fn copy_selection(&self, view: &gtk::TextView) {
        if self.enabled.get() && view.is_editable() && self.buffer.has_selection() {
            self.copy_selection_text();
            view.stop_signal_emission_by_name("copy-clipboard");
        }
    }

    pub(super) fn cut_selection(&self, view: &gtk::TextView) {
        if self.enabled.get() && view.is_editable() && self.buffer.has_selection() {
            self.copy_selection_text();
            self.buffer.begin_user_action();
            self.buffer.delete_selection(true, true);
            self.buffer.end_user_action();
            view.stop_signal_emission_by_name("cut-clipboard");
        }
    }

    fn copy_selection_text(&self) {
        let Some((a, b)) = self.buffer.selection_bounds() else {
            return;
        };
        let text = self.buffer.text(&a, &b, true).map(|s| s.to_string()).unwrap_or_default();
        if let Some(display) = gdk::Display::default() {
            if let Some(clipboard) = gtk::Clipboard::default(&display) {
                clipboard.set_text(&text);
            }
        }
    }

    /// Apply a block edit as one undoable action.
    pub(super) fn apply_edit(&self, edit: &Edit) -> bool {
        self.editing.set(true);
        self.buffer.begin_user_action();
        if edit.start != edit.end {
            let (mut a, mut b) = (self.iter(edit.start as i32), self.iter(edit.end as i32));
            self.buffer.delete(&mut a, &mut b);
        }
        if !edit.text.is_empty() {
            let mut at = self.iter(edit.start as i32);
            self.buffer.insert(&mut at, &edit.text);
        }
        self.buffer.place_cursor(&self.iter(edit.cursor as i32));
        self.editing.set(false);
        self.clamp_cursor();
        self.buffer.end_user_action();
        true
    }

    pub(super) fn edit_key(&self, event: &gdk::EventKey, indent_tabs: bool, indent_width: i32) -> bool {
        use gdk::keys::constants as k;
        if !self.enabled.get() || !self.view.is_editable() {
            return false;
        }
        let state = event.state();
        if state.intersects(gdk::ModifierType::MOD1_MASK | gdk::ModifierType::SUPER_MASK) {
            return false;
        }
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        let key = event.keyval();
        let enter = key == k::Return || key == k::KP_Enter;
        if ctrl && !enter {
            return false;
        }
        let cursor = self.caret();
        let mut first = self.iter(cursor);
        first.set_line_offset(0);
        let mut last = first.clone();
        if !last.ends_line() {
            last.forward_to_line_end();
        }
        let line = self
            .buffer
            .text(&first, &last, true)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let block = {
            let blocks = self.blocks.borrow();
            (blocks.len() > 0).then(|| {
                let index = blocks.index(cursor);
                let deco = &blocks.items[index];
                (deco.kind, deco.closed, deco.text.clone(), blocks.spans[index].0)
            })
        };
        let in_code = matches!(&block, Some((Kind::Codeblock, ..)));
        if let Some((Kind::Codeblock, true, text, start)) = &block {
            if !ctrl && !self.buffer.has_selection() {
                let lines: Vec<&str> = text.split_inclusive('\n').collect();
                if lines.len() == 2 {
                    let content = start + lines[0].chars().count() as i32;
                    let character = key.to_unicode();
                    let mut value = character.filter(|c| !c.is_control()).map(String::from);
                    if key == k::Tab && !shift {
                        value = Some(if indent_tabs {
                            "\t".into()
                        } else {
                            " ".repeat(indent_width.max(0) as usize)
                        });
                    }
                    if let Some(value) = value {
                        if cursor <= content {
                            let n = value.chars().count();
                            return self.apply_edit(&Edit {
                                start: content as usize,
                                end: content as usize,
                                text: format!("{value}\n"),
                                cursor: content as usize + n,
                            });
                        }
                    }
                }
            }
        }
        let prefix = if in_code { None } else { blockedit::prefix(&line) };
        let content = first.offset() + prefix.as_ref().map_or(0, |p| p.end as i32);
        if key == k::Home && prefix.is_some() {
            return self.move_cursor(content, shift);
        }
        if self.buffer.has_selection() {
            return self.indent_selection(event, indent_tabs, indent_width);
        }
        if key == k::Left && prefix.is_some() && cursor == content {
            return self.move_cursor((first.offset() - 1).max(0), shift);
        }
        if matches!(&block, Some((Kind::Table, ..))) {
            return false;
        }
        let action = if enter {
            if ctrl {
                "toggle"
            } else {
                "enter"
            }
        } else if key == k::Tab {
            if shift {
                "outdent"
            } else {
                "tab"
            }
        } else if key == k::ISO_Left_Tab {
            "outdent"
        } else if key == k::BackSpace {
            "backspace"
        } else if key == k::Delete {
            "delete"
        } else if key == k::Down {
            "down"
        } else if key == k::Up {
            "up"
        } else if key == k::Escape {
            "exit"
        } else if key == k::space {
            "space"
        } else {
            return false;
        };
        let text = self
            .buffer
            .text(&self.buffer.start_iter(), &self.buffer.end_iter(), true)
            .map(|s| s.to_string())
            .unwrap_or_default();
        match blockedit::transform(
            &text,
            cursor as usize,
            action,
            indent_width.max(0) as usize,
            indent_tabs,
        ) {
            Some(edit) => self.apply_edit(&edit),
            None => false,
        }
    }

    fn move_cursor(&self, offset: i32, extend: bool) -> bool {
        let iter = self.iter(offset);
        if extend {
            self.buffer.move_mark(&self.insert_mark, &iter);
        } else {
            self.buffer.place_cursor(&iter);
        }
        true
    }

    fn indent_selection(&self, event: &gdk::EventKey, tabs: bool, width: i32) -> bool {
        use gdk::keys::constants as k;
        let key = event.keyval();
        if key != k::Tab && key != k::ISO_Left_Tab {
            return false;
        }
        let Some((mut first, mut last)) = self.buffer.selection_bounds() else {
            return false;
        };
        first.set_line_offset(0);
        if last.line_offset() == 0 {
            last.backward_char();
        }
        if !last.ends_line() {
            last.forward_to_line_end();
        }
        let text = self
            .buffer
            .text(&first, &last, true)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let lines: Vec<&str> = text.split('\n').collect();
        if !lines
            .iter()
            .all(|l| l.trim().is_empty() || super::model::LIST.is_match(l))
        {
            return false;
        }
        let outdent =
            key == k::ISO_Left_Tab || event.state().contains(gdk::ModifierType::SHIFT_MASK);
        let width = width.max(0) as usize;
        let replace = |line: &str| -> String {
            if line.trim().is_empty() {
                return line.to_string();
            }
            if outdent {
                let spaces = line.len() - line.trim_start_matches(' ').len();
                let amount = if line.starts_with('\t') { 1 } else { width.min(spaces) };
                return line[amount..].to_string();
            }
            format!("{}{}", if tabs { "\t".into() } else { " ".repeat(width) }, line)
        };
        let replacement = lines.iter().map(|l| replace(l)).collect::<Vec<_>>().join("\n");
        let offset = first.offset();
        self.buffer.begin_user_action();
        let (mut a, mut b) = (first, last);
        self.buffer.delete(&mut a, &mut b);
        let mut at = self.iter(offset);
        self.buffer.insert(&mut at, &replacement);
        let end = offset + replacement.chars().count() as i32;
        self.buffer.select_range(&self.iter(end), &self.iter(offset));
        self.buffer.end_user_action();
        true
    }
}
