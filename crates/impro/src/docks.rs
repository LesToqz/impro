use crate::MainWindow;
use gtk::prelude::*;
use impro_terminal::TerminalPanel;
use std::time::Instant;

pub(crate) struct SavedLayout {
    panel: TerminalPanel,
    widgets: Vec<(gtk::Widget, bool, bool)>,
    positions: [i32; 3],
    pending: [bool; 2],
}

impl MainWindow {
    pub(crate) fn panels(&self) -> [TerminalPanel; 2] {
        [self.terminal_bottom.clone(), self.terminal_right.clone()]
    }
    pub(crate) fn connect_docks(&self) {
        for panel in self.panels() {
            let weak = self.weak();
            panel.connect_last_closed(move |panel| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.hide_dock(panel);
                }
            });
            let weak = self.weak();
            panel.connect_hide_requested(move |panel| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.hide_dock(panel);
                }
            });
            let weak = self.weak();
            panel.connect_feature_disabled(move |_, kind| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.require_feature(kind);
                }
            });
            let weak = self.weak();
            panel.connect_maximize_toggled(move |panel, maximized| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.maximize_changed(panel, maximized);
                }
            });
            let weak = self.weak();
            let p = panel.downgrade();
            panel.add_button().connect_choose(move |kind| {
                if let (Some(this), Some(panel)) = (weak.upgrade().map(Self), p.upgrade()) {
                    this.add_dock_pane(&panel, kind);
                }
            });
            let weak = self.weak();
            panel.connect_visible_notify(move |_| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.sync_titlebar();
                }
            });
        }
        for (right, paned, panel) in [
            (false, &self.dock_bottom, &self.terminal_bottom),
            (true, &self.dock, &self.terminal_right),
        ] {
            let weak = self.weak();
            paned.connect_local("size-allocate", true, move |_| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.schedule_dock_size(right);
                }
                None
            });
            let weak = self.weak();
            panel.connect_show(move |_| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.schedule_dock_size(right);
                }
            });
        }
        let weak = self.weak();
        self.explorer.connect_visible_notify(move |_| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.sync_titlebar();
            }
        });
        for (signal, action) in [
            (
                "settings-requested",
                Self::open_settings_default as fn(&Self),
            ),
            ("dock-toggle-requested", Self::toggle_right_dock),
            ("bottom-dock-toggle-requested", Self::toggle_bottom_dock),
            ("explorer-toggle-requested", Self::toggle_explorer),
        ] {
            let callback = self.callback(action);
            self.titlebar.connect_request(signal, move |_| callback());
        }
        self.sync_titlebar();
    }
    fn open_settings_default(&self) {
        self.open_settings(None);
    }
    fn sync_titlebar(&self) {
        self.titlebar
            .set_dock_active(self.terminal_right.get_visible());
        self.titlebar
            .set_bottom_dock_active(self.terminal_bottom.get_visible());
        self.titlebar
            .set_explorer_active(self.explorer.get_visible());
    }
    pub(crate) fn set_visible(widget: &impl IsA<gtk::Widget>, visible: bool) {
        widget.set_no_show_all(!visible);
        if visible {
            widget.show_all();
        } else {
            widget.hide();
        }
    }
    pub fn hide_dock(&self, panel: &TerminalPanel) {
        self.restore_terminal_fullscreen();
        Self::set_visible(panel, false);
    }
    pub fn show_dock(&self, panel: &TerminalPanel) {
        Self::set_visible(panel, true);
        if panel.pane_count() == 0 {
            if let Some(kind) = ["terminal", "graph", "gitgraph"]
                .into_iter()
                .find(|kind| self.feature_enabled(kind))
            {
                self.add_dock_pane(panel, kind);
            } else {
                self.hide_dock(panel);
                self.notify("All dock features are turned off in Settings.", "info");
                return;
            }
        }
        panel.focus_terminal();
    }
    pub fn toggle_dock(&self, panel: &TerminalPanel) {
        self.restore_terminal_fullscreen();
        if panel.get_visible() {
            self.hide_dock(panel);
        } else {
            self.show_dock(panel);
        }
    }
    pub fn toggle_bottom_dock(&self) {
        self.toggle_dock(&self.terminal_bottom);
    }
    pub fn toggle_terminal_bottom(&self) {
        if self.require_feature("terminal") {
            self.toggle_bottom_dock();
        }
    }
    pub fn toggle_terminal_right(&self) {
        if self.require_feature("terminal") {
            self.toggle_dock(&self.terminal_right);
        }
    }
    pub fn toggle_right_dock(&self) {
        self.restore_terminal_fullscreen();
        if self.terminal_right.get_visible() {
            self.hide_dock(&self.terminal_right);
        } else if self.terminal_right.pane_count() == 0 && self.feature_enabled("graph") {
            self.open_graph();
        } else {
            self.show_dock(&self.terminal_right);
        }
    }
    pub fn split_terminal(&self) {
        if !self.require_feature("terminal") {
            return;
        }
        let panel = if self.focus_in(&self.terminal_right) {
            &self.terminal_right
        } else {
            &self.terminal_bottom
        };
        self.restore_terminal_fullscreen();
        let empty = panel.pane_count() == 0;
        self.show_dock(panel);
        if !empty {
            panel.new_terminal();
        }
    }
    pub fn toggle_explorer(&self) {
        if !self.require_feature("explorer") {
            return;
        }
        self.restore_terminal_fullscreen();
        Self::set_visible(&self.explorer, !self.explorer.get_visible());
        self.explorer_was_visible.set(self.explorer.get_visible());
    }
    pub(crate) fn focus_in(&self, widget: &impl IsA<gtk::Widget>) -> bool {
        GtkWindowExt::focused_widget(&self.window)
            .is_some_and(|focus| focus == *widget.as_ref() || focus.is_ancestor(widget))
    }
    pub fn terminal_focused(&self) -> bool {
        GtkWindowExt::focused_widget(&self.window)
            .is_some_and(|focus| focus.is::<impro_vte::Terminal>())
    }
    pub fn is_terminal_fullscreen(&self) -> bool {
        self.fullscreen.borrow().is_some()
    }
    pub fn restore_terminal_fullscreen(&self) {
        let panel = self
            .fullscreen
            .borrow()
            .as_ref()
            .map(|saved| saved.panel.clone());
        if let Some(panel) = panel {
            panel.restore();
        }
    }
    fn maximize_changed(&self, panel: &TerminalPanel, maximized: bool) {
        if !maximized {
            if !self
                .fullscreen
                .borrow()
                .as_ref()
                .is_some_and(|s| s.panel == *panel)
            {
                return;
            }
            let saved = self.fullscreen.borrow_mut().take();
            if let Some(saved) = saved {
                for (widget, visible, no_show) in saved.widgets {
                    widget.set_no_show_all(no_show);
                    widget.set_visible(visible);
                }
                for (paned, position) in [&self.sidebar, &self.dock, &self.dock_bottom]
                    .into_iter()
                    .zip(saved.positions)
                {
                    paned.set_position(position);
                }
                for (pending, value) in self.size_pending.iter().zip(saved.pending) {
                    pending.set(value);
                }
            }
            return;
        }
        self.restore_terminal_fullscreen();
        let widgets: Vec<gtk::Widget> = vec![
            self.explorer.clone().upcast(),
            self.editor.clone().upcast(),
            self.dock_bottom.clone().upcast(),
            self.terminal_bottom.clone().upcast(),
            self.terminal_right.clone().upcast(),
        ];
        let saved = SavedLayout {
            panel: panel.clone(),
            widgets: widgets
                .into_iter()
                .map(|w| {
                    let visible = w.get_visible();
                    let no_show = w.is_no_show_all();
                    (w, visible, no_show)
                })
                .collect(),
            positions: [
                self.sidebar.position(),
                self.dock.position(),
                self.dock_bottom.position(),
            ],
            pending: [self.size_pending[0].get(), self.size_pending[1].get()],
        };
        *self.fullscreen.borrow_mut() = Some(saved);
        Self::set_visible(&self.explorer, false);
        if panel == &self.terminal_bottom {
            Self::set_visible(&self.editor, false);
            Self::set_visible(&self.terminal_right, false);
        } else {
            Self::set_visible(&self.dock_bottom, false);
        }
    }
    pub fn toggle_terminal_fullscreen(&self) {
        if !self.require_feature("terminal") {
            return;
        }
        if self.is_terminal_fullscreen() {
            self.restore_terminal_fullscreen();
            return;
        }
        let panels = self.panels();
        let panel = panels
            .iter()
            .find(|p| p.get_visible() && self.focus_in(*p))
            .or_else(|| panels.iter().find(|p| p.get_visible()))
            .unwrap_or(&self.terminal_bottom);
        if !panel.get_visible() {
            self.show_dock(panel);
        }
        if panel.pane_count() > 0 {
            panel.toggle_maximize();
        }
    }
    fn schedule_dock_size(&self, right: bool) {
        let i = usize::from(right);
        if !self.size_pending[i].get()
            || self.dock_size_idle[i].borrow().is_some()
            || self.closed.get()
        {
            return;
        }
        let weak = self.weak();
        let source = glib::idle_add_local_once(move || {
            if let Some(this) = weak.upgrade().map(Self) {
                this.dock_size_idle[i].borrow_mut().take();
                this.size_terminal(right);
            }
        });
        *self.dock_size_idle[i].borrow_mut() = Some(source);
    }
    fn size_terminal(&self, right: bool) {
        let started = Instant::now();
        let i = usize::from(right);
        let (dock, panel) = if right {
            (&self.dock, &self.terminal_right)
        } else {
            (&self.dock_bottom, &self.terminal_bottom)
        };
        if self.is_terminal_fullscreen() || !self.size_pending[i].get() || !panel.get_visible() {
            return;
        }
        let extent = if right {
            dock.allocated_width()
        } else {
            dock.allocated_height()
        };
        if extent <= 1 {
            return;
        }
        let first = dock
            .child1()
            .map(|w| {
                if right {
                    w.allocated_width()
                } else {
                    w.allocated_height()
                }
            })
            .unwrap_or(0);
        let second = if right {
            panel.allocated_width()
        } else {
            panel.allocated_height()
        };
        let handle = (extent - first - second).max(0);
        let available = extent - handle;
        let size = if right {
            ((extent as f64 * 0.4).round() as i32)
                .max(380)
                .min((extent as f64 * 0.6) as i32)
                .min(available - 300)
        } else {
            ((extent as f64 * 0.3).round() as i32)
                .max(150)
                .min(available - 150)
        }
        .max(0);
        self.size_pending[i].set(second != size);
        panel.set_initial_dock_extent(right, size);
        let position = (available - size).max(0);
        if dock.position() != position {
            dock.set_position(position);
        }
        self.allocation_us
            .set(self.allocation_us.get().max(started.elapsed().as_micros()));
    }
    pub fn layout_ready(&self) -> bool {
        self.panels().iter().enumerate().all(|(i, p)| {
            !p.get_visible()
                || (!self.size_pending[i].get() && self.dock_size_idle[i].borrow().is_none())
        })
    }
    pub fn max_allocation_us(&self) -> u128 {
        self.allocation_us.get()
    }

    pub(crate) fn sync_features(&self, name: &str) {
        self.restore_terminal_fullscreen();
        let kinds: Vec<_> = ["terminal", "graph", "gitgraph"]
            .into_iter()
            .filter(|kind| self.feature_enabled(kind))
            .collect();
        for panel in self.panels() {
            panel.set_pane_kinds(&kinds);
            if ["terminal", "graph", "gitgraph"].contains(&name) {
                let key = (panel == self.terminal_right, name.to_string());
                if !self.feature_enabled(name) {
                    self.disabled
                        .borrow_mut()
                        .entry(key)
                        .or_insert_with(|| panel.count_kind(name));
                    panel.close_kind(name);
                } else {
                    let count = self.disabled.borrow_mut().remove(&key).unwrap_or(0);
                    if count > 0 {
                        Self::set_visible(&panel, true);
                    }
                    for _ in 0..count {
                        self.add_dock_pane(&panel, name);
                    }
                }
            }
        }
        if name == "explorer" {
            Self::set_visible(
                &self.explorer,
                self.feature_enabled("explorer") && self.explorer_was_visible.get(),
            );
        }
        self.explorer
            .set_notes_enabled(self.feature_enabled("notes"));
        self.sync_note_toolbars();
        Self::set_visible(
            &self.titlebar.explorer_button(),
            self.feature_enabled("explorer"),
        );
        Self::set_visible(&self.titlebar.dock_button(), !kinds.is_empty());
        Self::set_visible(&self.titlebar.bottom_dock_button(), !kinds.is_empty());
    }
}
