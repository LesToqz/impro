//! Pointer and keyboard handling of the canvas.

use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use super::camera::Keys;
use super::menu::{self, GraphType};
use super::widget::{Inner, OpenMode, Press};
use crate::geometry::{wheel_pixels, WheelUnit, CLICK_SLOP};
use crate::model::NodeKind;

/// Pixels one wheel notch is worth (what the browser engine reports).
const NOTCH_PIXELS: f64 = 53.0;

/// The modifier bit that tells a pointer button is held.
fn button_mask(button: u32) -> gdk::ModifierType {
    match button {
        1 => gdk::ModifierType::BUTTON1_MASK,
        2 => gdk::ModifierType::BUTTON2_MASK,
        3 => gdk::ModifierType::BUTTON3_MASK,
        4 => gdk::ModifierType::BUTTON4_MASK,
        _ => gdk::ModifierType::BUTTON5_MASK,
    }
}

impl Inner {
    fn node_at(&self, x: f64, y: f64) -> Option<(usize, Arc<str>)> {
        let scene = self.scene.borrow();
        let i = scene.pick(x, y, 0.0)?;
        Some((i, scene.graph.nodes[i].id.clone()))
    }

    pub(super) fn on_press(
        self: &Rc<Self>,
        x: f64,
        y: f64,
        button: u32,
        state: gdk::ModifierType,
        time: u32,
    ) {
        self.close_menu();
        self.panel.close_popovers();
        self.canvas.grab_focus();
        self.pointer.set(Some((x, y)));
        self.scene.borrow_mut().restore_camera = false;
        if button == 3 {
            if let Some((i, _)) = self.node_at(x, y) {
                self.context_for(i, x, y);
            }
            // the menu's file stays highlighted: its fade needs frames
            self.kick();
            return;
        }
        // a press whose release never arrived (its button is not held any
        // more) must not swallow this one
        let stale = match self.press.borrow().as_ref() {
            Some(Press::Node { button: b, .. }) => !state.contains(button_mask(*b)),
            Some(Press::Pan { .. }) => {
                !state.intersects(gdk::ModifierType::BUTTON1_MASK | gdk::ModifierType::BUTTON2_MASK)
            }
            None => false,
        };
        if stale {
            self.abandon_press();
        }
        if self.press.borrow().is_some() || !(1..=2).contains(&button) {
            return;
        }
        let hit = self.node_at(x, y);
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let mut scene = self.scene.borrow_mut();
        scene.cam.stop_tracking();
        let press = match hit {
            Some((i, id)) => {
                if button == 1 {
                    scene.press_node(i);
                }
                Press::Node {
                    id,
                    button,
                    x0: x,
                    y0: y,
                    click: true,
                    ctrl,
                }
            }
            None => {
                scene.cam.begin_pan();
                Press::Pan {
                    x0: x,
                    y0: y,
                    pan0: (scene.cam.cam.pan_x, scene.cam.cam.pan_y),
                    last_time: time,
                    last_move: time,
                }
            }
        };
        drop(scene);
        *self.press.borrow_mut() = Some(press);
        self.update_cursor();
        self.kick();
    }

    /// A motion event with its button state: a press whose button is no
    /// longer held lost its release (a grab took it, the window lost the
    /// pointer) and ends here instead of dragging along with the bare pointer.
    pub(super) fn on_motion_event(
        self: &Rc<Self>,
        x: f64,
        y: f64,
        time: u32,
        state: gdk::ModifierType,
    ) {
        let held = match self.press.borrow().as_ref() {
            Some(Press::Node { button, .. }) => state.contains(button_mask(*button)),
            Some(Press::Pan { .. }) => {
                state.intersects(gdk::ModifierType::BUTTON1_MASK | gdk::ModifierType::BUTTON2_MASK)
            }
            None => true,
        };
        if !held {
            self.abandon_press();
        }
        self.on_motion(x, y, time);
    }

    /// Ends the current press without activating, dragging on or flinging.
    pub(super) fn abandon_press(self: &Rc<Self>) {
        let press = self.press.borrow_mut().take();
        match press {
            Some(Press::Node { .. }) => self.scene.borrow_mut().end_drag(),
            Some(Press::Pan { .. }) => self.scene.borrow_mut().cam.end_pan(f64::INFINITY, true),
            None => return,
        }
        self.update_cursor();
        self.kick();
    }

    pub(super) fn on_motion(self: &Rc<Self>, x: f64, y: f64, time: u32) {
        self.pointer.set(Some((x, y)));
        enum Next {
            Hover,
            None,
        }
        let next = {
            let mut press = self.press.borrow_mut();
            match press.as_mut() {
                None => Next::Hover,
                Some(Press::Node {
                    id, x0, y0, click, ..
                }) => {
                    let (dx, dy) = (x - *x0, y - *y0);
                    let slop = CLICK_SLOP as f64;
                    if dx * dx + dy * dy > slop * slop {
                        *click = false;
                    }
                    if !*click {
                        let mut scene = self.scene.borrow_mut();
                        if let Some(i) = scene.graph.index_of(id).filter(|i| scene.drag == Some(*i))
                        {
                            let (wx, wy) = scene.cam.cam.to_world(x, y);
                            scene.drag_to(i, wx as f32, wy as f32);
                        }
                    }
                    Next::None
                }
                Some(Press::Pan {
                    x0,
                    y0,
                    pan0,
                    last_time,
                    last_move,
                }) => {
                    let mut scene = self.scene.borrow_mut();
                    let elapsed = time.wrapping_sub(*last_time) as f64;
                    scene
                        .cam
                        .drag_pan(pan0.0 + (x - *x0), pan0.1 + (y - *y0), elapsed);
                    *last_time = time;
                    *last_move = time;
                    Next::None
                }
            }
        };
        if matches!(next, Next::Hover) {
            self.scene.borrow_mut().update_hover(x, y);
        }
        self.update_cursor();
        self.kick();
    }

    pub(super) fn on_release(
        self: &Rc<Self>,
        _x: f64,
        _y: f64,
        button: u32,
        state: gdk::ModifierType,
        time: u32,
    ) {
        let matches = match self.press.borrow().as_ref() {
            Some(Press::Node { button: b, .. }) => *b == button,
            Some(Press::Pan { .. }) => button == 1 || button == 2,
            None => false,
        };
        if !matches {
            return;
        }
        let press = self.press.borrow_mut().take();
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        match press {
            Some(Press::Node {
                id,
                button,
                click,
                ctrl: press_ctrl,
                ..
            }) => {
                self.scene.borrow_mut().end_drag();
                if click && (button == 1 || button == 2) {
                    self.activate(&id, button == 2 || ctrl || press_ctrl);
                }
            }
            Some(Press::Pan { last_move, .. }) => {
                let idle = time.wrapping_sub(last_move) as f64;
                let reduce = self.effective_reduce();
                self.scene.borrow_mut().cam.end_pan(idle, reduce);
            }
            None => {}
        }
        if let Some((x, y)) = self.pointer.get() {
            self.scene.borrow_mut().update_hover(x, y);
        }
        self.update_cursor();
        self.kick();
    }

    /// The pointer left the canvas. Grabs (a popup menu opening) do not count.
    pub(super) fn on_leave(self: &Rc<Self>, mode: gdk::CrossingMode) {
        if mode != gdk::CrossingMode::Normal {
            return;
        }
        self.pointer.set(None);
        if self.press.borrow().is_none() {
            self.scene.borrow_mut().set_hover(None);
            self.update_cursor();
            self.kick();
        }
    }

    pub(super) fn on_focus_out(&self) {
        self.scene.borrow_mut().cam.keys = Keys::default();
    }

    pub(super) fn on_scroll(self: &Rc<Self>, x: f64, y: f64, event: &gdk::EventScroll) {
        self.close_menu();
        let (_, dy) = event.delta();
        let delta = match event.direction() {
            gdk::ScrollDirection::Up => -NOTCH_PIXELS,
            gdk::ScrollDirection::Down => NOTCH_PIXELS,
            gdk::ScrollDirection::Smooth => dy * NOTCH_PIXELS,
            _ => return,
        };
        let ctrl = event.state().contains(gdk::ModifierType::CONTROL_MASK);
        self.wheel(x, y, delta, ctrl);
    }

    /// A wheel movement of `pixels` (positive: zoom out).
    pub(super) fn wheel(self: &Rc<Self>, x: f64, y: f64, pixels: f64, ctrl: bool) {
        let px = wheel_pixels(pixels, WheelUnit::Pixel, ctrl);
        if px == 0.0 {
            return;
        }
        {
            let mut scene = self.scene.borrow_mut();
            scene.restore_camera = false;
            scene.cam.wheel(x, y, px);
        }
        self.kick();
    }

    pub(super) fn on_key(
        self: &Rc<Self>,
        key: gdk::keys::Key,
        state: gdk::ModifierType,
        down: bool,
    ) -> glib::Propagation {
        use gdk::keys::constants as k;
        if state.intersects(
            gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::MOD1_MASK
                | gdk::ModifierType::SUPER_MASK,
        ) {
            return glib::Propagation::Proceed;
        }
        let fast = state.contains(gdk::ModifierType::SHIFT_MASK);
        let mut handled = true;
        {
            let mut scene = self.scene.borrow_mut();
            let keys = &mut scene.cam.keys;
            if down {
                keys.fast = fast;
            }
            if key == k::Left {
                keys.left = down;
            } else if key == k::Right {
                keys.right = down;
            } else if key == k::Up {
                keys.up = down;
            } else if key == k::Down {
                keys.down = down;
            } else if key == k::equal || key == k::plus || key == k::KP_Add {
                keys.zoom_in = down;
            } else if key == k::minus || key == k::underscore || key == k::KP_Subtract {
                keys.zoom_out = down;
            } else if key == k::Shift_L || key == k::Shift_R {
                keys.fast = down;
                handled = false;
            } else {
                handled = false;
            }
            if handled && down {
                scene.restore_camera = false;
                scene.cam.stop_tracking();
            }
        }
        if handled {
            self.kick();
            return glib::Propagation::Stop;
        }
        if !down {
            return glib::Propagation::Proceed;
        }
        if key == k::Return || key == k::KP_Enter {
            let hover = {
                let scene = self.scene.borrow();
                scene.hover.map(|i| scene.graph.nodes[i].id.clone())
            };
            if let Some(id) = hover {
                self.activate(&id, false);
                return glib::Propagation::Stop;
            }
        } else if key == k::Escape {
            if self.menu.borrow().is_some() {
                self.close_menu();
            } else if self.panel.is_open() {
                self.set_panel_open(false, true);
            } else {
                self.scene.borrow_mut().set_hover(None);
                self.kick();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    /// Pointer cursor over markers, a closed hand while dragging.
    pub(super) fn update_cursor(&self) {
        let name = self.cursor_name();
        if let Some(window) = self.canvas.window() {
            let cursor = (name != "default")
                .then(|| gdk::Cursor::from_name(&window.display(), name))
                .flatten();
            window.set_cursor(cursor.as_ref());
        }
    }

    pub(super) fn cursor_name(&self) -> &'static str {
        let scene = self.scene.borrow();
        let grabbing =
            scene.is_dragging() || matches!(self.press.borrow().as_ref(), Some(Press::Pan { .. }));
        if grabbing {
            "grabbing"
        } else if scene.hover.is_some() {
            "pointer"
        } else {
            "default"
        }
    }

    /// Click on a marker: files open, tags search, missing targets are created.
    pub(super) fn activate(self: &Rc<Self>, id: &Arc<str>, new_tab: bool) {
        let (kind, label) = {
            let scene = self.scene.borrow();
            match scene.graph.index_of(id) {
                Some(i) => {
                    let n = &scene.graph.nodes[i];
                    (n.kind, n.label().to_string())
                }
                None => return,
            }
        };
        match kind {
            NodeKind::Tag => {
                for f in self.handlers.search_tag.borrow().clone() {
                    f(&label);
                }
            }
            NodeKind::Unresolved => {
                for f in self.handlers.create_note.borrow().clone() {
                    f(&label);
                }
            }
            _ => {
                let path = self.abs_path_of(id);
                let mode = if new_tab {
                    OpenMode::NewTab
                } else {
                    OpenMode::Current
                };
                self.emit_open(&path, mode);
            }
        }
    }

    fn context_for(self: &Rc<Self>, i: usize, x: f64, y: f64) {
        let (kind, id, label) = {
            let scene = self.scene.borrow();
            let n = &scene.graph.nodes[i];
            (n.kind, n.id.clone(), n.label().to_string())
        };
        match kind {
            NodeKind::Tag => self.activate(&id, false),
            NodeKind::Unresolved => {}
            _ => {
                self.scene.borrow_mut().set_hover(Some(i));
                self.show_context_menu(&id, &label, x, y);
            }
        }
    }

    /// Opens the context menu of a file marker at a canvas position.
    pub(super) fn show_context_menu(self: &Rc<Self>, id: &Arc<str>, title: &str, x: f64, y: f64) {
        self.close_menu();
        *self.menu_node.borrow_mut() = Some(id.clone());
        let weak = self.weak();
        let menu = menu::context_menu(title, move |action| {
            if let Some(inner) = weak.upgrade() {
                inner.context_action(action);
            }
        });
        menu.set_attach_widget(Some(&self.canvas));
        if let Some(window) = self.canvas.window() {
            let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
            let trigger = gtk::current_event();
            menu.popup_at_rect(
                &window,
                &rect,
                gdk::Gravity::NorthWest,
                gdk::Gravity::NorthWest,
                trigger.as_ref(),
            );
        }
        let weak = self.weak();
        menu.connect_deactivate(move |m| {
            if let Some(inner) = weak.upgrade() {
                // dismissed by a click elsewhere: forget it
                let current = inner.menu.borrow().as_ref() == Some(m);
                if current {
                    inner.menu.borrow_mut().take();
                }
                inner.canvas.grab_focus();
            }
        });
        *self.menu.borrow_mut() = Some(menu);
    }

    pub(super) fn close_menu(&self) {
        // popping down emits signals: release the slot first
        let menu = self.menu.borrow_mut().take();
        if let Some(m) = menu {
            m.popdown();
        }
    }

    /// Graph type used when the panel is closed and the chip is shown.
    #[allow(dead_code)]
    pub(super) fn current_type(&self) -> GraphType {
        self.graph_type.get()
    }
}
