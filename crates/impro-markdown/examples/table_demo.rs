//! Standalone table host: `cargo run -p impro-markdown --example table_demo`.
//! Optional arguments: theme id, screenshot path. Screenshots exit automatically.
use gtk::prelude::*;
use impro_markdown::livetable::{TableBlock, TableHost, Tables};
use std::{cell::Cell, rc::Rc, time::Duration};
const SAMPLE:&str="| Name | Detail | Count |\n| :--- | :---: | ---: |\n| **Alpha** | `code` | 12 |\n| Beta | line<br>two | 3 |\n| Gamma | escaped \\| pipe | 4 |\n";
struct Host {
    view: gtk::TextView,
    slot: gtk::Box,
    silent: Cell<bool>,
}
impl TableHost for Host {
    fn view(&self) -> gtk::TextView {
        self.view.clone()
    }
    fn silent(&self, enter: bool) {
        self.silent.set(enter);
    }
    fn replacement(&self, _block: &TableBlock, widget: &gtk::Widget, _height: i32) {
        self.slot.pack_start(widget, false, false, 0);
        widget.show_all();
    }
    fn retag(&self, _block: &TableBlock) {}
    fn queue_layout(&self) {}
    fn activate_link(&self, uri: &str) {
        eprintln!("link: {uri}");
    }
}
fn main() {
    gtk::init().expect("GTK display");
    impro_ui::init();
    impro_markdown::init();
    let args: Vec<_> = std::env::args().collect();
    let theme = impro_core::themes::get_theme(args.get(1).map(String::as_str));
    impro_ui::style::apply_style(Some(theme)).expect("valid theme");
    let slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    slot.set_margin_start(30);
    slot.set_margin_end(30);
    slot.set_margin_top(30);
    slot.set_margin_bottom(30);
    let main = gtk::TextView::new();
    main.buffer().expect("buffer").set_text(SAMPLE);
    let font = if main
        .pango_context()
        .list_families()
        .iter()
        .any(|f| f.name() == "Inter")
    {
        "Inter"
    } else {
        "sans-serif"
    };
    let mut ui = theme.ui.clone();
    ui.insert(
        "foreground".into(),
        theme.syntax["operator"].foreground.clone(),
    );
    let provider = gtk::CssProvider::new();
    provider
        .load_from_data(
            impro_markdown::livetable::styled_css(&ui, &theme.chrome["gutter_bg"], 11, font)
                .as_bytes(),
        )
        .expect("table CSS");
    gtk::StyleContext::add_provider_for_screen(
        &gdk::Screen::default().expect("screen"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_USER + 2,
    );
    let host = Rc::new(Host {
        view: main,
        slot: slot.clone(),
        silent: Cell::new(false),
    });
    let tables = Tables::new(host.clone());
    let buffer = host.view.buffer().expect("buffer");
    let table = tables
        .build(TableBlock::new(1, &buffer, 0, buffer.char_count(), SAMPLE))
        .expect("table");
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro");
    window.set_default_size(850, 400);
    window.add(&slot);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    window.show_all();
    let grid = table.clone();
    glib::idle_add_local(move || {
        if grid.fill_slice() {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
    if let Some(path) = args.get(2).cloned() {
        let window = window.clone();
        glib::timeout_add_local_once(Duration::from_millis(200), move || {
            window
                .window()
                .expect("surface")
                .pixbuf(0, 0, window.allocated_width(), window.allocated_height())
                .expect("pixels")
                .savev(path, "png", &[])
                .expect("PNG");
            gtk::main_quit();
        });
    }
    gtk::main();
    tables.destroy();
    unsafe {
        window.destroy();
    }
}
