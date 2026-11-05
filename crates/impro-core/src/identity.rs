//! Central application identity and environment-variable contract.
pub const APP_NAME: &str = "impro";
pub const APP_ID: &str = "dev.impro.impro";
/// Windows AppUserModelID: taskbar grouping and pinning; the installer shortcut must carry the same string.
pub const APP_USER_MODEL_ID: &str = APP_ID;
pub const WINDOW_TITLE: &str = APP_NAME;
pub const CONFIG_DIR: &str = "impro";
pub const SETTINGS_FILE: &str = "settings.json";
pub const NOTES_DIR: &str = ".impro/Notes";
pub const REDUCE_MOTION_ENV: &str = "IMPRO_REDUCE_MOTION";
/// Debug builds only: `1` docks the menu/toggle bar inside the window (the Windows layout) on any platform.
pub const DOCKED_TITLEBAR_ENV: &str = "IMPRO_DOCKED_TITLEBAR";
pub const QA_SCREENSHOT_ENV: &str = "IMPRO_QA_SCREENSHOT";
pub const GRAPH_QA_ENV: &str = "IMPRO_GRAPH_QA";
pub const ENV_PREFIX: &str = "IMPRO_";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const QA_TIMING_ENV: &str = "IMPRO_QA_TIMING";
pub const GRAPH_DEBUG_ENV: &str = "IMPRO_GRAPH_DEBUG";
pub const GRAPH_GPU_ENV: &str = "IMPRO_GRAPH_GPU";
pub const CSS_PREFIX: &str = "impro-";
pub const ICON_NAME: &str = "impro";
pub const DESKTOP_FILE: &str = "impro.desktop";
