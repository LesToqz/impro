//! Editor components; implemented by the owning port task.

pub fn init() {
    impro_ui::register_css("editor", css);
}

pub fn css(_ui: &impro_core::themes::UiColors) -> String {
    String::new()
}
