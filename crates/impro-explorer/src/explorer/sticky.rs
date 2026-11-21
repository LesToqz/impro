//! Sticky folders, as in Zed's project panel: while the tree is scrolled into a folder, the
//! folders holding the first visible row stay pinned at the top, one row each, with a soft
//! shadow under the last. When a folder's last row scrolls past, its pinned row is pushed up
//! with it. A click on a pinned folder scrolls the tree back to that folder.
//!
//! The rows are painted on a drawing area laid over the tree's scrolled window, from the same
//! glyphs, text and indent guides as the tree.
use super::*;
use gtk::cairo;

/// A folder pinned at the top of the tree.
#[derive(Clone, Debug)]
pub(crate) struct StickyRow {
    pub path: gtk::TreePath,
    pub name: String,
    pub file: PathBuf,
}

/// What the overlay shows: the pinned rows, the height of one row, and how far the last one
/// is pushed up (0 or negative).
#[derive(Clone, Debug, Default)]
pub(crate) struct Sticky {
    pub rows: Vec<StickyRow>,
    pub row_height: i32,
    pub drift: i32,
    pub hovered: Option<usize>,
}

/// Height of the shadow under the last pinned row.
const SHADOW: i32 = 6;

impl ExplorerPanel {
    /// The overlay drawing area, hooked to the tree's scrolling and expansion.
    pub(super) fn build_sticky(&self) -> gtk::DrawingArea {
        let area = gtk::DrawingArea::new();
        area.style_context().add_class("impro-explorer-sticky");
        area.set_valign(gtk::Align::Start);
        area.set_halign(gtk::Align::Fill);
        area.set_no_show_all(true);
        area.add_events(
            gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::POINTER_MOTION_MASK
                | gdk::EventMask::LEAVE_NOTIFY_MASK
                | gdk::EventMask::SCROLL_MASK
                | gdk::EventMask::SMOOTH_SCROLL_MASK,
        );
        let weak = self.downgrade();
        area.connect_draw(move |area, cr| {
            if let Some(panel) = weak.upgrade() {
                panel.paint_sticky(area, cr);
            }
            glib::Propagation::Stop
        });
        let weak = self.downgrade();
        area.connect_button_press_event(move |_, event| {
            if let Some(panel) = weak.upgrade() {
                if event.button() == 1 && event.event_type() == gdk::EventType::ButtonPress {
                    panel.sticky_clicked(event.position().1);
                }
            }
            glib::Propagation::Stop
        });
        let weak = self.downgrade();
        area.connect_motion_notify_event(move |area, event| {
            if let Some(panel) = weak.upgrade() {
                let index = panel.sticky_index_at(event.position().1);
                let changed = {
                    let mut sticky = panel.imp().sticky.borrow_mut();
                    std::mem::replace(&mut sticky.hovered, index) != index
                };
                if changed {
                    area.queue_draw();
                }
            }
            glib::Propagation::Proceed
        });
        let weak = self.downgrade();
        area.connect_leave_notify_event(move |area, _| {
            if let Some(panel) = weak.upgrade() {
                panel.imp().sticky.borrow_mut().hovered = None;
                area.queue_draw();
            }
            glib::Propagation::Proceed
        });
        // The overlay sits over the scrolled window, not inside it: scroll it by hand.
        let weak = self.downgrade();
        area.connect_scroll_event(move |_, event| {
            if let Some(panel) = weak.upgrade() {
                panel.scroll_tree_by(event);
            }
            glib::Propagation::Stop
        });
        area
    }

    /// Follow scrolling, resizing and expansion.
    pub(super) fn connect_sticky(&self) {
        let w = self.w();
        let weak = self.downgrade();
        w.scroll.vadjustment().connect_value_changed(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.update_sticky();
            }
        });
        let weak = self.downgrade();
        w.scroll.vadjustment().connect_changed(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.update_sticky();
            }
        });
        let weak = self.downgrade();
        w.tree.connect_row_collapsed(move |_, _, _| {
            if let Some(panel) = weak.upgrade() {
                panel.update_sticky();
            }
        });
        let weak = self.downgrade();
        w.tree.connect_row_expanded(move |_, _, _| {
            if let Some(panel) = weak.upgrade() {
                panel.update_sticky();
            }
        });
    }

    /// Recompute the pinned rows (Zed's sticky anchor) and resize the overlay.
    pub(super) fn update_sticky(&self) {
        if self.is_destroyed() {
            return;
        }
        let w = self.w();
        let rows = self.compute_sticky();
        let area = &w.sticky;
        let height = if rows.rows.is_empty() {
            0
        } else {
            rows.row_height * rows.rows.len() as i32 + rows.drift + impro_ui::scale::px(SHADOW)
        };
        let hovered = if height > 0 {
            self.imp().sticky.borrow().hovered
        } else {
            None
        };
        let rows_height = (rows.row_height * rows.rows.len() as i32 + rows.drift).max(0);
        *self.imp().sticky.borrow_mut() = Sticky { hovered, ..rows };
        if height <= 0 {
            if area.is_visible() {
                area.hide();
            }
            return;
        }
        if area.height_request() != height {
            area.set_size_request(-1, height);
        }
        if !area.is_visible() {
            area.show();
        }
        // Only the pinned rows take the pointer; the shadow under them lets clicks through to
        // the row it overlaps.
        let input = gtk::cairo::RectangleInt::new(0, 0, i32::MAX / 2, rows_height);
        area.input_shape_combine_region(Some(&gtk::cairo::Region::create_rectangle(&input)));
        area.queue_draw();
    }

    /// Zed's `find_sticky_anchor`: walk the rows from the top of the view; the first row whose
    /// depth is below its position, or that is the last row of a folder at the right depth,
    /// anchors the pinned rows (its folders). In the second case the last pinned row drifts:
    /// it is pushed up with the anchor's bottom edge.
    fn compute_sticky(&self) -> Sticky {
        let w = self.w();
        let tree = &w.tree;
        let empty = Sticky::default();
        if !tree.is_mapped() {
            return empty;
        }
        let Some(column) = tree.column(0) else {
            return empty;
        };
        let Some((first, _)) = tree.visible_range() else {
            return empty;
        };
        let first_area = tree.background_area(Some(&first), Some(&column));
        let height = first_area.height();
        // Nothing scrolled away: nothing to pin.
        if height <= 0 || w.scroll.vadjustment().value() <= 0.0 {
            return empty;
        }
        let depth = |path: &gtk::TreePath| path.depth().saturating_sub(1) as usize;
        let mut rows: Vec<gtk::TreePath> = vec![first];
        let mut anchor: Option<(usize, bool)> = None;
        let mut ix = 0;
        while ix < 64 {
            let current = rows[ix].clone();
            let next = self.next_visible_row(&current);
            let d = depth(&current);
            if d < ix {
                anchor = Some((ix, false));
                break;
            }
            let Some(next) = next else { break };
            let outdented = depth(&next) + 1 == d;
            if outdented && (d == ix || d == ix + 1) {
                anchor = Some((ix, d == ix + 1));
                break;
            }
            rows.push(next);
            ix += 1;
        }
        let Some((index, drifting)) = anchor else {
            return empty;
        };
        let anchor_path = rows[index].clone();
        // The anchor's folders, outermost first.
        let mut folders = Vec::new();
        let mut parent = anchor_path.clone();
        while parent.depth() > 1 && parent.up() {
            folders.push(parent.clone());
        }
        folders.reverse();
        if folders.is_empty() {
            return empty;
        }
        let drift = if drifting {
            let area = tree.background_area(Some(&anchor_path), Some(&column));
            let bottom = area.y() + area.height();
            (bottom - height * folders.len() as i32).min(0)
        } else {
            0
        };
        let rows = folders
            .into_iter()
            .filter_map(|path| {
                let iter = w.store.iter(&path)?;
                Some(StickyRow {
                    name: util::string(&w.store, &iter, 1),
                    file: util::decode_path(&util::string(&w.store, &iter, 2)),
                    path,
                })
            })
            .collect();
        Sticky {
            rows,
            row_height: height,
            drift,
            hovered: None,
        }
    }

    /// The folders pinned over the tree, outermost first (test seam).
    #[doc(hidden)]
    pub fn sticky_folders(&self) -> Vec<PathBuf> {
        self.imp()
            .sticky
            .borrow()
            .rows
            .iter()
            .map(|row| row.file.clone())
            .collect()
    }

    /// Top of pinned row `index` in the overlay.
    fn sticky_top(sticky: &Sticky, index: usize) -> i32 {
        let top = sticky.row_height * index as i32;
        if index + 1 == sticky.rows.len() {
            top + sticky.drift
        } else {
            top
        }
    }

    fn sticky_index_at(&self, y: f64) -> Option<usize> {
        let sticky = self.imp().sticky.borrow();
        (0..sticky.rows.len()).rev().find(|&index| {
            let top = f64::from(Self::sticky_top(&sticky, index));
            y >= top && y < top + f64::from(sticky.row_height)
        })
    }

    /// Scroll so the clicked folder's row sits just below where it was pinned.
    fn sticky_clicked(&self, y: f64) {
        let Some(index) = self.sticky_index_at(y) else {
            return;
        };
        let (path, height) = {
            let sticky = self.imp().sticky.borrow();
            (sticky.rows[index].path.clone(), sticky.row_height)
        };
        let w = self.w();
        let Some(column) = w.tree.column(0) else {
            return;
        };
        let adjustment = w.scroll.vadjustment();
        let area = w.tree.background_area(Some(&path), Some(&column));
        let top = adjustment.value() + f64::from(area.y());
        let value = top - f64::from(height * index as i32) - 1.0;
        let max = adjustment.upper() - adjustment.page_size();
        adjustment.set_value(value.clamp(adjustment.lower(), max.max(adjustment.lower())));
    }

    /// Apply a wheel or touchpad scroll over the overlay to the tree.
    fn scroll_tree_by(&self, event: &gdk::EventScroll) {
        let adjustment = self.w().scroll.vadjustment();
        let step = adjustment.page_size().powf(2.0 / 3.0);
        let delta = match event.direction() {
            gdk::ScrollDirection::Up => -step,
            gdk::ScrollDirection::Down => step,
            gdk::ScrollDirection::Smooth => event.delta().1 * step,
            _ => 0.0,
        };
        if delta != 0.0 {
            let max = adjustment.upper() - adjustment.page_size();
            let value = (adjustment.value() + delta).clamp(adjustment.lower(), max.max(0.0));
            adjustment.set_value(value);
        }
    }

    fn paint_sticky(&self, area: &gtk::DrawingArea, cr: &cairo::Context) {
        let sticky = self.imp().sticky.borrow().clone();
        if sticky.rows.is_empty() {
            return;
        }
        let w = self.w();
        let tree = &w.tree;
        let context = tree.style_context();
        let lookup = |name: &str| context.lookup_color(name);
        let background = lookup("impro_explorer_sticky_bg");
        let hover = lookup("impro_explorer_sticky_hover");
        let guide = lookup("impro_indent_guide");
        let foreground = context.color(gtk::StateFlags::NORMAL);
        let width = f64::from(area.allocated_width());
        let height = sticky.row_height;
        let indent = tree.level_indentation();
        let x_scroll = w.scroll.hadjustment().value();
        let mut description = tree.pango_context().font_description().unwrap_or_default();
        description.set_size((self.font_size() * f64::from(pango::SCALE)) as i32);
        let source = |color: &gdk::RGBA| {
            cr.set_source_rgba(color.red(), color.green(), color.blue(), color.alpha());
        };
        // Paint the drifting (last) row first so the rows above cover it as it slides up.
        for index in (0..sticky.rows.len()).rev() {
            let row = &sticky.rows[index];
            let top = f64::from(Self::sticky_top(&sticky, index));
            let h = f64::from(height);
            if let Some(color) = &background {
                source(color);
                cr.rectangle(0.0, top, width, h);
                let _ = cr.fill();
            }
            if sticky.hovered == Some(index) {
                if let Some(color) = &hover {
                    source(color);
                    cr.rectangle(0.0, top, width, h);
                    let _ = cr.fill();
                }
            }
            let depth = row.path.depth().saturating_sub(1);
            let x0 = -x_scroll + f64::from(indent * depth);
            // Guides of the levels above this folder.
            if let Some(color) = &guide {
                source(color);
                for level in 0..depth {
                    let x =
                        (-x_scroll).round() + f64::from(level * indent + impro_ui::scale::px(15));
                    cr.rectangle(x, top, 1.0, h);
                    let _ = cr.fill();
                }
            }
            let icon = crate::icons::glyph_size();
            let icon_x = x0 + f64::from(crate::icons::glyph_inset());
            let icon_color = lookup("impro_explorer_icon").unwrap_or(foreground);
            impro_ui::glyphs::draw(
                cr,
                "folder-open",
                icon_x.round(),
                (top + (h - f64::from(icon)) / 2.0).floor(),
                f64::from(icon),
                &icon_color,
            );
            let layout = area.create_pango_layout(Some(&row.name));
            layout.set_font_description(Some(&description));
            let (_, logical) = layout.pixel_extents();
            let text_x = icon_x + f64::from(icon + impro_ui::scale::px(6));
            let text_y = top + ((h - f64::from(logical.height())) / 2.0).floor();
            let colour = self.git_colour(&row.file, true).unwrap_or(foreground);
            source(&colour);
            cr.move_to(text_x.round(), text_y);
            pangocairo::functions::show_layout(cr, &layout);
        }
        // Shadow under the last row.
        let last = sticky.rows.len() - 1;
        let bottom = f64::from(Self::sticky_top(&sticky, last) + height);
        let shadow = f64::from(impro_ui::scale::px(SHADOW));
        let gradient = cairo::LinearGradient::new(0.0, bottom, 0.0, bottom + shadow);
        gradient.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.10);
        gradient.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
        let _ = cr.set_source(&gradient);
        cr.rectangle(0.0, bottom, width, shadow);
        let _ = cr.fill();
    }
}
