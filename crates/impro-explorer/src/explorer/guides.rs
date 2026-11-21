//! Row decorations painted over the tree after GTK draws it, as in Zed's project panel:
//! indent guides (the guide of the selected entry's folder brighter) and a border around the
//! cursor row while the tree has the keyboard focus.
use super::*;
use gtk::cairo;

/// Indentation per level in logical pixels (Zed's `indent_size`).
pub(crate) const INDENT: i32 = 20;
/// Left edge of a row to the centre of its icon: 1 px border, 6 px padding, half a 16 px icon.
const GUIDE_OFFSET: i32 = 15;
/// A guide stops this short of the first and the last row it runs along.
const GUIDE_PAD: i32 = 4;

fn px(value: i32) -> i32 {
    impro_ui::scale::px(value)
}

fn set_source(cr: &cairo::Context, color: &gdk::RGBA) {
    cr.set_source_rgba(color.red(), color.green(), color.blue(), color.alpha());
}

impl ExplorerPanel {
    /// Paint the indent guides and the focus border (an after-handler of the tree's `draw`).
    pub(super) fn paint_decorations(&self, cr: &cairo::Context) {
        if self.is_destroyed() {
            return;
        }
        let w = self.w();
        let tree = &w.tree;
        let Some(bin) = tree.bin_window() else {
            return;
        };
        if !gtk::cairo_should_draw_window(cr, &bin) {
            return;
        }
        let (Some(column), Some((first, last))) = (tree.column(0), tree.visible_range()) else {
            return;
        };
        let context = tree.style_context();
        let guide = context.lookup_color("impro_indent_guide");
        let active = context.lookup_color("impro_indent_guide_active");
        let indent = tree.level_indentation();
        let (offset, pad) = (px(GUIDE_OFFSET), px(GUIDE_PAD));
        let active_parent = self.active_guide_parent().map(|p| p.indices());
        let _ = cr.save();
        let mut path = first;
        loop {
            let area = tree.background_area(Some(&path), Some(&column));
            let (x0, y0) = tree.convert_bin_window_to_widget_coords(area.x(), area.y());
            let indices = path.indices();
            // Zed's depth: the root is 0.
            let depth = indices.len().saturating_sub(1);
            if depth > 0 {
                let lasts = self.last_child_flags(&indices);
                let opens_below = tree.row_expanded(&path)
                    && w.store
                        .iter(&path)
                        .is_some_and(|iter| w.store.iter_has_child(&iter));
                for level in 0..depth {
                    let x = x0 + level as i32 * indent + offset;
                    let mut top = y0;
                    let mut bottom = y0 + area.height();
                    // First row of the guide: the first child of the folder at `level`.
                    if depth == level + 1 && indices[level + 1] == 0 {
                        top += pad;
                    }
                    // Last row: every folder below `level` on the way here is its parent's
                    // last child, and nothing opens below this row.
                    if !opens_below && lasts[level + 1..=depth].iter().all(|last| *last) {
                        bottom -= pad;
                    }
                    let is_active = active_parent.as_ref().is_some_and(|parent| {
                        parent.len() == level + 1 && indices.starts_with(parent)
                    });
                    let color = if is_active { &active } else { &guide };
                    if let Some(color) = color {
                        if bottom > top {
                            set_source(cr, color);
                            cr.rectangle(
                                f64::from(x),
                                f64::from(top),
                                1.0,
                                f64::from(bottom - top),
                            );
                            let _ = cr.fill();
                        }
                    }
                }
            }
            if path == last {
                break;
            }
            match self.next_visible_row(&path) {
                Some(next) => path = next,
                None => break,
            }
        }
        self.paint_focus_border(cr, &column);
        let _ = cr.restore();
    }

    /// While the tree has the focus, the cursor row is outlined (1 px, 2 px on the right).
    fn paint_focus_border(&self, cr: &cairo::Context, column: &gtk::TreeViewColumn) {
        let tree = &self.w().tree;
        if !tree.has_focus() || self.imp().mouse_down.get() {
            return;
        }
        let (Some(path), _) = tree.cursor() else {
            return;
        };
        let Some(color) = tree.style_context().lookup_color("impro_explorer_focus") else {
            return;
        };
        let area = tree.background_area(Some(&path), Some(column));
        let (_, y) = tree.convert_bin_window_to_widget_coords(area.x(), area.y());
        let width = f64::from(tree.allocated_width());
        let (y, height) = (f64::from(y), f64::from(area.height()));
        set_source(cr, &color);
        cr.set_line_width(1.0);
        cr.rectangle(0.5, y + 0.5, width - 1.0, height - 1.0);
        let _ = cr.stroke();
        cr.rectangle(width - 2.0, y, 2.0, height);
        let _ = cr.fill();
    }

    /// For each level `k` of `indices` (1..), whether the row at that level is the last child of
    /// its parent. Index 0 (the root) is unused.
    fn last_child_flags(&self, indices: &[i32]) -> Vec<bool> {
        let store = &self.w().store;
        let mut flags = vec![false; indices.len()];
        for (level, flag) in flags.iter_mut().enumerate().skip(1) {
            let parent = gtk::TreePath::from_indicesv(&indices[..level]);
            if let Some(parent) = store.iter(&parent) {
                *flag = indices[level] + 1 >= store.iter_n_children(Some(&parent));
            }
        }
        flags
    }

    /// The row after `path` in display order (rows under collapsed folders are skipped).
    pub(super) fn next_visible_row(&self, path: &gtk::TreePath) -> Option<gtk::TreePath> {
        let w = self.w();
        let iter = w.store.iter(path)?;
        if w.tree.row_expanded(path) && w.store.iter_has_child(&iter) {
            let mut child = path.clone();
            child.down();
            return Some(child);
        }
        let mut row = path.clone();
        loop {
            let iter = w.store.iter(&row)?;
            // `util::next_iter`, not `iter_next`: see util.rs.
            if util::next_iter(&w.store, &iter).is_some() {
                row.next();
                return Some(row);
            }
            if row.depth() <= 1 || !row.up() {
                return None;
            }
        }
    }

    /// The folder whose guide is highlighted: the selected row when it is an expanded folder,
    /// otherwise the folder holding it.
    pub(super) fn active_guide_parent(&self) -> Option<gtk::TreePath> {
        let w = self.w();
        let (_, iter) = w.tree.selection().selected()?;
        let mut path = w.store.path(&iter)?;
        if util::string(&w.store, &iter, 0) == "folder" && w.tree.row_expanded(&path) {
            return Some(path);
        }
        (path.depth() > 1 && path.up()).then_some(path)
    }
}
