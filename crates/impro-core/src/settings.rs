//! Failure-tolerant private JSON settings, resolved at call time; graph blobs stay opaque.
use crate::{identity, themes};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};
pub type Values = Map<String, Value>;
pub const FONT_KEYS: [&str; 3] = [
    "terminal_font_size",
    "editor_font_size",
    "explorer_font_size",
];
pub const FONT_NAME_MAX: usize = 200;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownSetting(pub String);
impl std::fmt::Display for UnknownSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown setting key: {}", self.0)
    }
}
impl std::error::Error for UnknownSetting {}
pub fn defaults() -> &'static Values {
    static V: OnceLock<Values> = OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_str(include_str!("defaults.json")).expect("embedded defaults")
    })
}
pub fn limits() -> &'static BTreeMap<String, (f64, f64)> {
    static V: OnceLock<BTreeMap<String, (f64, f64)>> = OnceLock::new();
    V.get_or_init(|| serde_json::from_str(include_str!("limits.json")).expect("embedded limits"))
}
pub fn choices() -> &'static BTreeMap<String, Vec<String>> {
    static V: OnceLock<BTreeMap<String, Vec<String>>> = OnceLock::new();
    V.get_or_init(|| serde_json::from_str(include_str!("choices.json")).expect("embedded choices"))
}
pub fn accent_presets() -> &'static [String] {
    static V: OnceLock<Vec<String>> = OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_str(include_str!("accent_presets.json")).expect("embedded accents")
    })
}
pub fn default_font_sizes() -> BTreeMap<&'static str, i32> {
    FONT_KEYS.into_iter().zip([11, 11, 10]).collect()
}
pub fn font_size_limits() -> BTreeMap<&'static str, (i32, i32)> {
    FONT_KEYS
        .into_iter()
        .zip([(7, 32), (7, 32), (8, 24)])
        .collect()
}
pub fn new_keys() -> Vec<&'static str> {
    defaults()
        .keys()
        .filter(|k| k.as_str() != "theme" && !FONT_KEYS.contains(&k.as_str()))
        .map(String::as_str)
        .collect()
}
pub fn normalize(key: &str, value: &Value) -> Result<Value, UnknownSetting> {
    let default = defaults()
        .get(key)
        .ok_or_else(|| UnknownSetting(key.into()))?;
    if key == "theme" {
        return Ok(Value::from(themes::get_theme(value.as_str()).id.clone()));
    }
    if key == "accent" {
        return Ok(value
            .as_str()
            .filter(|s| themes::valid_color(s))
            .map(|s| Value::from(s.to_ascii_lowercase()))
            .unwrap_or(Value::Null));
    }
    if let Some(options) = choices().get(key) {
        return Ok(value
            .as_str()
            .filter(|s| options.iter().any(|x| x == s))
            .map(Value::from)
            .unwrap_or_else(|| default.clone()));
    }
    if default.is_boolean() {
        return Ok(if value.is_boolean() {
            value.clone()
        } else {
            default.clone()
        });
    }
    if let Some(&(lo, hi)) = limits().get(key) {
        let number = value.as_f64().filter(|v| v.is_finite()).or_else(|| {
            // JSON integers have unbounded precision in the source settings store.
            // Clamp enormous integers without coercing them to infinity; nonfinite
            // floating-point values still fall back to the default.
            let raw = value.as_number()?.to_string();
            let digits = raw.strip_prefix('-').unwrap_or(&raw);
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                Some(if raw.starts_with('-') { lo } else { hi })
            } else {
                None
            }
        });
        return Ok(number
            .map(|v| Value::from(v.clamp(lo, hi).round_ties_even() as i64))
            .unwrap_or_else(|| default.clone()));
    }
    Ok(value
        .as_str()
        .filter(|s| s.chars().count() <= FONT_NAME_MAX && s.chars().all(crate::text::is_printable))
        .map(|s| Value::from(s.trim()))
        .unwrap_or_else(|| default.clone()))
}
pub fn settings_path() -> PathBuf {
    crate::platform::config_root()
        .join(identity::CONFIG_DIR)
        .join(identity::SETTINGS_FILE)
}
/// Explicit-path store supports isolated tests and avoids mutating process environment.
#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
}
impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn load(&self) -> Values {
        fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Values>(&bytes).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, data: &Values) -> io::Result<()> {
        let mut merged = self.load();
        merged.extend(data.clone());
        let parent = self.path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "settings path needs a parent")
        })?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::Builder::new()
            .prefix(".settings-")
            .tempfile_in(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        serde_json::to_writer_pretty(&mut file, &merged)?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.as_file().sync_all()?;
        file.persist(&self.path).map_err(|e| e.error)?;
        // Replacement has committed. A directory sync is best-effort: reporting
        // failure now would incorrectly promise that the old file was preserved.
        let _ = fs::File::open(parent).and_then(|directory| directory.sync_all());
        Ok(())
    }
    pub fn get_value(&self, key: &str) -> Result<Value, UnknownSetting> {
        normalize(key, self.load().get(key).unwrap_or(&Value::Null))
    }
    pub fn get_values(&self) -> Values {
        let data = self.load();
        defaults()
            .keys()
            .map(|key| {
                (
                    key.clone(),
                    normalize(key, data.get(key).unwrap_or(&Value::Null)).expect("known key"),
                )
            })
            .collect()
    }
    pub fn set_value(&self, key: &str, value: &Value) -> Result<bool, UnknownSetting> {
        let value = normalize(key, value)?;
        Ok(self
            .save(&[(key.to_string(), value)].into_iter().collect())
            .is_ok())
    }
    pub fn set_values(&self, values: &Values) -> Result<bool, UnknownSetting> {
        let data = values
            .iter()
            .map(|(k, v)| normalize(k, v).map(|v| (k.clone(), v)))
            .collect::<Result<Values, _>>()?;
        Ok(self.save(&data).is_ok())
    }
    pub fn get_blob(&self, key: &str) -> Value {
        self.load().remove(key).unwrap_or(Value::Null)
    }
    pub fn set_blob(&self, key: &str, value: &Value) -> bool {
        self.save(&[(key.to_string(), value.clone())].into_iter().collect())
            .is_ok()
    }
}
pub fn load() -> Values {
    Store::new(settings_path()).load()
}
pub fn save(data: &Values) -> bool {
    Store::new(settings_path()).save(data).is_ok()
}
pub fn get_value(key: &str) -> Result<Value, UnknownSetting> {
    Store::new(settings_path()).get_value(key)
}
pub fn get_values() -> Values {
    Store::new(settings_path()).get_values()
}
pub fn set_value(key: &str, value: &Value) -> Result<bool, UnknownSetting> {
    Store::new(settings_path()).set_value(key, value)
}
pub fn set_values(values: &Values) -> Result<bool, UnknownSetting> {
    Store::new(settings_path()).set_values(values)
}
pub fn reset_value(key: &str) -> Result<bool, UnknownSetting> {
    set_value(
        key,
        defaults()
            .get(key)
            .ok_or_else(|| UnknownSetting(key.into()))?,
    )
}
pub fn get_theme_id() -> String {
    themes::get_theme(load().get("theme").and_then(Value::as_str))
        .id
        .clone()
}
pub fn set_theme_id(id: Option<&str>) -> bool {
    save(
        &[(
            "theme".into(),
            Value::from(themes::get_theme(id).id.clone()),
        )]
        .into_iter()
        .collect(),
    )
}
pub fn get_font_size(key: &str) -> Result<i32, UnknownSetting> {
    if !FONT_KEYS.contains(&key) {
        return Err(UnknownSetting(key.into()));
    }
    Ok(get_value(key)?.as_i64().unwrap_or(11) as i32)
}
pub fn get_font_sizes() -> BTreeMap<String, i32> {
    let data = get_values();
    FONT_KEYS
        .into_iter()
        .map(|k| (k.into(), data[k].as_i64().unwrap_or(11) as i32))
        .collect()
}
pub fn set_font_size(key: &str, value: &Value) -> Result<bool, UnknownSetting> {
    if !FONT_KEYS.contains(&key) {
        return Err(UnknownSetting(key.into()));
    }
    set_value(key, value)
}
pub fn reset_font_size(key: &str) -> Result<bool, UnknownSetting> {
    if !FONT_KEYS.contains(&key) {
        return Err(UnknownSetting(key.into()));
    }
    reset_value(key)
}
pub fn get_graph() -> Value {
    Store::new(settings_path()).get_blob("graph")
}
pub fn set_graph(value: &Value) -> bool {
    Store::new(settings_path()).set_blob("graph", value)
}
pub fn get_gitgraph() -> Value {
    Store::new(settings_path()).get_blob("gitgraph")
}
pub fn set_gitgraph(value: &Value) -> bool {
    Store::new(settings_path()).set_blob("gitgraph", value)
}
