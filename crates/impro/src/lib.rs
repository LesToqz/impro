//! Application window and feature composition. All handles live on GTK's thread.
pub mod app;
mod commands;
mod docks;
mod preferences;
pub mod style;
mod views;
mod window;
pub use window::MainWindow;
