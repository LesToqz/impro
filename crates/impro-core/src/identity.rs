//! Central application identity and environment-variable contract.
pub const APP_NAME: &str = "impro";
pub const APP_ID: &str = "dev.impro.impro";
pub const WINDOW_TITLE: &str = APP_NAME;
pub const CONFIG_DIR: &str = "impro";
pub const SETTINGS_FILE: &str = "settings.json";
pub const NOTES_DIR: &str = ".impro/Notes";
pub const REDUCE_MOTION_ENV: &str = "IMPRO_REDUCE_MOTION";
pub const QA_SCREENSHOT_ENV: &str = "IMPRO_QA_SCREENSHOT";
pub const GRAPH_QA_ENV: &str = "IMPRO_GRAPH_QA";
pub const ENV_PREFIX: &str = "IMPRO_";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
