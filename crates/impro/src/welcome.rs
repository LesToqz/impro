//! The editor's empty state: the impro mark, the main ways to start, and recent folders.
//!
//! Plain GTK widgets styled through `impro_ui::register_css`, so theme and accent changes
//! apply live with the rest of the application. Nothing here animates or runs a timer, the
//! rows are ordinary buttons (Tab, arrows, Enter and Space work), and construction reads no
//! file: the recent list comes from the in-memory view of `state.json` loaded in `launch`.
use crate::{launch, window::WindowWidgets, MainWindow};
use gtk::{atk::prelude::AtkObjectExt, prelude::*};
use impro_core::themes::UiColors;
use impro_ui::widgets::{CenteredColumn, WrapBox};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::{Rc, Weak},
};

const TAGLINE: &str = "Notes, code and a terminal in one window.";
/// Recent folders shown on the page; the File menu lists all of them.
const MAX_SHOWN: usize = 5;
const COLUMN_WIDTH: i32 = 316;
const COLUMN_GAP: i32 = 40;
const MARK_HEIGHT: i32 = 68;
/// Panes shorter than this (in pixels) use the compact layout.
const COMPACT_BELOW: i32 = 420;

// Secondary text is the foreground at 74% rather than the muted token: that keeps every
// bundled theme at or above 4.5:1 on the page and on the key caps, which `muted` does not
// (3.2:1 to 4.5:1 on several themes). Hover and focus tint with the accent, so an accent
// change shows here as well; the text itself never changes colour.
const CSS: &str = r#"
.impro-welcome, .impro-welcome viewport { background: {ui['bg']}; }
.impro-welcome-title { color: {ui['foreground']}; font-size: 26px; font-weight: 600; }
.impro-welcome-tagline { color: alpha({ui['foreground']}, 0.74); font-size: 14px; }
.impro-welcome-heading { color: alpha({ui['foreground']}, 0.74); font-size: 12px;
    font-weight: 600; padding: 0 11px 6px; }
button.impro-welcome-row { background: transparent; background-image: none;
    border: 1px solid transparent; border-radius: 8px; box-shadow: none; text-shadow: none;
    color: {ui['foreground']}; font-size: 14px; min-height: 22px; padding: 6px 10px;
    transition: none; -gtk-outline-radius: 8px; }
button.impro-welcome-row:hover, button.impro-welcome-row:focus {
    background: alpha({ui['accent']}, 0.14); }
button.impro-welcome-row:active { background: alpha({ui['accent']}, 0.24); }
label.impro-welcome-key { background: alpha({ui['overlay']}, 0.07);
    border: 1px solid alpha({ui['overlay']}, 0.14); border-radius: 5px;
    color: alpha({ui['foreground']}, 0.74); font-size: 11px; font-weight: 500;
    min-width: 12px; padding: 0 6px; }
label.impro-welcome-path { color: alpha({ui['foreground']}, 0.74); font-size: 12px; }
button.impro-welcome-row.impro-welcome-tight { padding: 3px 10px; }
label.impro-welcome-title.impro-welcome-tight { font-size: 22px; }
label.impro-welcome-heading.impro-welcome-tight { padding-bottom: 3px; }
"#;

/// Register the welcome CSS. Call before the first theme is applied (before the window is built).
pub fn init() {
    impro_ui::register_css("app.welcome", css);
}

pub fn css(ui: &UiColors) -> String {
    impro_ui::render_css(CSS, ui)
}

struct Action {
    label: &'static str,
    /// Command-palette label that carries this action's shortcut, if it has one.
    command: &'static str,
    /// Needs the notes feature (and the explorer that hosts it).
    notes: bool,
    run: fn(&MainWindow),
}

const ACTIONS: [Action; 5] = [
    Action {
        label: "Open Folder…",
        command: "File: Open Folder",
        notes: false,
        run: MainWindow::open_folder,
    },
    Action {
        label: "Quick Switcher",
        command: "File: Quick Switcher",
        notes: false,
        run: MainWindow::open_quick_switcher,
    },
    Action {
        label: "Command Palette",
        command: "View: Command Palette",
        notes: false,
        run: MainWindow::open_command_palette,
    },
    Action {
        label: "New Note",
        command: "Notes: New note",
        notes: true,
        run: MainWindow::new_note,
    },
    Action {
        label: "Settings",
        command: "Settings: Open settings",
        notes: false,
        run: |window| window.open_settings(None),
    },
];

/// "Ctrl+Shift+P" becomes one cap per key; a trailing "+" key stays a key.
pub fn keycaps(shortcut: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut rest = shortcut;
    loop {
        match rest.find('+') {
            Some(0) => {
                keys.push("+".to_owned());
                break;
            }
            Some(at) => {
                keys.push(rest[..at].to_owned());
                rest = &rest[at + 1..];
            }
            None => {
                if !rest.is_empty() {
                    keys.push(rest.to_owned());
                }
                break;
            }
        }
    }
    keys
}

fn class(widget: &impl IsA<gtk::Widget>, name: &str) {
    widget.style_context().add_class(name);
}

fn label(text: &str, style: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    class(&label, style);
    label
}

fn row_button() -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    // Keyboard focus shows the ring; a mouse click should not leave one behind.
    button.set_focus_on_click(false);
    class(&button, "impro-welcome-row");
    button
}

fn name_button(button: &gtk::Button, name: &str, description: Option<&str>) {
    if let Some(accessible) = button.accessible() {
        accessible.set_name(name);
        if let Some(description) = description {
            accessible.set_description(description);
        }
    }
}

fn action_row(text: &str, shortcut: Option<&str>) -> gtk::Button {
    let button = row_button();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = label(text, "impro-welcome-label");
    row.pack_start(&title, true, true, 0);
    if let Some(shortcut) = shortcut {
        let keys = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        keys.set_valign(gtk::Align::Center);
        for key in keycaps(shortcut) {
            keys.pack_start(&label(&key, "impro-welcome-key"), false, false, 0);
        }
        row.pack_end(&keys, false, false, 0);
    }
    button.add(&row);
    let spoken = text.trim_end_matches('…');
    match shortcut {
        Some(shortcut) => name_button(&button, &format!("{spoken}, {shortcut}"), None),
        None => name_button(&button, spoken, None),
    }
    button
}

fn recent_row(path: &std::path::Path, home: Option<&std::path::Path>) -> gtk::Button {
    let button = row_button();
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let name = launch::folder_name(path);
    let title = label(&name, "impro-welcome-label");
    // An ellipsized label still reports its full text as its natural width; cap it so a
    // long path cannot widen the column (and push the two columns apart).
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(32);
    let shown = launch::display_path(path, home, cfg!(windows));
    let detail = label(&shown, "impro-welcome-path");
    detail.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    detail.set_max_width_chars(36);
    rows.pack_start(&title, false, false, 0);
    rows.pack_start(&detail, false, false, 0);
    button.add(&rows);
    button.set_tooltip_text(Some(&shown));
    name_button(&button, &name, Some(&shown));
    button
}

// The impro mark (packaging/icons/impro-mark.svg) as vector paths, so it is crisp at any
// scale, needs no image decoding, and costs no file access at startup.
const MARK_DOT: &str = "M349.1 63.5A16.0 16.0 0 0 0 326.1 49.1L216.1 102.6A45.0 45.0 0 0 0 190.8 143.1L190.8 209.6A16.0 16.0 0 0 0 213.8 223.9L324.1 170.1A44.5 44.5 0 0 0 349.1 130.1Z";
const MARK_BODY: &str = "M357.9 227.1A23.5 23.5 0 0 0 324.0 206.1L192.8 270.6A53.0 53.0 0 0 0 163.2 318.2L163.2 462.5A15.0 15.0 0 0 0 185.8 475.5L329.8 391.8A56.5 56.5 0 0 0 357.9 342.9Z";
const MARK_SHADE: &str = "M212 262L243 312L357 376L357 500L212 500Z";
const MARK_LIGHT: &str = "M212 262C212 290 225 302 243 312L357 376L357 190L212 190Z";
/// The artwork's group scale and the bounds of the drawn shapes after it.
const MARK_SCALE: f64 = 0.984615;
const MARK_BOUNDS: (f64, f64, f64, f64) = (160.0, 47.5, 194.0, 422.0);

fn path_tokens(data: &str) -> Vec<Result<char, f64>> {
    let mut tokens = Vec::new();
    let mut number = String::new();
    let flush = |number: &mut String, tokens: &mut Vec<Result<char, f64>>| {
        if let Ok(value) = number.parse() {
            tokens.push(Err(value));
        }
        number.clear();
    };
    for c in data.chars() {
        match c {
            c if c.is_ascii_alphabetic() => {
                flush(&mut number, &mut tokens);
                tokens.push(Ok(c));
            }
            ' ' | ',' => flush(&mut number, &mut tokens),
            '-' => {
                flush(&mut number, &mut tokens);
                number.push(c);
            }
            c => number.push(c),
        }
    }
    flush(&mut number, &mut tokens);
    tokens
}

/// Trace absolute M, L, C, A (circular arcs only) and Z path data.
fn trace(cr: &gtk::cairo::Context, data: &str) {
    let tokens = path_tokens(data);
    let mut at = 0;
    let mut current = (0.0, 0.0);
    let number = |at: &mut usize| {
        let value = tokens.get(*at).and_then(|t| t.as_ref().err()).copied();
        *at += 1;
        value.unwrap_or(0.0)
    };
    cr.new_path();
    while at < tokens.len() {
        let Ok(command) = tokens[at] else {
            at += 1;
            continue;
        };
        at += 1;
        match command {
            'M' | 'L' => {
                let (x, y) = (number(&mut at), number(&mut at));
                if command == 'M' {
                    cr.move_to(x, y);
                } else {
                    cr.line_to(x, y);
                }
                current = (x, y);
            }
            'C' => {
                let v: Vec<f64> = (0..6).map(|_| number(&mut at)).collect();
                cr.curve_to(v[0], v[1], v[2], v[3], v[4], v[5]);
                current = (v[4], v[5]);
            }
            'A' => {
                let v: Vec<f64> = (0..7).map(|_| number(&mut at)).collect();
                let (large, sweep, x, y) = (v[3] != 0.0, v[4] != 0.0, v[5], v[6]);
                arc(cr, current, v[0], large, sweep, (x, y));
                current = (x, y);
            }
            _ => cr.close_path(),
        }
    }
}

/// SVG endpoint arc to a cairo arc (radius is raised to half the chord when too small).
fn arc(
    cr: &gtk::cairo::Context,
    from: (f64, f64),
    radius: f64,
    large: bool,
    sweep: bool,
    to: (f64, f64),
) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let chord = dx.hypot(dy);
    if chord == 0.0 {
        return;
    }
    let r = radius.max(chord / 2.0);
    let height = (r * r - chord * chord / 4.0).max(0.0).sqrt();
    let side = if large != sweep { 1.0 } else { -1.0 };
    let centre = (
        (from.0 + to.0) / 2.0 + side * height * -dy / chord,
        (from.1 + to.1) / 2.0 + side * height * dx / chord,
    );
    let start = (from.1 - centre.1).atan2(from.0 - centre.0);
    let mut end = (to.1 - centre.1).atan2(to.0 - centre.0);
    let tau = std::f64::consts::TAU;
    if sweep {
        if end < start {
            end += tau;
        }
        cr.arc(centre.0, centre.1, r, start, end);
    } else {
        if end > start {
            end -= tau;
        }
        cr.arc_negative(centre.0, centre.1, r, start, end);
    }
}

fn gradient(line: (f64, f64, f64, f64), stops: &[(f64, u32, f64)]) -> gtk::cairo::LinearGradient {
    let gradient = gtk::cairo::LinearGradient::new(line.0, line.1, line.2, line.3);
    for &(offset, rgb, alpha) in stops {
        let channel = |shift: u32| f64::from((rgb >> shift) & 0xff) / 255.0;
        gradient.add_color_stop_rgba(offset, channel(16), channel(8), channel(0), alpha);
    }
    gradient
}

fn draw_mark(cr: &gtk::cairo::Context, width: f64, height: f64) -> Result<(), gtk::cairo::Error> {
    let (x, y, w, h) = MARK_BOUNDS;
    let scale = (width / w).min(height / h);
    cr.save()?;
    cr.translate((width - w * scale) / 2.0, (height - h * scale) / 2.0);
    cr.scale(scale, scale);
    cr.translate(-x, -y);
    cr.scale(MARK_SCALE, MARK_SCALE);
    trace(cr, MARK_DOT);
    cr.set_source(&gradient(
        (214.0, 224.0, 321.0, 56.0),
        &[
            (0.0, 0x4258f5, 1.0),
            (0.5, 0x7575f8, 1.0),
            (1.0, 0xa893fa, 1.0),
        ],
    ))?;
    cr.fill()?;
    trace(cr, MARK_BODY);
    cr.clip();
    cr.rectangle(150.0, 190.0, 230.0, 320.0);
    cr.set_source(&gradient(
        (0.0, 268.0, 0.0, 470.0),
        &[
            (0.0, 0x1f2db4, 1.0),
            (0.25, 0x3f45e4, 1.0),
            (0.65, 0x4b67f6, 1.0),
            (1.0, 0x3e77f8, 1.0),
        ],
    ))?;
    cr.fill()?;
    trace(cr, MARK_SHADE);
    cr.set_source(&gradient(
        (250.0, 300.0, 191.0, 405.0),
        &[
            (0.0, 0x1426b0, 1.0),
            (0.4, 0x1a34c0, 0.5),
            (1.0, 0x2050e0, 0.0),
        ],
    ))?;
    cr.fill()?;
    trace(cr, MARK_LIGHT);
    cr.set_source(&gradient(
        (286.0, 325.0, 346.0, 212.0),
        &[(0.0, 0x7c70f7, 1.0), (1.0, 0xa273f8, 1.0)],
    ))?;
    cr.fill()?;
    cr.restore()
}

fn size_mark(area: &gtk::DrawingArea, height: i32) {
    let (_, _, w, h) = MARK_BOUNDS;
    area.set_size_request((f64::from(height) * w / h).ceil() as i32, height);
}

fn mark(height: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    size_mark(&area, height);
    area.set_valign(gtk::Align::Center);
    area.connect_draw(move |area, cr| {
        let _ = draw_mark(
            cr,
            f64::from(area.allocated_width()),
            f64::from(area.allocated_height()),
        );
        glib::Propagation::Proceed
    });
    area
}

struct Inner {
    window: Weak<WindowWidgets>,
    density: Density,
    actions: Vec<(gtk::Button, bool)>,
    recents: gtk::Box,
    list: gtk::Box,
    shown: RefCell<Vec<PathBuf>>,
}

/// Handle to the installed welcome page.
#[derive(Clone)]
pub struct Welcome {
    root: gtk::Widget,
    inner: Rc<Inner>,
}

impl Welcome {
    pub fn widget(&self) -> &gtk::Widget {
        &self.root
    }
    /// The start actions in display order (hidden ones included).
    pub fn action_buttons(&self) -> Vec<gtk::Button> {
        self.inner.actions.iter().map(|(b, _)| b.clone()).collect()
    }
    /// Buttons of the recent-folder rows currently built.
    pub fn recent_buttons(&self) -> Vec<gtk::Button> {
        self.inner
            .list
            .children()
            .into_iter()
            .filter_map(|c| c.downcast().ok())
            .collect()
    }
    /// The "Recent folders" section.
    pub fn recent_section(&self) -> gtk::Widget {
        self.inner.recents.clone().upcast()
    }
    /// Re-read the recent list and feature switches (also done whenever the page maps).
    pub fn refresh(&self) {
        self.inner.refresh();
    }
    /// Force the compact (short pane) or the roomy layout; the page switches by itself
    /// when its height crosses a threshold.
    pub fn set_compact(&self, compact: bool) {
        self.inner.density.apply(compact);
    }
}

impl Inner {
    fn refresh(&self) {
        let Some(window) = self.window.upgrade().map(MainWindow) else {
            return;
        };
        let notes = window.feature_enabled("notes") && window.feature_enabled("explorer");
        for (button, needs_notes) in &self.actions {
            button.set_visible(!needs_notes || notes);
        }
        let recent: Vec<PathBuf> = launch::recent_excluding(&window.root())
            .into_iter()
            .take(MAX_SHOWN)
            .collect();
        self.recents.set_visible(!recent.is_empty());
        if *self.shown.borrow() == recent {
            return;
        }
        for child in self.list.children() {
            self.list.remove(&child);
        }
        let home = impro_core::platform::home_dir();
        for path in &recent {
            let button = recent_row(path, home.as_deref());
            let weak = self.window.clone();
            let path = path.clone();
            button.connect_clicked(move |_| {
                if let Some(window) = weak.upgrade().map(MainWindow) {
                    window.set_root(&path);
                }
            });
            self.density.restyle(&button);
            self.list.pack_start(&button, false, false, 0);
        }
        self.list.show_all();
        *self.shown.borrow_mut() = recent;
    }
}

/// The widgets that change between the roomy and the compact layout.
#[derive(Clone)]
struct Density {
    content: gtk::Box,
    logo: gtk::DrawingArea,
    tagline: gtk::Label,
    /// Restyled one by one: a style class set on an ancestor does not reach the rows,
    /// because style invalidation stops at the custom wrapping container.
    tight: Vec<gtk::Widget>,
    compact: Rc<Cell<bool>>,
}

impl Density {
    fn apply(&self, compact: bool) {
        self.compact.set(compact);
        let (gap, edge, mark) = if compact {
            (12, 6, 44)
        } else {
            (30, 28, MARK_HEIGHT)
        };
        self.content.set_spacing(gap);
        self.content.set_margin_top(edge);
        self.content.set_margin_bottom(edge);
        size_mark(&self.logo, mark);
        self.tagline.set_visible(!compact);
        for widget in &self.tight {
            self.restyle(widget);
        }
    }

    /// Give `widget` the style of the current layout (also used for rows built later).
    fn restyle(&self, widget: &impl IsA<gtk::Widget>) {
        let style = widget.style_context();
        if self.compact.get() {
            style.add_class("impro-welcome-tight");
        } else {
            style.remove_class("impro-welcome-tight");
        }
    }
}

/// A titled column: the column, its row container and its heading.
fn column(title: &str) -> (gtk::Box, gtk::Box, gtk::Label) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.set_size_request(COLUMN_WIDTH, -1);
    column.set_valign(gtk::Align::Start);
    let heading = label(title, "impro-welcome-heading");
    column.pack_start(&heading, false, false, 0);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    column.pack_start(&list, false, false, 0);
    (column, list, heading)
}

fn build(window: &MainWindow) -> Welcome {
    let shortcut = |command: &str| {
        window
            .commands()
            .into_iter()
            .find(|c| c.label == command)
            .and_then(|c| c.hotkey.first().cloned())
    };
    let weak = window.weak();

    let title = label("impro", "impro-welcome-title");
    let tagline = label(TAGLINE, "impro-welcome-tagline");
    let words = gtk::Box::new(gtk::Orientation::Vertical, 2);
    words.set_valign(gtk::Align::Center);
    words.pack_start(&title, false, false, 0);
    words.pack_start(&tagline, false, false, 0);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    header.set_margin_start(11);
    let logo = mark(MARK_HEIGHT);
    header.pack_start(&logo, false, false, 0);
    header.pack_start(&words, false, false, 0);

    let (start, start_list, start_heading) = column("Start");
    let mut actions = Vec::new();
    for action in &ACTIONS {
        let button = action_row(action.label, shortcut(action.command).as_deref());
        let run = action.run;
        let weak = weak.clone();
        button.connect_clicked(move |_| {
            if let Some(window) = weak.upgrade().map(MainWindow) {
                run(&window);
            }
        });
        start_list.pack_start(&button, false, false, 0);
        actions.push((button, action.notes));
    }
    let (recents, list, recents_heading) = column("Recent folders");

    let columns = WrapBox::new(COLUMN_GAP, 28);
    columns.add(&start);
    columns.add(&recents);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // Natural width, centered: a lone "Start" column sits in the middle of the page.
    content.set_halign(gtk::Align::Center);
    content.set_valign(gtk::Align::Center);
    content.pack_start(&header, false, false, 0);
    content.pack_start(&columns, false, false, 0);

    let centered = CenteredColumn::new(2 * COLUMN_WIDTH + COLUMN_GAP, 32, 0, 0);
    centered.add(&content);
    let scroller = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_shadow_type(gtk::ShadowType::None);
    class(&scroller, "impro-welcome");
    scroller.add(&centered);
    let mut tight: Vec<gtk::Widget> = vec![
        title.upcast(),
        start_heading.upcast(),
        recents_heading.upcast(),
    ];
    tight.extend(actions.iter().map(|(button, _)| button.clone().upcast()));
    let density = Density {
        content,
        logo,
        tagline,
        tight,
        compact: Rc::new(Cell::new(false)),
    };
    density.apply(false);
    // Keep the focused row on screen when the page is shorter than its content.
    let vadjustment = scroller.vadjustment();
    for container in [&start, &recents, &start_list, &list] {
        container.set_focus_vadjustment(&vadjustment);
    }
    scroller.show_all();

    // A short pane (a small window with the terminal open) gets tighter spacing so the
    // start actions stay on screen. Hysteresis keeps it from flapping at the boundary.
    let compact = Cell::new(false);
    let adapt = density.clone();
    scroller.connect_size_allocate(move |_, rect| {
        let was = compact.get();
        let now = rect.height()
            < if was {
                COMPACT_BELOW + 40
            } else {
                COMPACT_BELOW
            };
        if now != was {
            compact.set(now);
            let density = adapt.clone();
            // Never restyle in the middle of an allocation pass.
            glib::idle_add_local_once(move || density.apply(now));
        }
    });

    let inner = Rc::new(Inner {
        window: weak,
        density,
        actions,
        recents,
        list,
        shown: RefCell::new(Vec::new()),
    });
    // The map handler owns the page state for as long as the page lives.
    let owner = inner.clone();
    scroller.connect_map(move |_| owner.refresh());
    let observer = Rc::downgrade(&inner);
    launch::connect_recent_changed(move || match observer.upgrade() {
        Some(inner) => {
            inner.refresh();
            true
        }
        None => false,
    });
    inner.refresh();
    Welcome {
        root: scroller.upcast(),
        inner,
    }
}

/// Replace the editor's one-line placeholder with the welcome page.
pub fn install(window: &MainWindow) -> Welcome {
    let welcome = build(window);
    window.editor.set_empty_widget(welcome.widget());
    welcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycaps_split_on_plus() {
        assert_eq!(keycaps("Ctrl+Shift+P"), ["Ctrl", "Shift", "P"]);
        assert_eq!(keycaps("Ctrl+,"), ["Ctrl", ","]);
        assert_eq!(keycaps("F1"), ["F1"]);
        assert_eq!(keycaps("Ctrl++"), ["Ctrl", "+"]);
        assert!(keycaps("").is_empty());
    }

    #[test]
    fn mark_paths_tokenize_into_commands_and_numbers() {
        let tokens = path_tokens("M1.5-2L3 4Z");
        assert_eq!(
            tokens,
            vec![
                Ok('M'),
                Err(1.5),
                Err(-2.0),
                Ok('L'),
                Err(3.0),
                Err(4.0),
                Ok('Z')
            ]
        );
        // Every path of the mark is made of the commands the tracer understands.
        for data in [MARK_DOT, MARK_BODY, MARK_SHADE, MARK_LIGHT] {
            assert!(path_tokens(data)
                .iter()
                .all(|t| t.as_ref().map_or(true, |c| "MLCAZ".contains(*c))));
        }
    }

    #[test]
    fn css_renders_without_placeholders() {
        let theme = impro_core::themes::get_theme(Some("impro-dark"));
        let css = css(&theme.ui);
        assert!(!css.contains("{ui["), "{css}");
        assert!(
            css.contains(&theme.ui["accent"]),
            "hover tint follows the accent"
        );
        assert!(css.contains(&theme.ui["bg"]));
    }
}
