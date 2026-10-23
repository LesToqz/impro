//! The controls panel: Filters, Groups, Display and Forces, built from GTK
//! widgets and styled by `graph.css`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

use gtk::prelude::*;

use super::icons::{self, Icon};
use super::menu::GraphType;
use super::palette::Rgba;
use super::switch::Switch;
use crate::prefs::{self, ColorRule, GraphPrefs, MAX_COLOR_RULES};

pub const PANEL_WIDTH: i32 = 240;

/// A user edit of the panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Search { text: String, now: bool },
    Toggle { key: &'static str, value: bool },
    Slider { key: &'static str, value: f64 },
    AddRule,
    RemoveRule(usize),
    RuleQuery(usize, String),
    RuleColor(usize, u32),
    Collapse { key: &'static str, collapsed: bool },
    Open(bool),
    Reset,
    Replay,
    GraphType(GraphType),
}

type OnChange = Rc<dyn Fn(Change)>;

/// `(section id, persisted collapse key, title)` in panel order.
pub const SECTIONS: [(&str, &str, &str); 4] = [
    ("filters", "collapse-filter", "Project scope"),
    ("groups", "collapse-color-groups", "Colour rules"),
    ("display", "collapse-display", "Visual detail"),
    ("forces", "collapse-forces", "Layout tuning"),
];

struct Section {
    key: &'static str,
    head: gtk::EventBox,
    chevron: gtk::DrawingArea,
    revealer: gtk::Revealer,
    collapsed: Cell<bool>,
}

struct GroupRow {
    row: gtk::Box,
    entry: gtk::Entry,
    swatch: gtk::Button,
    color: Rc<Cell<Rgba>>,
}

struct SliderRow {
    row: gtk::Box,
    scale: gtk::Scale,
    value: gtk::Label,
    format: fn(f64) -> String,
    step: Option<f64>,
}

struct ToggleRow {
    row: gtk::Box,
    switch: Switch,
}

struct Parts {
    scroller: gtk::ScrolledWindow,
    replay_buttons: Vec<gtk::Button>,
    chip: gtk::Box,
    reset_button: gtk::Button,
    close_button: gtk::Button,
    panel_revealer: gtk::Revealer,
    sections: Vec<Section>,
    toggles: HashMap<&'static str, ToggleRow>,
    sliders: HashMap<&'static str, SliderRow>,
    search: gtk::Entry,
    search_clear: gtk::Button,
    groups_box: gtk::Box,
    rows: RefCell<Vec<GroupRow>>,
    shown_rules: RefCell<Vec<ColorRule>>,
    add_rule: gtk::Button,
    replay_row: gtk::Box,
    wand: gtk::Button,
    type_buttons: Vec<gtk::Button>,
    presets: RefCell<Vec<Rgba>>,
    open: Cell<bool>,
    animated: Cell<bool>,
    popover: RefCell<Option<gtk::Popover>>,
    type_menu: RefCell<Option<gtk::Menu>>,
    graph_type: Cell<GraphType>,
    local: Cell<bool>,
}

/// The controls; clones share widgets.
#[derive(Clone)]
pub struct Panel {
    /// Place this in the overlay, top right.
    pub root: gtk::Box,
    parts: Rc<Parts>,
    on_change: OnChange,
    syncing: Rc<Cell<bool>>,
}

fn fmt1(v: f64) -> String {
    format!("{v:.1}")
}
fn fmt2(v: f64) -> String {
    format!("{v:.2}")
}
fn fmt0(v: f64) -> String {
    format!("{}", v.round() as i64)
}

fn class(w: &impl IsA<gtk::Widget>, name: &str) {
    w.style_context().add_class(name);
}

fn label(text: &str, css: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_line_wrap(true);
    class(&l, css);
    l
}

fn toggle_row(
    key: &'static str,
    text: &str,
    tip: &str,
    on_change: &OnChange,
    syncing: &Rc<Cell<bool>>,
) -> (&'static str, ToggleRow) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    class(&row, "impro-graph-row");
    let name = label(text, "impro-graph-name");
    name.set_hexpand(true);
    name.set_tooltip_text(Some(tip));
    let switch = Switch::new(text);
    switch.widget().set_tooltip_text(None);
    row.pack_start(&name, true, true, 0);
    row.pack_end(switch.widget(), false, false, 0);
    {
        let on_change = on_change.clone();
        let syncing = syncing.clone();
        switch.connect_toggled(move |value| {
            if !syncing.get() {
                on_change(Change::Toggle { key, value });
            }
        });
    }
    (key, ToggleRow { row, switch })
}

#[allow(clippy::too_many_arguments)]
fn slider_row(
    key: &'static str,
    text: &str,
    tip: Option<&str>,
    format: fn(f64) -> String,
    on_change: &OnChange,
    syncing: &Rc<Cell<bool>>,
) -> (&'static str, SliderRow) {
    let (lo, hi) = prefs::limits(key).unwrap_or((0.0, 1.0));
    let step = prefs::STEPS
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|(_, s)| *s);
    let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
    class(&row, "impro-graph-row");
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let name = label(text, "impro-graph-name");
    name.set_hexpand(true);
    if let Some(tip) = tip {
        name.set_tooltip_text(Some(tip));
    }
    let value = gtk::Label::new(None);
    value.set_xalign(1.0);
    class(&value, "impro-graph-value");
    head.pack_start(&name, true, true, 0);
    head.pack_end(&value, false, false, 0);
    let adjustment = gtk::Adjustment::new(lo, lo, hi, step.unwrap_or((hi - lo) / 100.0), 0.0, 0.0);
    let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
    scale.set_draw_value(false);
    class(&scale, "impro-graph-scale");
    scale.set_margin_top(8);
    if let Some(a) = scale.accessible() {
        a.set_name(text);
    }
    if let Some(step) = step {
        // the control snaps to its step like a range input
        scale.connect_change_value(move |s, _, v| {
            let snapped = (lo + ((v - lo) / step).round() * step).clamp(lo, hi);
            s.set_value(snapped);
            glib::Propagation::Stop
        });
    }
    {
        let on_change = on_change.clone();
        let syncing = syncing.clone();
        let value_label = value.clone();
        adjustment.connect_value_changed(move |a| {
            value_label.set_text(&format(a.value()));
            if !syncing.get() {
                on_change(Change::Slider {
                    key,
                    value: a.value(),
                });
            }
        });
    }
    value.set_text(&format(lo));
    row.pack_start(&head, false, false, 0);
    row.pack_start(&scale, false, false, 0);
    (
        key,
        SliderRow {
            row,
            scale,
            value,
            format,
            step,
        },
    )
}

fn draw_chevron(area: &gtk::DrawingArea, collapsed: bool, cr: &cairo::Context) {
    let ctx = area.style_context();
    let muted = ctx
        .lookup_color("impro_graph_muted")
        .unwrap_or_else(|| gdk::RGBA::new(0.6, 0.7, 0.8, 1.0));
    cr.set_source_rgba(muted.red(), muted.green(), muted.blue(), 1.0);
    let size = area.allocated_width() as f64;
    if collapsed {
        cr.translate(size / 2.0, size / 2.0);
        cr.rotate(-FRAC_PI_2);
        cr.translate(-size / 2.0, -size / 2.0);
    }
    icons::draw(cr, Icon::Chevron, size);
}

fn swatch_button(color: Rc<Cell<Rgba>>) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    class(&button, "impro-graph-swatch");
    let area = gtk::DrawingArea::new();
    area.set_size_request(18, 18);
    area.connect_draw(move |a, cr| {
        let c = color.get();
        let (w, h) = (a.allocated_width() as f64, a.allocated_height() as f64);
        let radius = w.min(h) / 2.0;
        cr.set_source_rgba(c.r as f64, c.g as f64, c.b as f64, 1.0);
        cr.arc(w / 2.0, h / 2.0, radius - 0.5, 0.0, TAU);
        let _ = cr.fill_preserve();
        let border = a
            .style_context()
            .lookup_color("impro_graph_border")
            .unwrap_or_else(|| gdk::RGBA::new(0.3, 0.4, 0.5, 1.0));
        cr.set_source_rgba(border.red(), border.green(), border.blue(), 1.0);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
        glib::Propagation::Proceed
    });
    button.add(&area);
    button
}

impl Panel {
    pub fn new(on_change: impl Fn(Change) + 'static) -> Panel {
        let on_change: OnChange = Rc::new(on_change);
        let syncing = Rc::new(Cell::new(false));

        // --- sections -------------------------------------------------------
        let mut toggles: HashMap<&'static str, ToggleRow> = HashMap::new();
        let mut sliders: HashMap<&'static str, SliderRow> = HashMap::new();
        let mut add_toggle = |entry: (&'static str, ToggleRow)| -> gtk::Box {
            let row = entry.1.row.clone();
            toggles.insert(entry.0, entry.1);
            row
        };
        let mut add_slider = |entry: (&'static str, SliderRow)| -> gtk::Box {
            let row = entry.1.row.clone();
            sliders.insert(entry.0, entry.1);
            row
        };

        // filters
        let filters = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let search = gtk::Entry::new();
        search.set_placeholder_text(Some("Find paths, names or text…"));
        search.set_input_purpose(gtk::InputPurpose::FreeForm);
        class(&search, "impro-graph-entry");
        search.set_width_chars(1);
        if let Some(a) = search.accessible() {
            a.set_name("Filter project files");
        }
        let search_clear = icons::icon_button(Icon::Close, "Clear project filter", 16);
        search_clear.set_halign(gtk::Align::End);
        search_clear.set_valign(gtk::Align::Center);
        search_clear.set_margin_end(2);
        search_clear.set_no_show_all(true);
        let search_box = gtk::Overlay::new();
        search_box.add(&search);
        search_box.add_overlay(&search_clear);
        search_box.set_margin_top(2);
        search_box.set_margin_bottom(6);
        filters.pack_start(&search_box, false, false, 0);
        filters.pack_start(
            &add_slider(slider_row(
                "localDepth",
                "Neighbourhood hops",
                Some("Follow references up to this many hops from the active file"),
                fmt0,
                &on_change,
                &syncing,
            )),
            false,
            false,
            0,
        );
        for (key, text, tip) in [
            (
                "localIncoming",
                "Referenced by",
                "Include files that refer to this file",
            ),
            (
                "localOutgoing",
                "References to",
                "Include files reached from this file",
            ),
            (
                "showTags",
                "Tag markers",
                "Connect tags to the notes that mention them",
            ),
            (
                "showAttachments",
                "Referenced assets",
                "Include images and other referenced assets",
            ),
            (
                "hideUnresolved",
                "Hide missing targets",
                "Exclude references whose target cannot be found",
            ),
            (
                "showOrphans",
                "Unlinked files",
                "Keep files with no visible connections",
            ),
            (
                "showCode",
                "Source and imports",
                "Map source files and the modules they import",
            ),
        ] {
            filters.pack_start(
                &add_toggle(toggle_row(key, text, tip, &on_change, &syncing)),
                false,
                false,
                0,
            );
        }

        // groups
        let groups = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let groups_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let add_rule = gtk::Button::with_label("Add colour rule");
        class(&add_rule, "impro-graph-button");
        class(&add_rule, "impro-graph-add");
        groups.pack_start(&groups_box, false, false, 0);
        groups.pack_start(&add_rule, false, false, 0);

        // display
        let display = gtk::Box::new(gtk::Orientation::Vertical, 0);
        display.pack_start(
            &add_toggle(toggle_row(
                "showArrow",
                "Reference direction",
                "Mark the direction of each reference as you zoom in",
                &on_change,
                &syncing,
            )),
            false,
            false,
            0,
        );
        for (key, text, f) in [
            (
                "textFadeMultiplier",
                "Label detail",
                fmt1 as fn(f64) -> String,
            ),
            ("nodeSizeMultiplier", "File marker size", fmt2),
            ("lineSizeMultiplier", "Connection width", fmt2),
        ] {
            display.pack_start(
                &add_slider(slider_row(key, text, None, f, &on_change, &syncing)),
                false,
                false,
                0,
            );
        }
        let replay_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        class(&replay_row, "impro-graph-row");
        let replay = gtk::Button::with_label("Replay history");
        class(&replay, "impro-graph-button");
        replay.set_hexpand(true);
        replay.set_tooltip_text(Some("Replay project history"));
        replay_row.pack_start(&replay, true, true, 0);
        display.pack_start(&replay_row, false, false, 0);

        // forces
        let forces = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (key, text, f) in [
            (
                "centerStrength",
                "Pull toward centre",
                fmt2 as fn(f64) -> String,
            ),
            ("repelStrength", "Space between files", fmt1),
            ("linkStrength", "Connection tension", fmt2),
            ("linkDistance", "Preferred spacing", fmt0),
        ] {
            forces.pack_start(
                &add_slider(slider_row(key, text, None, f, &on_change, &syncing)),
                false,
                false,
                0,
            );
        }

        let bodies = [filters, groups, display, forces];
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let mut sections = Vec::new();
        for (i, ((id, key, title), body)) in SECTIONS.iter().zip(bodies).enumerate() {
            let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
            class(&section, "impro-graph-section");
            section.set_widget_name(&format!("impro-graph-section-{id}"));
            let head = gtk::EventBox::new();
            head.set_can_focus(true);
            head.add_events(gdk::EventMask::BUTTON_PRESS_MASK | gdk::EventMask::KEY_PRESS_MASK);
            class(&head, "impro-graph-section-event");
            if let Some(a) = head.accessible() {
                a.set_role(atk::Role::ToggleButton);
                a.set_name(title);
            }
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            class(&line, "impro-graph-section-head");
            if i == 0 {
                class(&line, "impro-graph-first");
            }
            let chevron = gtk::DrawingArea::new();
            chevron.set_size_request(16, 16);
            chevron.set_valign(gtk::Align::Center);
            let text = gtk::Label::new(Some(title));
            text.set_xalign(0.0);
            line.pack_start(&chevron, false, false, 0);
            line.pack_start(&text, true, true, 0);
            head.add(&line);
            let revealer = gtk::Revealer::new();
            revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
            revealer.set_transition_duration(100);
            class(&body, "impro-graph-section-body");
            revealer.add(&body);
            section.pack_start(&head, false, false, 0);
            section.pack_start(&revealer, false, false, 0);
            content.pack_start(&section, false, false, 0);
            sections.push(Section {
                key,
                head,
                chevron,
                revealer,
                collapsed: Cell::new(true),
            });
        }

        // --- chip and head buttons --------------------------------------------
        let chip = gtk::Box::new(gtk::Orientation::Vertical, 8);
        class(&chip, "impro-graph-chip");
        let settings_btn = icons::icon_button(Icon::Settings, "Adjust project graph", 16);
        let chip_type = icons::icon_button(Icon::GraphType, "Select graph type", 16);
        let wand = icons::icon_button(Icon::Wand, "Replay project history", 16);
        chip.pack_start(&settings_btn, false, false, 0);
        chip.pack_start(&chip_type, false, false, 0);
        chip.pack_start(&wand, false, false, 0);

        let head_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        class(&head_box, "impro-graph-small");
        let head_type = icons::icon_button(Icon::GraphType, "Select graph type", 16);
        let reset = icons::icon_button(Icon::Reset, "Reset graph preferences", 16);
        let close = icons::icon_button(Icon::Close, "Hide graph controls", 16);
        head_box.pack_start(&head_type, false, false, 0);
        head_box.pack_start(&reset, false, false, 0);
        head_box.pack_start(&close, false, false, 0);
        head_box.set_halign(gtk::Align::End);
        head_box.set_valign(gtk::Align::Start);
        head_box.set_margin_top(6);
        head_box.set_margin_end(8);

        let scroller = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_propagate_natural_height(true);
        scroller.set_overlay_scrolling(true);
        scroller.set_min_content_width(PANEL_WIDTH - 2);
        scroller.set_max_content_width(PANEL_WIDTH - 2);
        scroller.set_propagate_natural_width(false);
        scroller.add(&content);
        let inner = gtk::Overlay::new();
        inner.add(&scroller);
        inner.add_overlay(&head_box);
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        class(&frame, "impro-graph-panel");
        frame.set_size_request(PANEL_WIDTH, -1);
        frame.pack_start(&inner, true, true, 0);
        let panel_revealer = gtk::Revealer::new();
        panel_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
        panel_revealer.set_transition_duration(120);
        panel_revealer.add(&frame);
        panel_revealer.set_reveal_child(true);
        // A closed panel must take no room: the revealer keeps its full
        // width while faded out (crossfade), so only an explicit hide frees
        // the corner for the strip, and `show_all` of the host must not
        // bring it back.
        panel_revealer.show_all();
        panel_revealer.set_no_show_all(true);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        class(&root, "impro-graph-controls");
        root.set_halign(gtk::Align::End);
        root.set_valign(gtk::Align::Start);
        root.set_margin_top(12);
        root.set_margin_end(12);
        root.pack_start(&chip, false, false, 0);
        root.pack_start(&panel_revealer, false, false, 0);
        chip.set_no_show_all(true);
        chip.hide();

        let parts = Rc::new(Parts {
            scroller: scroller.clone(),
            replay_buttons: vec![replay.clone(), wand.clone()],
            chip,
            reset_button: reset.clone(),
            close_button: close.clone(),
            panel_revealer,
            sections,
            toggles,
            sliders,
            search: search.clone(),
            search_clear: search_clear.clone(),
            groups_box,
            rows: RefCell::new(Vec::new()),
            shown_rules: RefCell::new(Vec::new()),
            add_rule: add_rule.clone(),
            replay_row,
            wand: wand.clone(),
            type_buttons: vec![chip_type.clone(), head_type.clone()],
            presets: RefCell::new(Vec::new()),
            open: Cell::new(true),
            animated: Cell::new(true),
            popover: RefCell::new(None),
            type_menu: RefCell::new(None),
            graph_type: Cell::new(GraphType::Global),
            local: Cell::new(false),
        });
        let panel = Panel {
            root,
            parts,
            on_change,
            syncing,
        };
        panel.wire(
            settings_btn,
            close,
            reset,
            replay,
            wand,
            [chip_type, head_type],
        );
        panel
    }

    fn emit(&self, change: Change) {
        if !self.syncing.get() {
            (self.on_change)(change);
        }
    }

    fn wire(
        &self,
        open: gtk::Button,
        close: gtk::Button,
        reset: gtk::Button,
        replay: gtk::Button,
        wand: gtk::Button,
        types: [gtk::Button; 2],
    ) {
        let p = self.clone();
        open.connect_clicked(move |_| p.emit(Change::Open(true)));
        let p = self.clone();
        close.connect_clicked(move |_| p.emit(Change::Open(false)));
        let p = self.clone();
        reset.connect_clicked(move |_| p.emit(Change::Reset));
        for b in [replay, wand] {
            let p = self.clone();
            b.connect_clicked(move |_| p.emit(Change::Replay));
        }
        for b in types {
            let p = self.clone();
            b.connect_clicked(move |b| p.show_type_menu(b));
        }
        // sections
        for (i, section) in self.parts.sections.iter().enumerate() {
            let p = self.clone();
            section.head.connect_button_press_event(move |head, e| {
                if e.button() == 1 || e.button() == 2 {
                    head.grab_focus();
                    p.toggle_section(i);
                }
                glib::Propagation::Stop
            });
            let p = self.clone();
            section.head.connect_key_press_event(move |_, e| {
                let k = e.keyval();
                if k == gdk::keys::constants::Return
                    || k == gdk::keys::constants::KP_Enter
                    || k == gdk::keys::constants::space
                {
                    p.toggle_section(i);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
            let collapsed_of = {
                let p = self.clone();
                move || p.parts.sections[i].collapsed.get()
            };
            section.chevron.connect_draw(move |a, cr| {
                draw_chevron(a, collapsed_of(), cr);
                glib::Propagation::Proceed
            });
        }
        // search
        let p = self.clone();
        self.parts.search.connect_changed(move |e| {
            let text = e.text().to_string();
            p.parts.search_clear.set_visible(!text.is_empty());
            p.emit(Change::Search { text, now: false });
        });
        let p = self.clone();
        self.parts.search.connect_activate(move |e| {
            p.emit(Change::Search {
                text: e.text().to_string(),
                now: true,
            });
        });
        let p = self.clone();
        self.parts.search.connect_key_press_event(move |e, ev| {
            if ev.keyval() == gdk::keys::constants::Escape && !e.text().is_empty() {
                e.set_text("");
                p.emit(Change::Search {
                    text: String::new(),
                    now: true,
                });
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        let p = self.clone();
        self.parts.search_clear.connect_clicked(move |_| {
            p.parts.search.set_text("");
            p.emit(Change::Search {
                text: String::new(),
                now: true,
            });
            p.parts.search.grab_focus();
        });
        // groups
        let p = self.clone();
        self.parts.add_rule.connect_clicked(move |_| {
            p.emit(Change::AddRule);
            // focus the new rule's query once the rows exist
            let p = p.clone();
            glib::idle_add_local_once(move || {
                if let Some(last) = p.parts.rows.borrow().last() {
                    last.entry.grab_focus();
                }
            });
        });
        // panel fade
        let p = self.clone();
        self.parts
            .panel_revealer
            .connect_child_revealed_notify(move |r| {
                if !r.is_child_revealed() && !p.parts.open.get() {
                    // Outside the revealer's own animation step, so the
                    // corner is laid out again for the strip alone.
                    let (p, r) = (p.clone(), r.clone());
                    glib::idle_add_local_once(move || {
                        if !p.parts.open.get() && !r.is_child_revealed() {
                            r.hide();
                            Self::set_row_visible(&p.parts.chip, true);
                            p.root.queue_resize();
                            if let Some(parent) = p.root.parent() {
                                parent.queue_resize();
                            }
                        }
                    });
                }
            });
    }

    fn toggle_section(&self, index: usize) {
        let section = &self.parts.sections[index];
        let collapsed = !section.collapsed.get();
        self.set_collapsed(index, collapsed);
        self.emit(Change::Collapse {
            key: section.key,
            collapsed,
        });
    }

    fn set_collapsed(&self, index: usize, collapsed: bool) {
        let section = &self.parts.sections[index];
        section.collapsed.set(collapsed);
        section
            .revealer
            .set_transition_duration(if self.parts.animated.get() { 100 } else { 0 });
        section.revealer.set_reveal_child(!collapsed);
        section.chevron.queue_draw();
        if let Some(a) = section.head.accessible() {
            super::switch::notify_state(&a, atk::StateType::Expanded, !collapsed);
        }
    }

    // --- state from preferences ---------------------------------------------

    /// Shows `prefs` without emitting changes.
    pub fn sync(&self, prefs: &GraphPrefs, local: bool, graph_type: GraphType) {
        self.syncing.set(true);
        let p = &*self.parts;
        if p.search.text() != prefs.search {
            p.search.set_text(&prefs.search);
        }
        p.search_clear.set_visible(!prefs.search.is_empty());
        let toggles: [(&str, bool); 8] = [
            ("showTags", prefs.show_tags),
            ("showAttachments", prefs.show_attachments),
            ("hideUnresolved", prefs.hide_unresolved),
            ("showOrphans", prefs.show_orphans),
            ("showCode", prefs.show_code),
            ("showArrow", prefs.show_arrow),
            ("localIncoming", prefs.local_incoming),
            ("localOutgoing", prefs.local_outgoing),
        ];
        for (key, value) in toggles {
            if let Some(t) = p.toggles.get(key) {
                t.switch.set_active(value);
            }
        }
        let sliders: [(&str, f64); 8] = [
            ("textFadeMultiplier", prefs.text_fade_multiplier),
            ("nodeSizeMultiplier", prefs.node_size_multiplier),
            ("lineSizeMultiplier", prefs.line_size_multiplier),
            ("centerStrength", prefs.center_strength),
            ("repelStrength", prefs.repel_strength),
            ("linkStrength", prefs.link_strength),
            ("linkDistance", prefs.link_distance),
            ("localDepth", prefs.local_depth as f64),
        ];
        for (key, value) in sliders {
            if let Some(s) = p.sliders.get(key) {
                if (s.scale.value() - value).abs() > 1e-9 {
                    s.scale.set_value(value);
                }
                s.value.set_text(&(s.format)(value));
            }
        }
        let collapsed = [
            prefs.collapse_filter,
            prefs.collapse_color_groups,
            prefs.collapse_display,
            prefs.collapse_forces,
        ];
        for (i, c) in collapsed.into_iter().enumerate() {
            if p.sections[i].collapsed.get() != c
                || !p.sections[i].revealer.is_child_revealed() == c
            {
                self.set_collapsed(i, c);
            }
        }
        self.show_rules(&prefs.color_groups);
        self.set_local(local);
        self.set_graph_type(graph_type);
        self.syncing.set(false);
    }

    /// Local mode swaps the unlinked-files switch and the replay button for
    /// the traversal controls.
    pub fn set_local(&self, local: bool) {
        let p = &*self.parts;
        p.local.set(local);
        for key in ["localDepth", "localIncoming", "localOutgoing"] {
            let row = p
                .sliders
                .get(key)
                .map(|s| s.row.clone())
                .or_else(|| p.toggles.get(key).map(|t| t.row.clone()));
            if let Some(row) = row {
                Self::set_row_visible(&row, local);
            }
        }
        for row in [
            p.toggles.get("showOrphans").map(|t| &t.row),
            Some(&p.replay_row),
        ]
        .into_iter()
        .flatten()
        {
            Self::set_row_visible(row, !local);
        }
        Self::set_row_visible(&p.wand, !local);
    }

    /// Shows or hides a row that `show_all` must not touch.
    fn set_row_visible(row: &impl IsA<gtk::Widget>, visible: bool) {
        row.set_no_show_all(false);
        row.show_all();
        row.set_no_show_all(true);
        row.set_visible(visible);
    }

    pub fn set_graph_type(&self, t: GraphType) {
        self.parts.graph_type.set(t);
        for b in &self.parts.type_buttons {
            let ctx = b.style_context();
            if t == GraphType::Global {
                ctx.remove_class("impro-graph-active");
            } else {
                ctx.add_class("impro-graph-active");
            }
        }
    }

    /// Reduced motion: no sliding sections, no fading panel, instant switches.
    pub fn set_animated(&self, animated: bool) {
        let p = &*self.parts;
        p.animated.set(animated);
        p.panel_revealer
            .set_transition_duration(if animated { 120 } else { 0 });
        for s in p.toggles.values() {
            s.switch.set_animated(animated);
        }
        for s in &p.sections {
            s.revealer
                .set_transition_duration(if animated { 100 } else { 0 });
        }
    }

    /// Opens or closes the panel (closed: the three-button chip).
    pub fn set_open(&self, open: bool) {
        let p = &*self.parts;
        if p.open.get() == open && p.panel_revealer.is_visible() == open {
            return;
        }
        p.open.set(open);
        if open {
            p.chip.hide();
            Self::set_row_visible(&p.panel_revealer, true);
            p.panel_revealer.set_reveal_child(true);
        } else {
            p.panel_revealer.set_reveal_child(false);
            if !p.animated.get() || !p.panel_revealer.is_mapped() {
                p.panel_revealer.hide();
                Self::set_row_visible(&p.chip, true);
            }
        }
    }

    /// Tallest the panel may grow before it scrolls.
    pub fn set_max_height(&self, height: i32) {
        self.parts.scroller.set_max_content_height(height.max(120));
    }

    /// Marks the replay buttons while the history plays.
    pub fn set_replaying(&self, replaying: bool) {
        for b in &self.parts.replay_buttons {
            let ctx = b.style_context();
            if replaying {
                ctx.add_class("impro-graph-active");
            } else {
                ctx.remove_class("impro-graph-active");
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.parts.open.get()
    }

    /// Palette used by the colour-rule popover (the legible accent presets).
    pub fn set_presets(&self, presets: Vec<Rgba>) {
        *self.parts.presets.borrow_mut() = presets;
    }

    // --- colour rules -------------------------------------------------------

    fn show_rules(&self, rules: &[ColorRule]) {
        let p = &*self.parts;
        if *p.shown_rules.borrow() == rules {
            return;
        }
        let same_count = p.rows.borrow().len() == rules.len();
        if same_count {
            for (row, rule) in p.rows.borrow().iter().zip(rules) {
                if row.entry.text() != rule.query {
                    row.entry.set_text(&rule.query);
                }
                let (r, g, b, a) = rule.color.rgba();
                row.color.set(Rgba::from_u8(r, g, b, a as f32));
                row.swatch.queue_draw();
            }
        } else {
            for row in p.rows.borrow_mut().drain(..) {
                p.groups_box.remove(&row.row);
            }
            for (i, rule) in rules.iter().enumerate() {
                let row = self.group_row(i, rule);
                p.groups_box.pack_start(&row.row, false, false, 0);
                row.row.show_all();
                p.rows.borrow_mut().push(row);
            }
        }
        p.add_rule.set_sensitive(rules.len() < MAX_COLOR_RULES);
        *p.shown_rules.borrow_mut() = rules.to_vec();
    }

    fn group_row(&self, index: usize, rule: &ColorRule) -> GroupRow {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        class(&row, "impro-graph-group");
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("Match files with a query…"));
        entry.set_hexpand(true);
        entry.set_width_chars(1);
        class(&entry, "impro-graph-entry");
        if let Some(a) = entry.accessible() {
            a.set_name("Colour rule query");
        }
        entry.set_text(&rule.query);
        let (r, g, b, a) = rule.color.rgba();
        let color = Rc::new(Cell::new(Rgba::from_u8(r, g, b, a as f32)));
        let swatch = swatch_button(color.clone());
        swatch.set_tooltip_text(Some("Choose a rule colour"));
        if let Some(a) = swatch.accessible() {
            a.set_name("Rule colour");
        }
        let remove = icons::icon_button(Icon::Close, "Remove colour rule", 14);
        class(&row, "impro-graph-small");
        row.pack_start(&entry, true, true, 0);
        row.pack_start(&swatch, false, false, 0);
        row.pack_start(&remove, false, false, 0);
        let p = self.clone();
        entry.connect_changed(move |e| p.emit(Change::RuleQuery(index, e.text().to_string())));
        let p = self.clone();
        remove.connect_clicked(move |_| p.emit(Change::RemoveRule(index)));
        let p = self.clone();
        swatch.connect_clicked(move |b| p.show_swatches(index, b));
        GroupRow {
            row,
            entry,
            swatch,
            color,
        }
    }

    fn show_swatches(&self, index: usize, anchor: &gtk::Button) {
        let old = self.parts.popover.borrow_mut().take();
        if let Some(old) = old {
            old.popdown();
        }
        let pop = gtk::Popover::new(Some(anchor));
        class(&pop, "impro-graph-popover");
        pop.set_modal(true);
        let grid = gtk::Grid::new();
        grid.set_column_spacing(6);
        grid.set_row_spacing(6);
        grid.set_margin(8);
        for (n, c) in self.parts.presets.borrow().iter().enumerate() {
            let color = Rc::new(Cell::new(*c));
            let b = swatch_button(color);
            if let Some(a) = b.accessible() {
                a.set_name(&c.to_hex());
            }
            let p = self.clone();
            let rgb = u32::from_str_radix(c.to_hex().trim_start_matches('#'), 16).unwrap_or(0);
            let popover = pop.clone();
            b.connect_clicked(move |_| {
                p.emit(Change::RuleColor(index, rgb));
                popover.popdown();
            });
            grid.attach(&b, (n % 6) as i32, (n / 6) as i32, 1, 1);
        }
        let custom = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        class(&custom, "impro-graph-custom");
        custom.pack_start(&label("Pick a colour", "impro-graph-custom"), true, true, 0);
        let chooser = gtk::ColorButton::new();
        chooser.set_use_alpha(false);
        if let Some(c) = self.parts.rows.borrow().get(index).map(|r| r.color.get()) {
            chooser.set_rgba(&gdk::RGBA::new(c.r as f64, c.g as f64, c.b as f64, 1.0));
        }
        if let Some(a) = chooser.accessible() {
            a.set_name("Choose any rule colour");
        }
        let p = self.clone();
        chooser.connect_color_set(move |c| {
            let rgba = c.rgba();
            let ch = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u32;
            p.emit(Change::RuleColor(
                index,
                ch(rgba.red()) << 16 | ch(rgba.green()) << 8 | ch(rgba.blue()),
            ));
        });
        custom.pack_end(&chooser, false, false, 0);
        grid.attach(&custom, 0, 2, 6, 1);
        pop.add(&grid);
        grid.show_all();
        pop.popup();
        *self.parts.popover.borrow_mut() = Some(pop);
    }

    fn show_type_menu(&self, anchor: &gtk::Button) {
        let old = self.parts.type_menu.borrow_mut().take();
        if let Some(old) = old {
            old.popdown();
        }
        let menu = super::menu::graph_type_menu(self.parts.graph_type.get(), {
            let p = self.clone();
            move |t| p.emit(Change::GraphType(t))
        });
        menu.popup_at_widget(
            anchor,
            gdk::Gravity::SouthEast,
            gdk::Gravity::NorthEast,
            None,
        );
        *self.parts.type_menu.borrow_mut() = Some(menu);
    }

    // --- inspection (tests and host) ------------------------------------------

    /// Closes any popover the panel opened.
    pub fn close_popovers(&self) {
        // popping down emits signals: release each slot first
        let popover = self.parts.popover.borrow_mut().take();
        if let Some(p) = popover {
            p.popdown();
        }
        let menu = self.parts.type_menu.borrow_mut().take();
        if let Some(m) = menu {
            m.popdown();
        }
    }

    pub fn search_entry(&self) -> &gtk::Entry {
        &self.parts.search
    }

    /// The switch with a preference key such as `"showTags"`.
    pub fn switch(&self, key: &str) -> Option<&Switch> {
        self.parts.toggles.get(key).map(|t| &t.switch)
    }

    /// The slider with a preference key such as `"linkDistance"`.
    pub fn scale(&self, key: &str) -> Option<&gtk::Scale> {
        self.parts.sliders.get(key).map(|s| &s.scale)
    }

    /// The text shown beside a slider.
    pub fn slider_text(&self, key: &str) -> Option<String> {
        self.parts
            .sliders
            .get(key)
            .map(|s| s.value.text().to_string())
    }

    pub fn step_of(&self, key: &str) -> Option<f64> {
        self.parts.sliders.get(key).and_then(|s| s.step)
    }

    /// Whether a row is shown (local-mode rows hide in the global graph).
    pub fn row_visible(&self, key: &str) -> Option<bool> {
        self.parts
            .toggles
            .get(key)
            .map(|t| t.row.is_visible())
            .or_else(|| self.parts.sliders.get(key).map(|s| s.row.is_visible()))
    }

    /// Whether section `id` ("filters", "groups", "display", "forces") is collapsed.
    pub fn collapsed(&self, id: &str) -> Option<bool> {
        SECTIONS
            .iter()
            .position(|s| s.0 == id)
            .map(|i| self.parts.sections[i].collapsed.get())
    }

    /// Clicks a section header as the user would.
    pub fn click_section(&self, id: &str) {
        if let Some(i) = SECTIONS.iter().position(|s| s.0 == id) {
            self.toggle_section(i);
        }
    }

    pub fn section_head(&self, id: &str) -> Option<&gtk::EventBox> {
        SECTIONS
            .iter()
            .position(|s| s.0 == id)
            .map(|i| &self.parts.sections[i].head)
    }

    pub fn add_rule_button(&self) -> &gtk::Button {
        &self.parts.add_rule
    }

    pub fn replay_visible(&self) -> bool {
        self.parts.replay_row.is_visible()
    }

    /// Entries of the colour rules, in order.
    pub fn rule_entries(&self) -> Vec<gtk::Entry> {
        self.parts
            .rows
            .borrow()
            .iter()
            .map(|r| r.entry.clone())
            .collect()
    }

    pub fn rule_swatches(&self) -> Vec<gtk::Button> {
        self.parts
            .rows
            .borrow()
            .iter()
            .map(|r| r.swatch.clone())
            .collect()
    }

    /// "Reset graph preferences".
    pub fn reset_button(&self) -> &gtk::Button {
        &self.parts.reset_button
    }

    /// "Hide graph controls".
    pub fn close_button(&self) -> &gtk::Button {
        &self.parts.close_button
    }

    /// The three-button strip shown while the panel is closed.
    pub fn chip_visible(&self) -> bool {
        self.parts.chip.is_visible()
    }

    /// The panel's revealer is shown (open, or fading out).
    pub fn revealer_shown(&self) -> bool {
        self.parts.panel_revealer.is_visible()
    }

    /// The root of the controls (strip or panel), as placed on the canvas.
    pub fn controls_root(&self) -> &gtk::Box {
        &self.root
    }

    /// The three buttons of the closed strip, top to bottom.
    pub fn chip_buttons(&self) -> Vec<gtk::Widget> {
        self.parts.chip.children()
    }

    /// The strip itself.
    pub fn chip(&self) -> &gtk::Box {
        &self.parts.chip
    }

    /// Title text of a section header.
    pub fn section_title(&self, id: &str) -> Option<String> {
        SECTIONS.iter().find(|s| s.0 == id).map(|s| s.2.to_string())
    }

    pub fn is_local(&self) -> bool {
        self.parts.local.get()
    }
}
