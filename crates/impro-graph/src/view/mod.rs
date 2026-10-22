//! The native Graph view: scene model, software renderer and GTK widgets.

mod build;
pub mod camera;
mod css;
pub mod draw;
mod flow;
pub mod host;
pub mod icons;
mod input;
pub mod menu;
pub mod palette;
pub mod panel;
pub mod raster;
pub mod scene;
pub mod store;
pub mod switch;
pub mod testing;
pub mod timelapse;
mod widget;

pub use css::{css, install as install_css};
pub use menu::{ContextAction, GraphType, CONTEXT_ITEMS};
pub use store::{MemoryStore, PrefsStore, SettingsStore};
pub use widget::{tag_query, FrameStats, GraphSummary, GraphView, OpenMode, ViewOptions};
