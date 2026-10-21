//! Interactive chrome demo; arguments: mode, theme, optional screenshot PNG.
use gtk::prelude::*;
use impro_chrome::*;
use std::path::PathBuf;
fn main() {
    gtk::init().expect("GTK display");
    init();
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map_or("titlebar", String::as_str);
    let theme = args.get(2).map_or("impro-dark", String::as_str);
    impro_ui::apply_style(Some(impro_core::themes::get_theme(Some(theme)))).expect("theme CSS");
    impro_ui::motion::set_reduce_motion(true);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro");
    window.set_default_size(900, 600);
    let menus = gtk::MenuBar::new();
    for name in ["File", "View"] {
        let item = gtk::MenuItem::with_label(name);
        let menu = gtk::Menu::new();
        menu.append(&gtk::MenuItem::with_label(if name == "File" {
            "Quick Switcher…"
        } else {
            "Command Palette…"
        }));
        item.set_submenu(Some(&menu));
        menus.append(&item);
    }
    let bar = TitleBar::new(&menus);
    window.set_titlebar(Some(&bar));
    let text = gtk::TextView::new();
    let host = OverlayHost::new(Some(text.upcast_ref()));
    window.add(host.widget());
    window.show_all();
    let palette = CommandPalette::new(|| {
        vec![
            Command::new("File: Save", || {}, vec!["Ctrl+S".into()]),
            Command::new("View: Toggle explorer", || {}, vec!["Ctrl+B".into()]),
            Command::new("Editor: Toggle comment", || {}, vec!["Ctrl+/".into()]),
        ]
    });
    let switcher = QuickSwitcher::new("/r");
    switcher.set_files(
        [
            "/r/src/b.py",
            "/r/a.md",
            "/r/Zeta.md",
            "/r/src/plan/Plan.md",
            "/elsewhere/x.md",
        ]
        .map(PathBuf::from)
        .to_vec(),
        vec!["/r/Zeta.md".into()],
    );
    match mode {
        "palette" => {
            host.show_prompt(&palette);
            palette.set_query("tog");
        }
        "switcher" => {
            host.show_prompt(&switcher);
            switcher.set_query("pla");
        }
        "notices" => {
            for level in overlay::NOTICE_LEVELS {
                host.notify(level, 0, level).expect("notice");
            }
        }
        _ => {}
    }
    let h = host.clone();
    let p = palette.clone();
    let s = switcher.clone();
    window.connect_key_press_event(move |_, event| {
        if event.state().contains(gdk::ModifierType::CONTROL_MASK) {
            if event.keyval() == gdk::keys::constants::p {
                h.show_prompt(&p);
                return glib::Propagation::Stop;
            }
            if event.keyval() == gdk::keys::constants::o {
                h.show_prompt(&s);
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    if let Some(path) = args.get(3).cloned() {
        let w = window.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            if let Some(native) = w.window() {
                let (_, _, width, height) = native.geometry();
                let pixbuf = native.pixbuf(0, 0, width, height).expect("window capture");
                pixbuf.savev(path, "png", &[]).expect("PNG capture");
            }
            gtk::main_quit();
        });
    }
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    gtk::main();
}
