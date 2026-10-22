//! Compare GTK alone and styled chrome with the full workspace startup trace.
use gtk::prelude::*;
use std::{cell::Cell, path::PathBuf, rc::Rc, time::Instant};

fn main() -> glib::ExitCode {
    let started = Instant::now();
    let chrome = std::env::args().nth(1).as_deref() == Some("chrome");
    let output = std::env::var_os("IMPRO_QA_SCREENSHOT")
        .map(PathBuf::from)
        .expect("IMPRO_QA_SCREENSHOT is required");
    let app = gtk::Application::new(
        Some("dev.impro.StartupFloor"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.connect_startup(move |_| {
        eprintln!(
            "gtk-startup-ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
    });
    let failed = Rc::new(Cell::new(false));
    let capture_failed = failed.clone();
    app.connect_activate(move |app| {
        if chrome {
            impro::style::init();
            impro_ui::apply_style(None).expect("valid composed CSS");
        }
        let window = gtk::ApplicationWindow::new(app);
        window.set_default_size(1200, 750);
        if chrome {
            let titlebar = impro_chrome::TitleBar::new(&gtk::MenuBar::new());
            window.set_titlebar(Some(&titlebar));
        }
        impro::app::install_screenshot_helper(
            &window,
            output.clone(),
            started,
            capture_failed.clone(),
        );
        eprintln!("built-ms={:.3}", started.elapsed().as_secs_f64() * 1000.0);
        window.show_all();
        eprintln!("shown-ms={:.3}", started.elapsed().as_secs_f64() * 1000.0);
        window.present();
    });
    let code = app.run_with_args::<&str>(&[]);
    if failed.get() {
        glib::ExitCode::FAILURE
    } else {
        code
    }
}
