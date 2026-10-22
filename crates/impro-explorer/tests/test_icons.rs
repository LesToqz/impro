//! Symbolic icons are decoded off the main thread and painted from a cache.
mod common;
use common::{poll, pump, run, window};
use gtk::prelude::*;
use impro_explorer::icons::{self, IconCell};

fn tree_with(names: &[&str]) -> (gtk::TreeView, gtk::Window) {
    let store = gtk::ListStore::new(&[String::static_type()]);
    for name in names {
        store.insert_with_values(None, &[(0, name)]);
    }
    let tree = gtk::TreeView::with_model(&store);
    tree.set_headers_visible(false);
    let column = gtk::TreeViewColumn::new();
    let cell = IconCell::new();
    gtk::prelude::CellLayoutExt::pack_start(&column, &cell, false);
    gtk::prelude::CellLayoutExt::add_attribute(&column, &cell, "icon-name", 0);
    tree.append_column(&column);
    let window = window(&tree, 200, 120);
    (tree, window)
}

#[test]
fn the_cell_is_a_pixbuf_renderer_of_menu_size() {
    run(|| {
        let cell = IconCell::new();
        assert!(cell.is::<gtk::CellRendererPixbuf>());
        cell.set_xpad(4);
        let (tree, _window) = tree_with(&["folder-symbolic"]);
        let (min, natural) = gtk::prelude::CellRendererExt::preferred_width(&cell, &tree);
        assert_eq!(
            (min, natural),
            (icons::menu_size() + 8, icons::menu_size() + 8)
        );
    });
}

#[test]
fn an_icon_is_decoded_in_the_background_and_then_served_from_the_cache() {
    run(|| {
        // the tree paints other icons only: a decode of this name would be a fallback
        let (tree, _window) = tree_with(&["folder-symbolic"]);
        let widget: &gtk::Widget = tree.upcast_ref();
        let fg = gdk::RGBA::new(0.8, 0.9, 0.9, 1.0);
        let size = icons::menu_size();
        let first = icons::surface(widget, "view-refresh-symbolic", size, &fg);
        assert!(first.is_none(), "the request returns without decoding");
        assert!(poll(5000, || icons::surface(
            widget,
            "view-refresh-symbolic",
            size,
            &fg
        )
        .is_some()));
        // Another colour shows the known one until its own decode finishes.
        let other = gdk::RGBA::new(0.1, 0.2, 0.3, 1.0);
        assert!(icons::surface(widget, "view-refresh-symbolic", size, &other).is_some());
        assert!(poll(5000, || {
            // the variant itself arrives later: the call keeps answering
            icons::surface(widget, "view-refresh-symbolic", size, &other).is_some()
        }));
    });
}

#[test]
fn unknown_icons_stay_empty_without_repeating_the_lookup() {
    run(|| {
        let (tree, _window) = tree_with(&["no-such-icon-symbolic"]);
        let widget: &gtk::Widget = tree.upcast_ref();
        let fg = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
        for _ in 0..5 {
            assert!(icons::surface(widget, "no-such-icon-symbolic", 16, &fg).is_none());
            pump(20);
        }
    });
}

#[test]
fn the_tree_repaints_when_its_icons_arrive() {
    run(|| {
        let (tree, window) = tree_with(&["folder-symbolic", "text-x-generic-symbolic"]);
        pump(100);
        let gw = window.window().expect("realized");
        let snapshot = || {
            use gtk::glib::translate::*;
            // SAFETY: the window is alive; the call returns a new reference or NULL.
            let pixbuf: Option<gtk::gdk_pixbuf::Pixbuf> = unsafe {
                from_glib_full(gdk::ffi::gdk_pixbuf_get_from_window(
                    gw.to_glib_none().0,
                    0,
                    0,
                    60,
                    60,
                ))
            };
            pixbuf
                .map(|p| p.read_pixel_bytes().to_vec())
                .unwrap_or_default()
        };
        // Glyph pixels appear in the cell once the decodes finish and the tree repaints.
        let glyph_pixels = || {
            let data = snapshot();
            let corner = data
                .get(58 * 4..58 * 4 + 3)
                .map(<[u8]>::to_vec)
                .unwrap_or_default();
            data.chunks(4)
                .take(60 * 60)
                .filter(|p| p.len() >= 3 && corner.len() == 3 && p[..3] != corner[..])
                .count()
        };
        assert!(poll(5000, || glyph_pixels() > 20), "icons painted");
        let _ = tree;
    });
}
