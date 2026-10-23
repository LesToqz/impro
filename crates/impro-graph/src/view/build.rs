//! Construction of the view: widget tree, signal wiring, worker start.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use atk::prelude::*;
use gtk::prelude::*;

use super::draw::Painter;
use super::host::Host;
use super::menu::GraphType;
use super::palette::Palette;
use super::panel::Panel;
use super::scene::Scene;
use super::store::Writer;
use super::widget::{Event, FrameStats, GraphSummary, GraphView, Handlers, Inner, ViewOptions};
use crate::prefs;

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

fn pass_through(overlay: &gtk::Overlay, child: &impl IsA<gtk::Widget>) {
    overlay.add_overlay(child);
    overlay.set_overlay_pass_through(child, true);
}

pub(super) fn create(root: &Path, options: ViewOptions) -> GraphView {
    // The panel's colours exist before the first theme call.
    let theme = impro_core::themes::get_theme(None);
    if !super::css::is_installed() {
        super::css::install(&theme.ui, &theme.ui["accent"]);
    }
    let prefs = prefs::normalize(&options.store.load());

    let canvas = gtk::DrawingArea::new();
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);
    canvas.set_can_focus(true);
    canvas.style_context().add_class("impro-graph-canvas");
    canvas.add_events(
        gdk::EventMask::BUTTON_PRESS_MASK
            | gdk::EventMask::BUTTON_RELEASE_MASK
            | gdk::EventMask::POINTER_MOTION_MASK
            | gdk::EventMask::LEAVE_NOTIFY_MASK
            | gdk::EventMask::SCROLL_MASK
            | gdk::EventMask::SMOOTH_SCROLL_MASK
            | gdk::EventMask::KEY_PRESS_MASK
            | gdk::EventMask::KEY_RELEASE_MASK
            | gdk::EventMask::FOCUS_CHANGE_MASK,
    );
    if let Some(a) = canvas.accessible() {
        a.set_role(atk::Role::Canvas);
        a.set_name("Project files and references");
    }
    let status = gtk::Label::new(Some("Mapping this project…"));
    status.set_halign(gtk::Align::Center);
    status.set_valign(gtk::Align::Center);
    status.set_justify(gtk::Justification::Center);
    status.set_line_wrap(true);
    status.style_context().add_class("impro-graph-status");
    if let Some(a) = status.accessible() {
        a.set_role(atk::Role::Statusbar);
    }
    let notice = gtk::Label::new(None);
    notice.set_halign(gtk::Align::Start);
    notice.set_valign(gtk::Align::End);
    notice.set_ellipsize(pango::EllipsizeMode::End);
    notice.style_context().add_class("impro-graph-notice");
    notice.set_no_show_all(true);

    let overlay = gtk::Overlay::new();
    overlay.add(&canvas);
    pass_through(&overlay, &status);
    pass_through(&overlay, &notice);

    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root_box.style_context().add_class("impro-graph");
    root_box.set_hexpand(true);
    root_box.set_vexpand(true);
    root_box.pack_start(&overlay, true, true, 0);

    #[allow(deprecated)]
    let (tx, rx) = glib::MainContext::channel::<Event>(glib::Priority::DEFAULT);
    let build_tx = tx.clone();
    let host = Host::start(move |output| {
        let _ = build_tx.send(Event::Built(Box::new(output)));
    });
    let save_tx = tx;
    let writer = Writer::start(options.store.clone(), move || {
        let _ = save_tx.send(Event::SaveFailed);
    });

    let root = absolute(root);
    let graph_type = GraphType::Global;
    let inner = Rc::new_cyclic(|me: &Weak<Inner>| {
        let weak = me.clone();
        let panel = Panel::new(move |change| {
            if let Some(inner) = weak.upgrade() {
                inner.panel_change(change);
            }
        });
        Inner {
            root_box: root_box.clone(),
            overlay: overlay.clone(),
            canvas: canvas.clone(),
            status: status.clone(),
            notice: notice.clone(),
            panel,
            scene: RefCell::new(Scene::new()),
            painter: RefCell::new(Painter::new()),
            prefs: RefCell::new(prefs),
            root: RefCell::new(root.clone()),
            graph_type: Cell::new(graph_type),
            local_path: RefCell::new(None),
            active_path: RefCell::new(None),
            replay: RefCell::new(None),
            summary: RefCell::new(GraphSummary::default()),
            host_reduce: Cell::new(false),
            font: RefCell::new(None),
            accent: RefCell::new(theme.ui["accent"].clone()),
            ui: RefCell::new(theme.ui.clone()),
            host,
            writer,
            generation: Cell::new(0),
            has_data: Cell::new(false),
            save_failed: Cell::new(false),
            press: RefCell::new(None),
            pointer: Cell::new(None),
            tick: RefCell::new(None),
            last_frame: Cell::new(None),
            menu: RefCell::new(None),
            menu_node: RefCell::new(None),
            build_timer: RefCell::new(None),
            save_timer: RefCell::new(None),
            scale_timer: RefCell::new(None),
            buffer_timers: RefCell::new(HashMap::new()),
            buffer_texts: RefCell::new(HashMap::new()),
            handlers: Handlers::default(),
            closed: Cell::new(false),
            stats: Cell::new(FrameStats::default()),
            me: RefCell::new(me.clone()),
        }
    });

    overlay.add_overlay(&inner.panel.root);
    wire(&inner, rx);
    inner.init_state(options.scan);
    GraphView::from_inner(inner)
}

#[allow(deprecated)]
fn wire(inner: &Rc<Inner>, rx: glib::Receiver<Event>) {
    let weak = Rc::downgrade(inner);
    rx.attach(None, move |event| match weak.upgrade() {
        Some(inner) => {
            inner.on_event(event);
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });

    let canvas = &inner.canvas;
    macro_rules! on {
        ($signal:ident, |$inner:ident $(, $arg:ident)*| $body:expr) => {{
            let weak = Rc::downgrade(inner);
            canvas.$signal(move |_ $(, $arg)*| match weak.upgrade() {
                Some($inner) => $body,
                None => glib::Propagation::Proceed,
            });
        }};
    }
    {
        let weak = Rc::downgrade(inner);
        canvas.connect_draw(move |area, cr| {
            if let Some(inner) = weak.upgrade() {
                inner.draw(area, cr);
            }
            glib::Propagation::Stop
        });
    }
    {
        let weak = Rc::downgrade(inner);
        canvas.connect_size_allocate(move |_, rect| {
            if let Some(inner) = weak.upgrade() {
                inner.resized(rect.width() as f64, rect.height() as f64);
            }
        });
    }
    {
        let weak = Rc::downgrade(inner);
        inner.overlay.connect_size_allocate(move |_, rect| {
            if let Some(inner) = weak.upgrade() {
                inner.panel.set_max_height(rect.height() - 24);
            }
        });
    }
    on!(connect_button_press_event, |i, e| {
        // double and triple presses repeat a press already delivered
        if e.event_type() == gdk::EventType::ButtonPress {
            let (x, y) = e.position();
            i.on_press(x, y, e.button(), e.state(), e.time());
        }
        glib::Propagation::Stop
    });
    on!(connect_button_release_event, |i, e| {
        let (x, y) = e.position();
        i.on_release(x, y, e.button(), e.state(), e.time());
        glib::Propagation::Stop
    });
    on!(connect_motion_notify_event, |i, e| {
        let (x, y) = e.position();
        i.on_motion_event(x, y, e.time(), e.state());
        glib::Propagation::Stop
    });
    on!(connect_grab_broken_event, |i, _e| {
        i.abandon_press();
        glib::Propagation::Proceed
    });
    on!(connect_leave_notify_event, |i, e| {
        i.on_leave(e.mode());
        glib::Propagation::Proceed
    });
    on!(connect_scroll_event, |i, e| {
        let (x, y) = e.position();
        i.on_scroll(x, y, e);
        glib::Propagation::Stop
    });
    on!(connect_key_press_event, |i, e| i.on_key(
        e.keyval(),
        e.state(),
        true
    ));
    on!(connect_key_release_event, |i, e| i.on_key(
        e.keyval(),
        e.state(),
        false
    ));
    on!(connect_focus_out_event, |i, _e| {
        i.on_focus_out();
        glib::Propagation::Proceed
    });
    {
        let weak = Rc::downgrade(inner);
        canvas.connect_map(move |c| {
            if let Some(inner) = weak.upgrade() {
                if c.toplevel()
                    .and_then(|t| t.downcast::<gtk::Window>().ok())
                    .is_some_and(|w| w.focused_widget().is_none())
                {
                    c.grab_focus();
                }
                inner.kick();
            }
        });
        let weak = Rc::downgrade(inner);
        canvas.connect_unmap(move |_| {
            if let Some(inner) = weak.upgrade() {
                inner.pause();
            }
        });
        let weak = Rc::downgrade(inner);
        canvas.connect_scale_factor_notify(move |c| {
            if weak.upgrade().is_some() {
                c.queue_draw();
            }
        });
        let weak = Rc::downgrade(inner);
        inner.root_box.connect_destroy(move |_| {
            if let Some(inner) = weak.upgrade() {
                inner.shutdown();
            }
        });
    }
}

impl Inner {
    pub(super) fn weak(&self) -> Weak<Inner> {
        self.me.borrow().clone()
    }

    /// First state: palette, style, camera zoom, panel, then the first scan.
    fn init_state(self: &Rc<Self>, scan: bool) {
        {
            let ui = self.ui.borrow().clone();
            let accent = self.accent.borrow().clone();
            let mut scene = self.scene.borrow_mut();
            let mut palette =
                Palette::from_theme(&ui, &accent, impro_core::settings::accent_presets());
            palette.font = "sans".to_string();
            scene.palette = palette.clone();
            scene.set_palette(palette);
            let prefs = self.prefs.borrow();
            scene.set_style(self.style_from(&prefs));
            scene.set_forces(prefs.forces());
            let zoom = prefs.scale;
            scene.cam.cam.zoom = zoom;
            scene.cam.target_zoom = zoom;
            scene.restore_camera = (zoom - 1.0).abs() > 1e-9;
        }
        self.panel.set_presets(self.preset_colors());
        let reduce = self.effective_reduce();
        self.panel.set_animated(!reduce);
        let prefs = self.prefs.borrow().clone();
        self.panel.sync(&prefs, false, GraphType::Global);
        self.panel.set_open(!prefs.close);
        self.update_status();
        if scan {
            let root = self.root.borrow().clone();
            self.host.scan(&root);
            self.request_build_now();
        }
    }
}
