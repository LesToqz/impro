//! Default window icon. Decoding is deferred to a low-priority idle after the first paint.
use gdk::gdk_pixbuf::{Pixbuf, PixbufLoader};
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc};

macro_rules! icon {
    ($size:literal) => {
        (
            $size,
            include_bytes!(concat!(
                "../../../packaging/icons/hicolor/",
                $size,
                "x",
                $size,
                "/apps/impro.png"
            ))
            .as_slice(),
        )
    };
}

const ICONS: [(u32, &[u8]); 7] = [
    icon!(16),
    icon!(24),
    icon!(32),
    icon!(48),
    icon!(64),
    icon!(128),
    icon!(256),
];

fn decode(bytes: &[u8]) -> Result<Pixbuf, glib::Error> {
    let loader = PixbufLoader::with_type("png")?;
    loader.write(bytes)?;
    loader.close()?;
    loader
        .pixbuf()
        .ok_or_else(|| glib::Error::new(glib::FileError::Failed, "no pixbuf"))
}

/// Set the default icon list for every window. A PNG that fails to decode is logged and skipped.
pub fn install_default() {
    let pixbufs: Vec<Pixbuf> = ICONS
        .iter()
        .filter_map(|(size, bytes)| match decode(bytes) {
            Ok(pixbuf) => Some(pixbuf),
            Err(error) => {
                eprintln!("impro: cannot decode {size}px app icon: {error}");
                None
            }
        })
        .collect();
    if !pixbufs.is_empty() {
        gtk::Window::set_default_icon_list(&pixbufs);
    }
}

/// Run `install_default` from a low-priority idle queued by the window's first draw.
pub fn install_after_first_paint(window: &impl IsA<gtk::Widget>) {
    let armed = Rc::new(Cell::new(false));
    window.connect_draw(move |_, _| {
        if !armed.replace(true) {
            glib::idle_add_local_full(glib::Priority::LOW, || {
                install_default();
                glib::ControlFlow::Break
            });
        }
        glib::Propagation::Proceed
    });
}
