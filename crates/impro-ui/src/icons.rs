//! Raw symbolic rasters avoid image-loader processes during first show.
//! The installed source is verified off-thread; other desktop icons keep GTK's lookup.
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
    sync::{Once, OnceLock},
};

#[path = "icon_sources.rs"]
mod sources;
static STARTED: Once = Once::new();
static VERIFIED: OnceLock<HashSet<&'static str>> = OnceLock::new();

/// Start source verification without GTK or main-thread filesystem reads.
/// Call before GTK startup when possible. A changed desktop icon uses its theme.
pub fn init() {
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("impro-icons".into())
            .spawn(|| {
                let verified = sources::SOURCES
                    .iter()
                    .filter_map(|(name, path, bytes)| {
                        std::fs::read(path)
                            .ok()
                            .filter(|actual| actual == bytes)
                            .map(|_| *name)
                    })
                    .collect();
                let _ = VERIFIED.set(verified);
            });
    });
}

fn pixels(name: &str, size: i32, scale: i32) -> Option<&'static [u8]> {
    if !VERIFIED.get()?.contains(name) {
        return None;
    }
    if size <= 0 || scale <= 0 {
        return None;
    }
    let pixels = sources::pixels(name, size.checked_mul(scale)?)?;
    let (_, source, _) = sources::SOURCES.iter().find(|(n, _, _)| *n == name)?;
    let theme = gtk::IconTheme::default()?;
    let info = theme.lookup_icon_for_scale(name, size, scale, gtk::IconLookupFlags::empty())?;
    // Preserve the user's desktop theme and any per-user icon overrides.
    (info.filename()?.to_str()? == *source).then_some(pixels)
}

/// Render verified symbolic colors directly, with no image decoder or disk IO.
/// Returns None for a different source or an unsupported size, scale or color.
pub fn symbolic_surface(
    name: &str,
    size: i32,
    scale: i32,
    context: &gtk::StyleContext,
    state: gtk::StateFlags,
) -> Option<gtk::cairo::Surface> {
    pixels(name, size, scale)?;
    let foreground = context.color(state);
    // Keep GTK's SVG coverage rounding for colors outside the exact atlas,
    // including translucent disabled states and intermediate CSS transitions.
    if foreground.alpha() != 1.0 {
        return None;
    }
    let channels = [foreground.red(), foreground.green(), foreground.blue()];
    if channels
        .iter()
        .any(|channel| (channel * 255.0 - (channel * 255.0).round()).abs() > 0.000001)
    {
        return None;
    }
    let color = format!(
        "#{:02x}{:02x}{:02x}",
        (foreground.red() * 255.0).round() as u8,
        (foreground.green() * 255.0).round() as u8,
        (foreground.blue() * 255.0).round() as u8,
    );
    let exact = sources::colored_pixels(name, size * scale, &color)?;
    surface(glib::Bytes::from_static(exact), size, scale)
}

fn surface(bytes: glib::Bytes, size: i32, scale: i32) -> Option<gtk::cairo::Surface> {
    let physical = size * scale;
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_bytes(
        &bytes,
        gtk::gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        physical,
        physical,
        physical * 4,
    );
    pixbuf.create_surface(scale, None::<&gdk::Window>)
}

/// Replace a known theme image with its identical raw symbolic raster.
/// Native HeaderBar images may change names when maximizing; track that too.
/// Unrecognized desktop icons retain GTK's normal theme lookup.
pub fn prepare_image(image: &gtk::Image) {
    if image.style_context().has_class("impro-startup-image") {
        return;
    }
    let Some(name) = image.icon_name() else {
        return;
    };
    let size = if image.pixel_size() > 0 {
        image.pixel_size()
    } else {
        16
    };
    if pixels(&name, size, image.scale_factor()).is_none() {
        return;
    }
    // Native color fallback can retain icon-name storage across allocations.
    // Install refresh handlers once even when HeaderBar prepares it again.
    image.style_context().add_class("impro-startup-image");
    let name = Rc::new(RefCell::new(name.to_string()));
    let updating = Rc::new(Cell::new(false));
    let update = {
        let name = name.clone();
        let updating = updating.clone();
        Rc::new(move |image: &gtk::Image| {
            if updating.replace(true) {
                return;
            }
            let name = name.borrow().clone();
            let size = if image.pixel_size() > 0 {
                image.pixel_size()
            } else {
                16
            };
            if let Some(surface) = symbolic_surface(
                &name,
                size,
                image.scale_factor(),
                &image.style_context(),
                image.state_flags(),
            ) {
                image.set_from_surface(Some(&surface));
            } else {
                image.set_from_icon_name(Some(&name), gtk::IconSize::Menu);
            }
            updating.set(false);
        })
    };
    update(image);
    let refresh = update.clone();
    image.connect_icon_name_notify(move |image| {
        if !updating.get() {
            if let Some(next) = image.icon_name() {
                *name.borrow_mut() = next.to_string();
                refresh(image);
            }
        }
    });
    let refresh = update.clone();
    image.connect_scale_factor_notify(move |image| refresh(image));
    let refresh = update.clone();
    image.connect_pixel_size_notify(move |image| refresh(image));
    let refresh = update.clone();
    image.connect_style_updated(move |image| refresh(image));
    let refresh = update.clone();
    image.connect_state_flags_changed(move |image, _| refresh(image));
    if let Some(theme) = gtk::IconTheme::default() {
        let weak = image.downgrade();
        let handler = theme.connect_changed(move |_| {
            if let Some(image) = weak.upgrade() {
                update(&image);
            }
        });
        let handler = RefCell::new(Some(handler));
        image.connect_destroy(move |_| {
            if let Some(handler) = handler.borrow_mut().take() {
                theme.disconnect(handler);
            }
        });
    }
}

/// Prepare existing images in a composed view without altering its widget tree.
pub fn prepare_widget(widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    if let Some(image) = widget.downcast_ref::<gtk::Image>() {
        prepare_image(image);
    } else if let Some(container) = widget.downcast_ref::<gtk::Container>() {
        container.forall(prepare_widget);
    }
}
