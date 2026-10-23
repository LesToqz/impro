//! Paired graph audit driver. Uses public view APIs and existing QA helpers.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gtk::prelude::*;
use impro_graph::view::menu::activatable_labels;
use impro_graph::view::testing as t;
use impro_graph::view::{MemoryStore, ViewOptions};
use impro_graph::{GraphType, GraphView, OpenMode};
use serde_json::{json, Value};

const SLIDERS: [&str; 8] = [
    "localDepth",
    "textFadeMultiplier",
    "nodeSizeMultiplier",
    "lineSizeMultiplier",
    "centerStrength",
    "repelStrength",
    "linkStrength",
    "linkDistance",
];

fn state(view: &GraphView) -> Value {
    let s = t::info(view);
    json!({"zoom":s.zoom,"target_zoom":s.target_zoom,"pan":[s.pan.0,s.pan.1],
        "size":[s.size.0,s.size.1],"nodes":s.nodes,"lines":s.lines,
        "hover":s.hover,"dragging":s.dragging,"simulating":s.simulating,
        "alpha":s.alpha,"velocity":s.camera_velocity,"frames":view.frame_stats().frames,"ticks":s.ticks,"running":t::needs_frames(view)})
}

fn labels(widget: &gtk::Widget, found: &mut Vec<String>) {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        if label.style_context().has_class("impro-graph-name") {
            found.push(label.text().to_string());
        }
    }
    if let Some(container) = widget.downcast_ref::<gtk::Container>() {
        for child in container.children() {
            labels(&child, found);
        }
    }
}

fn snapshot(view: &GraphView) -> Value {
    let toggle_labels = [
        ("localIncoming", "Referenced by"),
        ("localOutgoing", "References to"),
        ("showTags", "Tag markers"),
        ("showAttachments", "Referenced assets"),
        ("hideUnresolved", "Hide missing targets"),
        ("showOrphans", "Unlinked files"),
        ("showCode", "Source and imports"),
        ("showArrow", "Reference direction"),
    ];
    let toggles: BTreeMap<_, _> = toggle_labels
        .iter()
        .filter_map(|(key, label)| view.panel().switch(key).map(|s| (*label, s.is_active())))
        .collect();
    let nodes: BTreeMap<String, Value> = t::node_ids(view)
        .into_iter()
        .map(|id| {
            let (alpha, color) = t::node_look(view, &id).unwrap_or_default();
            let world = t::node_world(view, &id).map(|(x, y)| json!([x, y]));
            let screen = t::node_screen(view, &id).map(|(x, y, r)| json!([x, y, r]));
            (
                id,
                json!({"world":world,"screen":screen,"alpha":alpha,"color":color}),
            )
        })
        .collect();
    let sliders: BTreeMap<&str, Value> = SLIDERS
        .iter()
        .filter_map(|&key| {
            let s = view.panel().scale(key)?;
            let a = s.adjustment();
            let label = s
                .accessible()
                .and_then(|a| a.property::<Option<String>>("accessible-name"));
            Some((
                key,
                json!([label, a.lower(), a.upper(), a.step_increment(), s.value()]),
            ))
        })
        .collect();
    let sections: Vec<Value> = ["filters", "groups", "display", "forces"]
        .iter()
        .map(|id| json!([view.panel().section_title(id), view.panel().collapsed(id)]))
        .collect();
    let mut control_labels = Vec::new();
    labels(view.panel().root.upcast_ref(), &mut control_labels);
    let background = t::background_hex(view);
    let text_color = ["dark", "light", "midnight", "ember"]
        .iter()
        .find_map(|id| {
            let name = format!("impro-{id}");
            let theme = impro_core::themes::get_theme(Some(&name));
            (theme.ui["bg"] == background).then(|| theme.ui["foreground"].clone())
        })
        .unwrap_or_else(|| "#e4edf5".to_string());
    let (status, visible) = view.status_text();
    let label_paint: BTreeMap<_, _> = t::node_ids(view)
        .into_iter()
        .filter_map(|id| {
            t::label_paint(view, &id).map(|p| {
                (
                    id,
                    json!({"font_size":p.font_size,
            "screen_font_size":p.screen_font_size,"wrap_width":p.wrap_width,"alpha":p.alpha,
            "bitmap_font_size":p.bitmap_font_size,"bitmap_size":p.bitmap_size}),
                )
            })
        })
        .collect();
    let line_paint: Vec<_> = t::line_paint(view).into_iter().map(|p| json!({
        "endpoints":p.endpoints,"width":p.width,"color":p.color,"alpha":p.alpha,"emphasis":p.emphasis})).collect();
    let arrow_paint: Vec<_> = t::arrow_paint(view)
        .into_iter()
        .map(|p| {
            json!({
        "endpoints":p.endpoints,"vertices":p.vertices,"alpha":p.alpha})
        })
        .collect();
    json!({"state":state(view),"nodes":nodes,"prefs":view.prefs().to_value(),
        "panel_open":view.panel_open(),"chip_visible":view.panel().chip_visible(),
        "panel_width":view.panel().root.allocated_width(),
        "buttons":{"add":view.panel().add_rule_button().label().map(|s|s.to_string()),
            "reset":view.panel().reset_button().tooltip_text().map(|s|s.to_string()),
            "close":view.panel().close_button().tooltip_text().map(|s|s.to_string()),
            "replay":replay_label(view.panel().root.upcast_ref())},
        "search":[view.panel().search_entry().accessible().and_then(|a|a.property::<Option<String>>("accessible-name")),
            view.panel().search_entry().placeholder_text().map(|s|s.to_string())],
        "cursor_set":view.canvas().window().and_then(|w|w.cursor()).is_some(),"cursor":t::cursor_name(view),
        "paint":{"labels":label_paint,"lines":line_paint,"arrows":arrow_paint},
        "sections":sections,"sliders":sliders,
        "labels":control_labels,"toggles":toggles,"status":if visible {status} else {String::new()},
        "notice":view.notice_text().unwrap_or_default(),"graph_type":view.graph_type().as_str(),
        "local_rows":[view.panel().row_visible("localDepth"),view.panel().row_visible("localIncoming"),
            view.panel().row_visible("localOutgoing"),view.panel().row_visible("showOrphans")],
        "background":background,"text_color":text_color,"accent":t::accent_hex(view),
        "mapped":view.is_mapped(),"frames":view.frame_stats().frames})
}

fn replay_label(widget: &gtk::Widget) -> Option<String> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        if button.tooltip_text().as_deref() == Some("Replay project history") {
            if let Some(label) = button.label() {
                return Some(label.to_string());
            }
        }
    }
    widget
        .downcast_ref::<gtk::Container>()
        .and_then(|c| c.children().iter().find_map(replay_label))
}

fn type_button(widget: &gtk::Widget) -> Option<gtk::Button> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        if button.tooltip_text().as_deref() == Some("Select graph type") && button.is_mapped() {
            return Some(button.clone());
        }
    }
    widget
        .downcast_ref::<gtk::Container>()
        .and_then(|c| c.children().iter().find_map(type_button))
}
fn type_menu(widget: &gtk::Widget) -> Option<gtk::Menu> {
    if let Some(menu) = widget.downcast_ref::<gtk::Menu>() {
        if activatable_labels(menu).iter().any(|s| s == "Global graph") {
            return Some(menu.clone());
        }
    }
    widget
        .downcast_ref::<gtk::Container>()
        .and_then(|c| c.children().iter().find_map(type_menu))
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn num(v: &Value, k: &str, d: f64) -> f64 {
    v[k].as_f64().unwrap_or(d)
}
fn flag(v: &Value, k: &str) -> bool {
    v[k].as_bool().unwrap_or(false)
}
fn settle(view: &GraphView) -> Result<(), Box<dyn Error>> {
    if !t::settle(view, 30_000) {
        return Err("graph did not settle within 30 seconds".into());
    }
    t::pump(60);
    Ok(())
}
fn point(view: &GraphView, id: &str) -> Result<(f64, f64), Box<dyn Error>> {
    let (x, y, _) = t::node_screen(view, id).ok_or_else(|| format!("missing marker {id}"))?;
    Ok((x, y))
}
/// Send a normal GTK canvas event with a real monotonic event timestamp.
/// The zero-time pointer QA helpers cannot measure motion velocity.
fn pointer_event(
    view: &GraphView,
    kind: gdk::EventType,
    x: f64,
    y: f64,
    button: u32,
) -> Result<(), Box<dyn Error>> {
    use glib::translate::*;
    let window = view.canvas().window().ok_or("canvas has no GDK window")?;
    let mut event = gdk::Event::new(kind);
    let timestamp = (glib::monotonic_time() / 1000) as u32;
    // SAFETY: Event::new allocated the matching GDK union member. GDK owns the
    // transferred window reference until the event is freed after dispatch.
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        if kind == gdk::EventType::MotionNotify {
            (*raw).motion.window = window.to_glib_full();
            (*raw).motion.send_event = 1;
            (*raw).motion.time = timestamp;
            (*raw).motion.x = x;
            (*raw).motion.y = y;
            (*raw).motion.state = gdk::ModifierType::BUTTON1_MASK.into_glib();
        } else {
            (*raw).button.window = window.to_glib_full();
            (*raw).button.send_event = 1;
            (*raw).button.time = timestamp;
            (*raw).button.x = x;
            (*raw).button.y = y;
            (*raw).button.button = button;
        }
    }
    if let Some(pointer) = window.display().default_seat().and_then(|s| s.pointer()) {
        event.set_device(Some(&pointer));
    }
    view.canvas().event(&event);
    Ok(())
}

fn shifted_key(view: &GraphView, key: gdk::keys::Key, down: bool) -> Result<(), Box<dyn Error>> {
    use glib::translate::*;
    let window = view.canvas().window().ok_or("canvas has no GDK window")?;
    let kind = if down {
        gdk::EventType::KeyPress
    } else {
        gdk::EventType::KeyRelease
    };
    let mut event = gdk::Event::new(kind);
    // SAFETY: Event::new created a key event and owns the window reference.
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).key.window = window.to_glib_full();
        (*raw).key.send_event = 1;
        (*raw).key.time = (glib::monotonic_time() / 1000) as u32;
        (*raw).key.keyval = *key;
        (*raw).key.state = gdk::ModifierType::SHIFT_MASK.into_glib();
    }
    view.canvas().event(&event);
    Ok(())
}

fn ticks() -> Result<f64, Box<dyn Error>> {
    let stat = fs::read_to_string("/proc/self/stat")?;
    let tail = stat.rsplit_once(')').ok_or("bad process statistics")?.1;
    let cols: Vec<_> = tail.split_whitespace().collect();
    Ok(cols[11].parse::<f64>()? + cols[12].parse::<f64>()?)
}
fn wait_update(updates: &Rc<RefCell<u64>>, before: u64) -> Result<(), Box<dyn Error>> {
    if !t::wait_for(10_000, || *updates.borrow() > before) {
        return Err("graph update deadline exceeded".into());
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: graph_probe ROOT ACTIONS_JSON OUT_DIR".into());
    }
    let root = PathBuf::from(&args[1]);
    let actions: Vec<Value> = serde_json::from_str(&fs::read_to_string(&args[2])?)?;
    let out = Path::new(&args[3]);
    fs::create_dir_all(out)?;
    gtk::init()?;
    impro_ui::init();
    impro_graph::init();
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_font_name(Some("DejaVu Sans 10"));
        settings.set_gtk_cursor_blink(false);
    }
    let store = Arc::new(MemoryStore::new(json!({})));
    let view = GraphView::with_options(
        &root,
        ViewOptions {
            store: store.clone(),
            scan: true,
        },
    );
    let loading_status = view.status_text();
    let theme = impro_core::themes::get_theme(Some("impro-dark"));
    let _ = impro_ui::apply_style(Some(theme));
    view.set_theme(&theme.ui, &theme.ui["accent"]);
    view.set_font(Some("DejaVu Sans"));
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let e = events.clone();
    let r = root.clone();
    view.connect_open_file(move |p,m|e.borrow_mut().push(json!({"type":"open",
        "id":p.strip_prefix(&r).unwrap_or(p).to_string_lossy(),"mode":match m {
            OpenMode::Current=>"current",OpenMode::NewTab=>"new-tab",OpenMode::RightPane=>"right-pane"}})));
    let e = events.clone();
    view.connect_search_tag(move |tag| {
        e.borrow_mut()
            .push(json!({"type":"search","query":format!("tag:#{tag}")}))
    });
    let e = events.clone();
    view.connect_create_note(move |name| e.borrow_mut().push(json!({"type":"create","name":name})));
    let e = events.clone();
    let r = root.clone();
    view.connect_reveal(move |p| {
        e.borrow_mut()
            .push(json!({"type":"reveal","id":p.strip_prefix(&r).unwrap_or(p).to_string_lossy()}))
    });
    let e = events.clone();
    let r = root.clone();
    view.connect_copy_path(move |p| {
        e.borrow_mut()
            .push(json!({"type":"copy","id":p.strip_prefix(&r).unwrap_or(p).to_string_lossy()}))
    });
    let errors = Rc::new(RefCell::new(Vec::<String>::new()));
    let e = errors.clone();
    view.connect_error(move |s| e.borrow_mut().push(s.to_string()));
    let updates = Rc::new(RefCell::new(0u64));
    let u = updates.clone();
    view.connect_updated(move |_| *u.borrow_mut() += 1);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro graph audit");
    window.set_default_size(1000, 700);
    window.add(&*view);
    window.show_all();
    window.move_(0, 0);
    let start = Instant::now();
    wait_update(&updates, 0)?;
    settle(&view)?;
    let initial_settle_ms = start.elapsed().as_secs_f64() * 1000.0;
    let deliveries = Rc::new(RefCell::new(Vec::<Value>::new()));
    let delivery_view = view.clone();
    let d = deliveries.clone();
    view.connect_updated(move |_| d.borrow_mut().push(snapshot(&delivery_view)));
    let mut steps = serde_json::Map::new();
    for action in actions {
        let op = text(&action, "op");
        let name = text(&action, "name");
        eprintln!("audit {op} {name}");
        let before = snapshot(&view);
        let event_start = events.borrow().len();
        let update_start = *updates.borrow();
        let started = Instant::now();
        deliveries.borrow_mut().clear();
        let mut extra = json!({});
        let none = gdk::ModifierType::empty();
        match op {
            "capture" => {}
            "prefs" => {
                let mut p = view.prefs().to_value();
                for (k, v) in action.as_object().ok_or("invalid action")? {
                    if k != "name" && k != "op" {
                        p[k] = v.clone();
                    }
                }
                view.apply_prefs(&p);
                t::pump(200);
                settle(&view)?;
            }
            "theme" => {
                let id = format!("impro-{}", text(&action, "theme"));
                let th = impro_core::themes::get_theme(Some(&id));
                let _ = impro_ui::apply_style(Some(th));
                view.set_theme(&th.ui, &th.ui["accent"]);
                settle(&view)?;
            }
            "focus" => {
                view.focus_file(&root.join(text(&action, "id")));
                settle(&view)?;
            }
            "accent" => {
                let th = impro_core::themes::get_theme(Some("impro-dark"));
                view.set_theme(&th.ui, text(&action, "color"));
                settle(&view)?;
            }
            "font" => {
                view.set_font(Some(text(&action, "family")));
                settle(&view)?;
            }
            "active" => {
                let path = action["id"].as_str().map(|id| root.join(id));
                view.set_active_file(path.as_deref());
                settle(&view)?;
            }
            "hover" => {
                let (x, y) = point(&view, text(&action, "id"))?;
                t::motion(&view, x, y);
                settle(&view)?;
            }
            "leave" => {
                t::leave(&view);
                settle(&view)?;
            }
            "click" => {
                let (x, y) = point(&view, text(&action, "id"))?;
                let button = num(&action, "button", 1.0) as u32;
                let modifiers = if flag(&action, "ctrl") {
                    gdk::ModifierType::CONTROL_MASK
                } else {
                    none
                };
                t::press(&view, x, y, button, modifiers);
                let dx = num(&action, "move", 0.0);
                if dx != 0.0 {
                    t::motion(&view, x + dx, y);
                }
                t::release(&view, x + dx, y, button, modifiers);
                settle(&view)?;
            }
            "menu" => {
                t::open_context_menu(&view, text(&action, "id"));
                t::pump(80);
                extra["menu"] = json!(t::context_menu(&view).map(|m| activatable_labels(&m)));
                if let Some(m) = t::context_menu(&view) {
                    m.popdown();
                }
                t::key(&view, gdk::keys::constants::Escape, true);
            }
            "menu-actions" => {
                for index in 0..5 {
                    t::open_context_menu(&view, text(&action, "id"));
                    let menu = t::context_menu(&view).ok_or("file menu did not open")?;
                    let item = menu
                        .children()
                        .into_iter()
                        .filter_map(|w| w.downcast::<gtk::MenuItem>().ok())
                        .filter(|w| w.is_sensitive() && !w.is::<gtk::SeparatorMenuItem>())
                        .nth(index)
                        .ok_or("file menu item missing")?;
                    item.activate();
                }
                t::pump(100);
            }
            "drag" => {
                let (x, y) = point(&view, text(&action, "id"))?;
                t::press(&view, x, y, 1, none);
                t::motion(&view, x + 60.0, y + 40.0);
                t::pump(120);
                extra["held"] = snapshot(&view);
                t::release(&view, x + 60.0, y + 40.0, 1, none);
                settle(&view)?;
            }
            "pan" => {
                t::leave(&view);
                pointer_event(&view, gdk::EventType::ButtonPress, 2.0, 2.0, 1)?;
                for i in 1..=8 {
                    pointer_event(
                        &view,
                        gdk::EventType::MotionNotify,
                        2.0 + 10.0 * f64::from(i),
                        2.0 + 4.0 * f64::from(i),
                        1,
                    )?;
                    t::pump(16);
                }
                extra["held"] = state(&view);
                pointer_event(&view, gdk::EventType::ButtonRelease, 82.0, 34.0, 1)?;
                t::pump(120);
                extra["after120"] = state(&view);
                settle(&view)?;
            }
            "stop" => {
                pointer_event(&view, gdk::EventType::ButtonPress, 2.0, 2.0, 1)?;
                pointer_event(&view, gdk::EventType::MotionNotify, 62.0, 2.0, 1)?;
                pointer_event(&view, gdk::EventType::ButtonRelease, 62.0, 2.0, 1)?;
                t::pump(32);
                pointer_event(&view, gdk::EventType::ButtonPress, 2.0, 2.0, 1)?;
                extra["pressed"] = state(&view);
                t::pump(200);
                extra["later"] = state(&view);
                pointer_event(&view, gdk::EventType::ButtonRelease, 2.0, 2.0, 1)?;
                settle(&view)?;
            }
            "controlled-motion" => {
                t::leave(&view);
                t::press_at(&view, 2.0, 2.0, 1, none, 1000);
                for i in 1..=8 {
                    t::motion_at(
                        &view,
                        2.0 + 10.0 * f64::from(i),
                        2.0 + 4.0 * f64::from(i),
                        1000 + 16 * i,
                    );
                    t::advance_frame(&view, 1.0 / 60.0);
                }
                extra["grabbing_cursor"] = json!(t::cursor_name(&view));
                t::release_at(&view, 82.0, 34.0, 1, none, 1144);
                extra["fling_velocity"] = json!(t::info(&view).camera_velocity);
                let before = t::info(&view).camera_velocity.0;
                t::advance_frame(&view, 1.0 / 60.0);
                extra["pan_decay"] = json!(t::info(&view).camera_velocity.0 / before);
                settle(&view)?;
                let before = t::info(&view).zoom;
                t::wheel(&view, 200.0, 150.0, -120.0, false);
                t::advance_frame(&view, 1.0 / 60.0);
                let after = t::info(&view);
                extra["zoom_ease"] = json!((after.zoom - before) / (after.target_zoom - before));
                settle(&view)?;
            }
            "wheel" => {
                t::wheel(
                    &view,
                    200.0,
                    150.0,
                    num(&action, "delta", 0.0),
                    flag(&action, "ctrl"),
                );
                extra["immediate"] = state(&view);
                t::pump(40);
                extra["at40"] = state(&view);
                settle(&view)?;
            }
            "camera-reset" => {
                view.reset_camera();
                settle(&view)?;
            }
            "key" => {
                let key = match text(&action, "key") {
                    "ArrowLeft" => gdk::keys::constants::Left,
                    "ArrowRight" => gdk::keys::constants::Right,
                    "ArrowUp" => gdk::keys::constants::Up,
                    "ArrowDown" => gdk::keys::constants::Down,
                    "+" => gdk::keys::constants::plus,
                    "-" => gdk::keys::constants::minus,
                    "Enter" => gdk::keys::constants::Return,
                    "Escape" => gdk::keys::constants::Escape,
                    _ => return Err("unknown key".into()),
                };
                if flag(&action, "shift") {
                    shifted_key(&view, key, true)?;
                } else {
                    t::key(&view, key, true);
                }
                t::pump(200);
                if flag(&action, "shift") {
                    shifted_key(&view, key, false)?;
                    t::key(&view, gdk::keys::constants::Shift_L, false);
                } else {
                    t::key(&view, key, false);
                }
                settle(&view)?;
            }
            "search" => {
                view.panel().search_entry().set_text(text(&action, "text"));
                t::pump(350);
                settle(&view)?;
            }
            "local" => {
                let path = root.join(text(&action, "id"));
                view.set_active_file(Some(&path));
                view.open_local_with_depth(&path, num(&action, "depth", 2.0) as u32);
                wait_update(&updates, update_start)?;
                settle(&view)?;
            }
            "global" => {
                view.open_global();
                view.set_active_file(None);
                t::pump(200);
                settle(&view)?;
            }
            "local-empty" => {
                view.set_active_file(None);
                view.set_graph_type(GraphType::Local);
                wait_update(&updates, update_start)?;
                settle(&view)?;
            }
            "note" => {
                view.set_graph_type(GraphType::Note);
                wait_update(&updates, update_start)?;
                settle(&view)?;
            }
            "sections" => {
                for id in ["filters", "groups", "display", "forces"] {
                    view.panel().click_section(id);
                }
                t::pump(180);
            }
            "section" => {
                view.panel().click_section(text(&action, "id"));
                t::pump(180);
            }
            "type-menu" => {
                if let Some(button) = type_button(view.panel().root.upcast_ref()) {
                    button.clicked();
                    t::pump(100);
                    let menu = gtk::Window::list_toplevels().iter().find_map(type_menu);
                    extra["menu"] = json!(menu.as_ref().map(activatable_labels));
                    if let Some(menu) = menu {
                        menu.popdown();
                    }
                    view.panel().close_popovers();
                }
            }
            "group-add" => {
                view.panel().add_rule_button().clicked();
                t::pump(200);
                settle(&view)?;
            }
            "group-remove" => {
                let mut p = view.prefs().to_value();
                p["colorGroups"] = json!([]);
                view.apply_prefs(&p);
                t::pump(200);
                settle(&view)?;
            }
            "reset" => {
                view.panel().reset_button().clicked();
                t::pump(200);
                settle(&view)?;
            }
            "replay" => {
                view.replay_history();
                t::pump(250);
                extra["at250"] = snapshot(&view);
                t::pump(750);
                extra["at1000"] = snapshot(&view);
                settle(&view)?;
            }
            "buffer" => {
                view.buffer_changed(
                    &root.join(text(&action, "id")),
                    Some(text(&action, "text").to_string()),
                );
                t::pump(200);
                extra["at200"] = snapshot(&view);
                t::pump(400);
                extra["at600"] = snapshot(&view);
                settle(&view)?;
            }
            "buffer-clear" => {
                view.buffer_changed(&root.join(text(&action, "id")), None);
                t::pump(700);
                settle(&view)?;
            }
            "save" => {
                let path = root.join("saved.md");
                fs::write(&path, "[[hub/Hub]]\n")?;
                view.file_saved(&path);
                wait_update(&updates, update_start)?;
                settle(&view)?;
            }
            "rename" => {
                let from = root.join("saved.md");
                let to = root.join("renamed.md");
                fs::rename(&from, &to)?;
                view.path_renamed(&from, &to);
                wait_update(&updates, update_start)?;
                settle(&view)?;
            }
            "delete" => {
                let path = root.join("renamed.md");
                fs::remove_file(&path)?;
                view.path_deleted(&path);
                wait_update(&updates, update_start)?;
                settle(&view)?;
            }
            "resize" => {
                window.resize(800, 600);
                t::pump(250);
                settle(&view)?;
            }
            "hidden" => {
                t::wheel(&view, 200.0, 150.0, -120.0, false);
                window.hide();
                t::pump(100);
                extra["at0"] = state(&view);
                t::pump(1000);
                extra["at1000"] = state(&view);
                window.show_all();
                t::pump(200);
                settle(&view)?;
            }
            "idle" => {
                let cpu = ticks()?;
                let start = Instant::now();
                let idle_loop = glib::MainLoop::new(None, false);
                let stop = idle_loop.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(2000), move || {
                    stop.quit()
                });
                idle_loop.run();
                extra["cpu_ticks"] = json!(ticks()? - cpu);
                extra["wall_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
            }
            "cap" => {
                view.set_root(&root.with_file_name("cap-fixture"));
                wait_update(&updates, update_start)?;
                settle(&view)?;
            }
            "root-restore" => {
                view.set_root(&root);
                wait_update(&updates, update_start)?;
                settle(&view)?;
            }
            "persist" => {
                view.set_panel_open(false);
                view.panel().search_entry().set_text("path:hub");
                t::pump(800);
                view.flush_prefs();
                t::pump(100);
                extra["saved"] = json!(store.saves().last());
                let restored = GraphView::with_options(
                    &root,
                    ViewOptions {
                        store: store.clone(),
                        scan: false,
                    },
                );
                extra["restored"] = restored.prefs().to_value();
                restored.shutdown();
            }
            _ => return Err(format!("unknown operation {op}").into()),
        }
        if !name.is_empty() {
            let mut s = snapshot(&view);
            if name == "default" {
                let mut times = serde_json::Map::new();
                for name in ["hub/Hub.md", ".impro/Notes/First.md"] {
                    let created = fs::metadata(root.join(name))
                        .ok()
                        .and_then(|m| m.created().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|t| t.as_secs_f64());
                    times.insert(name.to_string(), json!(created));
                }
                extra["filesystem_created"] = json!(times);
            }
            if matches!(op, "save" | "rename" | "delete" | "buffer") {
                extra["deliveries"] = json!(*deliveries.borrow());
            }
            s["before"] = before;
            s["extra"] = extra;
            s["events"] = json!(&events.borrow()[event_start..]);
            s["elapsed_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            if flag(&action, "png") {
                t::save_png(&view, &out.join(format!("{name}.png")))?;
            }
            steps.insert(name.to_string(), s);
            fs::write(
                out.join("measurements.json"),
                serde_json::to_vec_pretty(&json!({"steps":steps,
                "errors":*errors.borrow(),"initial_settle_ms":initial_settle_ms,"loading_status":loading_status.0}))?,
            )?;
        }
    }
    view.shutdown();
    window.close();
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("graph audit error: {e}");
        std::process::exit(1);
    }
}
