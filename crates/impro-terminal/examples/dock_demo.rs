//! Both dock orientations. Set IMPRO_QA_SCREENSHOT to capture and exit.
use gtk::prelude::*;
use impro_terminal::TerminalPanel;
use std::time::Duration;
fn main() {
    gtk::init().expect("GTK display");
    impro_terminal::init();
    impro_ui::apply_style(None).expect("theme CSS");
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro");
    window.set_default_size(1200, 800);
    let outer = gtk::Paned::new(gtk::Orientation::Horizontal);
    let left = gtk::Paned::new(gtk::Orientation::Vertical);
    let editor = gtk::Box::new(gtk::Orientation::Vertical, 0);
    editor.style_context().add_class("background");
    let label = gtk::Label::new(Some("impro"));
    editor.pack_start(&label, true, true, 0);
    let bottom = TerminalPanel::new("/tmp", false, false, 11);
    let right = TerminalPanel::new("/tmp", false, true, 11);
    for panel in [&bottom, &right] {
        #[cfg(unix)]
        panel.set_shell("/bin/bash");
        panel.apply_theme(impro_core::themes::get_theme(None));
        for _ in 0..2 {
            panel.new_terminal();
        }
    }
    left.pack1(&editor, true, false);
    left.pack2(&bottom, false, false);
    left.set_position(470);
    outer.pack1(&left, true, false);
    outer.pack2(&right, false, false);
    outer.set_position(800);
    window.add(&outer);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    window.show_all();
    if let Ok(path) = std::env::var("IMPRO_QA_SCREENSHOT") {
        let window = window.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(800), move || {
            if let Some(window) = window.upgrade() {
                if let Some(native) = window.window() {
                    if let Some(p) =
                        native.pixbuf(0, 0, window.allocated_width(), window.allocated_height())
                    {
                        p.savev(path, "png", &[]).expect("write screenshot");
                    }
                }
                unsafe {
                    window.destroy();
                }
            }
            gtk::main_quit();
        });
    }
    gtk::main();
}
