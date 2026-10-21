//! Explorer components; implemented by the owning port task.

pub fn init() {
    impro_ui::register_css("explorer", css);
}

pub fn css(_ui: &impro_core::themes::UiColors) -> String {
    String::new()
}
