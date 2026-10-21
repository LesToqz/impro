//! Terminal docks and pane chrome for impro.
pub mod panesplit;
pub mod resizegate;
pub mod terminal_bar;
pub fn init(){impro_ui::register_css("terminal.bar",css);}
pub fn css(ui:&impro_core::themes::UiColors)->String{terminal_bar::css(ui)}
