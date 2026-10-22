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
        impro_chrome::init();
        impro_editor::init();
        impro_explorer::init();
        impro_terminal::init();
        impro_graph::init();
        impro_gitviews::init();
        impro_settings_ui::init();
        impro_markdown::init();
    });
}
