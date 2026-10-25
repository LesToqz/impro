//! Register feature CSS once, before installing the shared theme provider.
use std::cell::Cell;

thread_local! { static INITIALIZED: Cell<bool> = const { Cell::new(false) }; }

pub fn init() {
    INITIALIZED.with(|initialized| {
        if initialized.replace(true) {
            return;
        }
        impro_core::init();
        impro_ui::init();
        // Raw primary-entry pixels already contain the symbolic foreground.
        // GTK's regular-image dim effect must not be applied a second time.
        impro_ui::register_css("app.startup-icons", |_| {
            ".impro-startup-raster image.left { -gtk-icon-effect: none; }".into()
        });
        impro_chrome::init();
        impro_editor::init();
        impro_explorer::init();
        impro_terminal::init();
        impro_graph::init();
        impro_gitviews::init();
        impro_settings_ui::init();
        impro_markdown::init();
        // Registered after feature CSS; user font overrides are appended later
        // by apply_theme and retain precedence over these Windows defaults.
        #[cfg(windows)]
        impro_ui::register_css("zz.windows-fonts", |_| {
            "* { font-family: \"Segoe UI\"; }\n\
             .impro-editor textview, .impro-editor textview text { \
             font-family: \"Cascadia Mono\", \"Consolas\", \"Courier New\", monospace; }\n"
                .into()
        });
    });
}
