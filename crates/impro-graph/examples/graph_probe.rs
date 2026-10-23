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
        "alpha":s.alpha,"ticks":s.ticks,"running":t::needs_frames(view)})
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
    let (status, visible) = view.status_text();
    json!({"state":state(view),"nodes":nodes,"prefs":view.prefs().to_value(),
        "panel_open":view.panel_open(),"sections":sections,"sliders":sliders,
        "labels":control_labels,"status":if visible {status} else {String::new()},
        "notice":view.notice_text().unwrap_or_default(),"graph_type":view.graph_type().as_str(),
        "local_rows":[view.panel().row_visible("localDepth"),view.panel().row_visible("localIncoming"),
            view.panel().row_visible("localOutgoing"),view.panel().row_visible("showOrphans")],
        "background":t::background_hex(view),"accent":t::accent_hex(view),
        "mapped":view.is_mapped(),"frames":view.frame_stats().frames})
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
    if !t::settle(view, 12_000) {
        return Err("graph did not settle within 12 seconds".into());
    }
    t::pump(60);
    Ok(())
}
fn point(view: &GraphView, id: &str) -> Result<(f64, f64), Box<dyn Error>> {
    let (x, y, _) = t::node_screen(view, id).ok_or_else(|| format!("missing marker {id}"))?;
    Ok((x, y))
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
    let mut steps = serde_json::Map::new();
    for action in actions {
        let op = text(&action, "op");
        let name = text(&action, "name");
        eprintln!("audit {op} {name}");
        let before = snapshot(&view);
        let event_start = events.borrow().len();
        let update_start = *updates.borrow();
        let started = Instant::now();
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
                t::press(&view, 2.0, 2.0, 1, none);
                for i in 1..=8 {
                    t::motion(&view, 2.0 + 10.0 * f64::from(i), 2.0 + 4.0 * f64::from(i));
                    t::pump(16);
                }
                extra["held"] = state(&view);
                t::release(&view, 82.0, 34.0, 1, none);
                t::pump(120);
                extra["after120"] = state(&view);
                settle(&view)?;
            }
            "stop" => {
                t::press(&view, 2.0, 2.0, 1, none);
                t::motion(&view, 62.0, 2.0);
                t::release(&view, 62.0, 2.0, 1, none);
                t::pump(32);
                t::press(&view, 2.0, 2.0, 1, none);
                extra["pressed"] = state(&view);
                t::pump(200);
                extra["later"] = state(&view);
                t::release(&view, 2.0, 2.0, 1, none);
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
                    t::key(&view, gdk::keys::constants::Shift_L, true);
                }
                t::key(&view, key, true);
                t::pump(200);
                t::key(&view, key, false);
                if flag(&action, "shift") {
                    t::key(&view, gdk::keys::constants::Shift_L, false);
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
                t::pump(2000);
                extra["cpu_ticks"] = json!(ticks()? - cpu);
                extra["wall_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
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
                "errors":*errors.borrow(),"initial_settle_ms":initial_settle_ms}))?,
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
