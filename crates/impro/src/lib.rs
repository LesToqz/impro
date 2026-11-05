//! Application window and feature composition. All handles live on GTK's thread.
pub mod app;
pub mod app_icon;
pub mod cli;
mod commands;
mod docks;
pub mod launch;
mod preferences;
mod startup_icons;
pub mod style;
mod views;
pub mod welcome;
pub mod win;
mod window;
pub use commands::accel_text;
pub use views::DiffHighlighter;
pub use window::MainWindow;
