//! The searchable shortcut list shown on the Hotkeys page.

/// Today's app shortcuts (window accelerators and the README table); the window may
/// replace them with `SettingsDialog::set_hotkeys`. " / " separates alternatives.
pub const DEFAULT_HOTKEYS: [(&str, &str); 35] = [
    ("Save file", "Ctrl+S"),
    ("Save all files", "Ctrl+Shift+S"),
    ("Quit", "Ctrl+Q"),
    ("Open settings", "Ctrl+,"),
    ("Next theme", "Ctrl+Alt+T"),
    ("Toggle explorer sidebar", "Ctrl+B"),
    ("Search in files", "Ctrl+Shift+F"),
    ("Toggle bottom terminal", "Ctrl+`"),
    ("Toggle right terminal", "Ctrl+Alt+`"),
    ("Split terminal", "Ctrl+Shift+5"),
    ("Toggle terminal fullscreen", "Ctrl+Shift+F11"),
    ("Close editor tab", "Ctrl+W"),
    ("Undo", "Ctrl+Z"),
    ("Redo", "Ctrl+Shift+Z"),
    ("Find in file", "Ctrl+F"),
    ("Toggle Markdown preview", "Ctrl+Shift+V"),
    ("Next editor tab", "Ctrl+Tab / Ctrl+PageDown"),
    ("Previous editor tab", "Ctrl+Shift+Tab / Ctrl+PageUp"),
    ("Toggle line comment", "Ctrl+/"),
    ("Zoom editor in", "Ctrl+Plus / Ctrl+="),
    ("Zoom editor out", "Ctrl+Minus"),
    ("Reset editor zoom", "Ctrl+0"),
    ("Split focused terminal", "Ctrl+Shift+T"),
    ("Kill focused terminal", "Ctrl+Shift+W"),
    ("Previous / next terminal", "Ctrl+Alt+Left / Ctrl+Alt+Right"),
    (
        "Previous / next stacked terminal",
        "Ctrl+Alt+Up / Ctrl+Alt+Down",
    ),
    ("Copy in terminal", "Ctrl+Shift+C"),
    ("Paste in terminal", "Ctrl+Shift+V"),
    ("Zoom terminal in", "Ctrl+Shift+Plus"),
    ("Zoom terminal out", "Ctrl+Shift+Minus"),
    ("Reset terminal zoom", "Ctrl+Shift+0"),
    ("Rename in explorer", "F2"),
    ("Delete in explorer", "Delete"),
    ("Refresh explorer", "F5"),
    ("New folder in explorer", "Ctrl+Shift+N"),
];

/// The default list as owned pairs.
pub fn default_hotkeys() -> Vec<(String, String)> {
    DEFAULT_HOTKEYS
        .iter()
        .map(|(label, accel)| (label.to_string(), accel.to_string()))
        .collect()
}

/// Merge command hotkeys with the panel-local defaults not already listed (what
/// the window's `hotkeys()` does): `commands` are `(label, accelerators)` pairs;
/// a default is kept only when none of its alternatives is in use by a command.
pub fn merge_hotkeys(commands: &[(String, Vec<String>)]) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = commands
        .iter()
        .filter(|(_, keys)| !keys.is_empty())
        .map(|(label, keys)| (label.clone(), keys.join(" / ")))
        .collect();
    let used: std::collections::HashSet<String> = pairs
        .iter()
        .flat_map(|(_, keys)| keys.split(" / ").map(str::to_owned).collect::<Vec<_>>())
        .collect();
    for (label, keys) in DEFAULT_HOTKEYS {
        if !keys.split(" / ").any(|key| used.contains(key)) {
            pairs.push((label.to_owned(), keys.to_owned()));
        }
    }
    pairs
}
