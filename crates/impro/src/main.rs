use gtk::prelude::*;
use webkit2gtk::WebViewExt;
fn main() {
    gtk::init().expect("GTK initialization");
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro");
    window.set_default_size(1000, 650);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let terminal = impro_vte::Terminal::new();
    terminal.set_vexpand(true);
    let web = webkit2gtk::WebView::new();
    web.set_vexpand(true);
    web.load_html(
        "<!doctype html><title>impro</title><h1>impro toolchain spike</h1>",
        None,
    );
    content.pack_start(&terminal, true, true, 0);
    content.pack_start(&web, true, true, 0);
    window.add(&content);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    window.show_all();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    terminal.spawn_async(
        None,
        &[&shell, "-i"],
        &[],
        glib::SpawnFlags::SEARCH_PATH,
        -1,
        None,
        |result| {
            if let Err(error) = result {
                eprintln!("Shell: {error}");
            }
        },
    );
    gtk::main();
}
