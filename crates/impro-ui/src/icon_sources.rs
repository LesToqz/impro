// Raster source identity and raw symbolic channels. Generated with regenerate.py.
pub(super) const SOURCES: &[(&str, &str, &[u8])] = &[
    (
        "document-new-symbolic",
        "/usr/share/icons/Yaru/scalable/actions/document-new-symbolic.svg",
        include_bytes!("../assets/icons/document-new-symbolic.svg"),
    ),
    (
        "edit-delete-symbolic",
        "/usr/share/icons/Yaru/scalable/actions/edit-delete-symbolic.svg",
        include_bytes!("../assets/icons/edit-delete-symbolic.svg"),
    ),
    (
        "edit-find-symbolic",
        "/usr/share/icons/Yaru/scalable/actions/edit-find-symbolic.svg",
        include_bytes!("../assets/icons/edit-find-symbolic.svg"),
    ),
    (
        "folder-new-symbolic",
        "/usr/share/icons/Yaru/scalable/actions/folder-new-symbolic.svg",
        include_bytes!("../assets/icons/folder-new-symbolic.svg"),
    ),
    (
        "list-add-symbolic",
        "/usr/share/icons/Yaru/scalable/actions/list-add-symbolic.svg",
        include_bytes!("../assets/icons/list-add-symbolic.svg"),
    ),
    (
        "pan-down-symbolic",
        "/usr/share/icons/Yaru/scalable/ui/pan-down-symbolic.svg",
        include_bytes!("../assets/icons/pan-down-symbolic.svg"),
    ),
    (
        "panel-bottom-symbolic",
        "/usr/share/icons/hicolor/scalable/actions/panel-bottom-symbolic.svg",
        include_bytes!("../assets/icons/panel-bottom-symbolic.svg"),
    ),
    (
        "preferences-system-symbolic",
        "/usr/share/icons/Yaru/scalable/apps/preferences-system-symbolic.svg",
        include_bytes!("../assets/icons/preferences-system-symbolic.svg"),
    ),
    (
        "sidebar-show-right-symbolic",
        "/usr/share/icons/Yaru/scalable/actions/sidebar-show-right-symbolic.svg",
        include_bytes!("../assets/icons/sidebar-show-right-symbolic.svg"),
    ),
    (
        "sidebar-show-symbolic",
        "/usr/share/icons/Yaru/scalable/actions/sidebar-show-symbolic.svg",
        include_bytes!("../assets/icons/sidebar-show-symbolic.svg"),
    ),
    (
        "view-fullscreen-symbolic",
        "/usr/share/icons/Yaru/scalable/actions/view-fullscreen-symbolic.svg",
        include_bytes!("../assets/icons/view-fullscreen-symbolic.svg"),
    ),
    (
        "view-refresh-symbolic",
        "/usr/share/icons/Yaru/scalable/actions/view-refresh-symbolic.svg",
        include_bytes!("../assets/icons/view-refresh-symbolic.svg"),
    ),
    (
        "window-close-symbolic",
        "/usr/share/icons/Yaru/scalable/ui/window-close-symbolic.svg",
        include_bytes!("../assets/icons/window-close-symbolic.svg"),
    ),
    (
        "window-maximize-symbolic",
        "/usr/share/icons/Yaru/scalable/ui/window-maximize-symbolic.svg",
        include_bytes!("../assets/icons/window-maximize-symbolic.svg"),
    ),
    (
        "window-minimize-symbolic",
        "/usr/share/icons/Yaru/scalable/ui/window-minimize-symbolic.svg",
        include_bytes!("../assets/icons/window-minimize-symbolic.svg"),
    ),
    (
        "window-restore-symbolic",
        "/usr/share/icons/Yaru/scalable/ui/window-restore-symbolic.svg",
        include_bytes!("../assets/icons/window-restore-symbolic.svg"),
    ),
];
pub(super) fn pixels(name: &str, size: i32) -> Option<&'static [u8]> {
    match (name, size) {
        ("document-new-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/document-new-symbolic.rgba"
        )),
        ("edit-delete-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/edit-delete-symbolic.rgba"
        )),
        ("edit-find-symbolic", 16) => {
            Some(include_bytes!("../assets/icons/16/edit-find-symbolic.rgba"))
        }
        ("folder-new-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/folder-new-symbolic.rgba"
        )),
        ("list-add-symbolic", 16) => {
            Some(include_bytes!("../assets/icons/16/list-add-symbolic.rgba"))
        }
        ("pan-down-symbolic", 16) => {
            Some(include_bytes!("../assets/icons/16/pan-down-symbolic.rgba"))
        }
        ("panel-bottom-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/panel-bottom-symbolic.rgba"
        )),
        ("preferences-system-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/preferences-system-symbolic.rgba"
        )),
        ("sidebar-show-right-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/sidebar-show-right-symbolic.rgba"
        )),
        ("sidebar-show-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/sidebar-show-symbolic.rgba"
        )),
        ("view-fullscreen-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/view-fullscreen-symbolic.rgba"
        )),
        ("view-refresh-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/view-refresh-symbolic.rgba"
        )),
        ("window-close-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/window-close-symbolic.rgba"
        )),
        ("window-maximize-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/window-maximize-symbolic.rgba"
        )),
        ("window-minimize-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/window-minimize-symbolic.rgba"
        )),
        ("window-restore-symbolic", 16) => Some(include_bytes!(
            "../assets/icons/16/window-restore-symbolic.rgba"
        )),
        ("document-new-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/document-new-symbolic.rgba"
        )),
        ("edit-delete-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/edit-delete-symbolic.rgba"
        )),
        ("edit-find-symbolic", 32) => {
            Some(include_bytes!("../assets/icons/32/edit-find-symbolic.rgba"))
        }
        ("folder-new-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/folder-new-symbolic.rgba"
        )),
        ("list-add-symbolic", 32) => {
            Some(include_bytes!("../assets/icons/32/list-add-symbolic.rgba"))
        }
        ("pan-down-symbolic", 32) => {
            Some(include_bytes!("../assets/icons/32/pan-down-symbolic.rgba"))
        }
        ("panel-bottom-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/panel-bottom-symbolic.rgba"
        )),
        ("preferences-system-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/preferences-system-symbolic.rgba"
        )),
        ("sidebar-show-right-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/sidebar-show-right-symbolic.rgba"
        )),
        ("sidebar-show-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/sidebar-show-symbolic.rgba"
        )),
        ("view-fullscreen-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/view-fullscreen-symbolic.rgba"
        )),
        ("view-refresh-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/view-refresh-symbolic.rgba"
        )),
        ("window-close-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/window-close-symbolic.rgba"
        )),
        ("window-maximize-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/window-maximize-symbolic.rgba"
        )),
        ("window-minimize-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/window-minimize-symbolic.rgba"
        )),
        ("window-restore-symbolic", 32) => Some(include_bytes!(
            "../assets/icons/32/window-restore-symbolic.rgba"
        )),
        ("window-close-symbolic", 64) => Some(include_bytes!(
            "../assets/icons/64/window-close-symbolic.rgba"
        )),
        ("window-maximize-symbolic", 64) => Some(include_bytes!(
            "../assets/icons/64/window-maximize-symbolic.rgba"
        )),
        ("window-minimize-symbolic", 64) => Some(include_bytes!(
            "../assets/icons/64/window-minimize-symbolic.rgba"
        )),
        ("window-restore-symbolic", 64) => Some(include_bytes!(
            "../assets/icons/64/window-restore-symbolic.rgba"
        )),
        _ => None,
    }
}

const COLORS: &[&str] = &[
    "#000000", "#073642", "#20352e", "#283e3b", "#333333", "#526a60", "#616161", "#657b83",
    "#6b766a", "#7299ab", "#828997", "#839496", "#858585", "#8793a8", "#8b93b6", "#8b949e",
    "#909084", "#a3b5b8", "#b09887", "#c8ccd4", "#cccccc", "#d5f1fa", "#e3eceb", "#e6edf3",
    "#eceff4", "#eee8d5", "#f7e5cf", "#f8f8f2", "#ffffff",
];
pub(super) fn colored_pixels(name: &str, size: i32, color: &str) -> Option<&'static [u8]> {
    let index = COLORS.iter().position(|c| *c == color)?;
    let bytes: &[u8] = match (name, size) {
        ("document-new-symbolic", 16) => {
            include_bytes!("../assets/icons/16/document-new-symbolic-colors.rgba")
        }
        ("edit-delete-symbolic", 16) => {
            include_bytes!("../assets/icons/16/edit-delete-symbolic-colors.rgba")
        }
        ("edit-find-symbolic", 16) => {
            include_bytes!("../assets/icons/16/edit-find-symbolic-colors.rgba")
        }
        ("folder-new-symbolic", 16) => {
            include_bytes!("../assets/icons/16/folder-new-symbolic-colors.rgba")
        }
        ("list-add-symbolic", 16) => {
            include_bytes!("../assets/icons/16/list-add-symbolic-colors.rgba")
        }
        ("pan-down-symbolic", 16) => {
            include_bytes!("../assets/icons/16/pan-down-symbolic-colors.rgba")
        }
        ("panel-bottom-symbolic", 16) => {
            include_bytes!("../assets/icons/16/panel-bottom-symbolic-colors.rgba")
        }
        ("preferences-system-symbolic", 16) => {
            include_bytes!("../assets/icons/16/preferences-system-symbolic-colors.rgba")
        }
        ("sidebar-show-right-symbolic", 16) => {
            include_bytes!("../assets/icons/16/sidebar-show-right-symbolic-colors.rgba")
        }
        ("sidebar-show-symbolic", 16) => {
            include_bytes!("../assets/icons/16/sidebar-show-symbolic-colors.rgba")
        }
        ("view-fullscreen-symbolic", 16) => {
            include_bytes!("../assets/icons/16/view-fullscreen-symbolic-colors.rgba")
        }
        ("view-refresh-symbolic", 16) => {
            include_bytes!("../assets/icons/16/view-refresh-symbolic-colors.rgba")
        }
        ("window-close-symbolic", 16) => {
            include_bytes!("../assets/icons/16/window-close-symbolic-colors.rgba")
        }
        ("window-maximize-symbolic", 16) => {
            include_bytes!("../assets/icons/16/window-maximize-symbolic-colors.rgba")
        }
        ("window-minimize-symbolic", 16) => {
            include_bytes!("../assets/icons/16/window-minimize-symbolic-colors.rgba")
        }
        ("window-restore-symbolic", 16) => {
            include_bytes!("../assets/icons/16/window-restore-symbolic-colors.rgba")
        }
        ("document-new-symbolic", 32) => {
            include_bytes!("../assets/icons/32/document-new-symbolic-colors.rgba")
        }
        ("edit-delete-symbolic", 32) => {
            include_bytes!("../assets/icons/32/edit-delete-symbolic-colors.rgba")
        }
        ("edit-find-symbolic", 32) => {
            include_bytes!("../assets/icons/32/edit-find-symbolic-colors.rgba")
        }
        ("folder-new-symbolic", 32) => {
            include_bytes!("../assets/icons/32/folder-new-symbolic-colors.rgba")
        }
        ("list-add-symbolic", 32) => {
            include_bytes!("../assets/icons/32/list-add-symbolic-colors.rgba")
        }
        ("pan-down-symbolic", 32) => {
            include_bytes!("../assets/icons/32/pan-down-symbolic-colors.rgba")
        }
        ("panel-bottom-symbolic", 32) => {
            include_bytes!("../assets/icons/32/panel-bottom-symbolic-colors.rgba")
        }
        ("preferences-system-symbolic", 32) => {
            include_bytes!("../assets/icons/32/preferences-system-symbolic-colors.rgba")
        }
        ("sidebar-show-right-symbolic", 32) => {
            include_bytes!("../assets/icons/32/sidebar-show-right-symbolic-colors.rgba")
        }
        ("sidebar-show-symbolic", 32) => {
            include_bytes!("../assets/icons/32/sidebar-show-symbolic-colors.rgba")
        }
        ("view-fullscreen-symbolic", 32) => {
            include_bytes!("../assets/icons/32/view-fullscreen-symbolic-colors.rgba")
        }
        ("view-refresh-symbolic", 32) => {
            include_bytes!("../assets/icons/32/view-refresh-symbolic-colors.rgba")
        }
        ("window-close-symbolic", 32) => {
            include_bytes!("../assets/icons/32/window-close-symbolic-colors.rgba")
        }
        ("window-maximize-symbolic", 32) => {
            include_bytes!("../assets/icons/32/window-maximize-symbolic-colors.rgba")
        }
        ("window-minimize-symbolic", 32) => {
            include_bytes!("../assets/icons/32/window-minimize-symbolic-colors.rgba")
        }
        ("window-restore-symbolic", 32) => {
            include_bytes!("../assets/icons/32/window-restore-symbolic-colors.rgba")
        }
        ("window-close-symbolic", 64) => {
            include_bytes!("../assets/icons/64/window-close-symbolic-colors.rgba")
        }
        ("window-maximize-symbolic", 64) => {
            include_bytes!("../assets/icons/64/window-maximize-symbolic-colors.rgba")
        }
        ("window-minimize-symbolic", 64) => {
            include_bytes!("../assets/icons/64/window-minimize-symbolic-colors.rgba")
        }
        ("window-restore-symbolic", 64) => {
            include_bytes!("../assets/icons/64/window-restore-symbolic-colors.rgba")
        }
        _ => return None,
    };
    let length = (size * size * 4) as usize;
    bytes.get(index * length..(index + 1) * length)
}
