//! Symbolic icons decoded off the main thread.
//!
//! GTK recolours a symbolic icon and decodes it in the middle of a paint, through
//! gdk-pixbuf's sandboxed image loader: 10-12 ms for the first use of an icon and
//! about 3.5 ms for every further colour (the hover and selected colours of a row
//! are two more). [`IconCell`] draws its icons from the cache below instead: a
//! missing variant is decoded by GTK's own worker thread, the paint goes on without
//! it (showing the same icon in another colour if one is known, otherwise
//! nothing), and the widget is repainted when the decode finishes.
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{cairo, gdk, gio, glib};
use std::cell::RefCell;
use std::collections::HashMap;

/// Slots kept before finished decodes are dropped (colours that change while a
/// style transition runs would otherwise grow the cache without bound).
const MAX_SLOTS: usize = 128;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    name: String,
    px: i32,
    scale: i32,
    rgba: [u32; 4],
}

enum Slot {
    /// Decoding; the widgets to repaint when it is done.
    Loading(Vec<glib::WeakRef<gtk::Widget>>),
    /// Decoded (`None` when the icon does not exist or failed to load).
    Ready(Option<cairo::Surface>),
}

#[derive(Default)]
struct Cache {
    slots: HashMap<Key, Slot>,
    /// The latest decode of a name and size in any colour, shown while another
    /// colour is on its way.
    last: HashMap<(String, i32, i32), cairo::Surface>,
    watched: bool,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// Edge of the icons in logical pixels (`GTK_ICON_SIZE_MENU`).
pub fn menu_size() -> i32 {
    gtk::IconSize::Menu.lookup().map_or(16, |(w, _)| w)
}

/// The icon `name` at `px` logical pixels, recoloured to `fg`, if it is decoded.
/// Otherwise the decode is started (once) and `widget` repainted when it is done.
pub fn surface(
    widget: &gtk::Widget,
    name: &str,
    px: i32,
    fg: &gdk::RGBA,
) -> Option<cairo::Surface> {
    let scale = widget.scale_factor().max(1);
    let key = Key {
        name: name.to_owned(),
        px,
        scale,
        rgba: [
            (fg.red() as f32).to_bits(),
            (fg.green() as f32).to_bits(),
            (fg.blue() as f32).to_bits(),
            (fg.alpha() as f32).to_bits(),
        ],
    };
    let (found, start) = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let fallback = cache.last.get(&(key.name.clone(), px, scale)).cloned();
        match cache.slots.get_mut(&key) {
            Some(Slot::Ready(surface)) => (surface.clone().or(fallback), false),
            Some(Slot::Loading(waiting)) => {
                if !waiting.iter().any(|w| w.upgrade().as_ref() == Some(widget)) {
                    waiting.push(widget.downgrade());
                }
                (fallback, false)
            }
            None => {
                if cache.slots.len() >= MAX_SLOTS {
                    cache.slots.retain(|_, s| matches!(s, Slot::Loading(_)));
                }
                cache
                    .slots
                    .insert(key.clone(), Slot::Loading(vec![widget.downgrade()]));
                (fallback, true)
            }
        }
    });
    if start {
        load(widget, key, fg);
    }
    found
}

/// Starts the decode of a colour variant nobody has asked for yet (the hover and
/// selected colours of a row), so the first hover finds it ready.
pub fn preload(widget: &gtk::Widget, name: &str, px: i32, fg: &gdk::RGBA) {
    let _ = surface(widget, name, px, fg);
}

fn load(widget: &gtk::Widget, key: Key, fg: &gdk::RGBA) {
    let theme = widget
        .screen()
        .and_then(|screen| gtk::IconTheme::for_screen(&screen))
        .or_else(gtk::IconTheme::default);
    let info = theme.as_ref().and_then(|t| {
        t.lookup_icon_for_scale(&key.name, key.px, key.scale, gtk::IconLookupFlags::empty())
    });
    let (Some(theme), Some(info)) = (theme, info) else {
        finish(&key, None);
        return;
    };
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.watched {
            cache.watched = true;
            theme.connect_changed(|_| {
                CACHE.with(|cache| {
                    cache
                        .borrow_mut()
                        .slots
                        .retain(|_, s| matches!(s, Slot::Loading(_)))
                })
            });
        }
    });
    let scale = key.scale;
    info.load_symbolic_async(
        fg,
        None,
        None,
        None,
        gio::Cancellable::NONE,
        move |result| {
            let surface = result
                .ok()
                .and_then(|(pixbuf, _)| pixbuf.create_surface(scale, None::<&gdk::Window>));
            finish(&key, surface);
        },
    );
}

fn finish(key: &Key, surface: Option<cairo::Surface>) {
    let waiting = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(surface) = &surface {
            cache
                .last
                .insert((key.name.clone(), key.px, key.scale), surface.clone());
        }
        match cache.slots.insert(key.clone(), Slot::Ready(surface)) {
            Some(Slot::Loading(waiting)) => waiting,
            _ => Vec::new(),
        }
    });
    for widget in waiting.into_iter().filter_map(|w| w.upgrade()) {
        widget.queue_draw();
    }
}

/// The style state GTK paints a cell in (`gtk_cell_renderer_get_state`).
fn cell_state(widget: &gtk::Widget, flags: gtk::CellRendererState) -> gtk::StateFlags {
    let mut state = widget.state_flags()
        & !(gtk::StateFlags::FOCUSED
            | gtk::StateFlags::PRELIGHT
            | gtk::StateFlags::SELECTED
            | gtk::StateFlags::DROP_ACTIVE);
    if state.contains(gtk::StateFlags::INSENSITIVE) {
        return state;
    }
    if flags.contains(gtk::CellRendererState::PRELIT) {
        state |= gtk::StateFlags::PRELIGHT;
    }
    if flags.contains(gtk::CellRendererState::SELECTED) {
        state |= gtk::StateFlags::SELECTED;
    }
    if flags.contains(gtk::CellRendererState::FOCUSED) {
        state |= gtk::StateFlags::FOCUSED;
    }
    state
}

mod imp {
    use super::*;
    use gtk::subclass::cell_renderer::CellRendererImpl;
    use gtk::subclass::cell_renderer_pixbuf::CellRendererPixbufImpl;

    #[derive(Default)]
    pub struct IconCell;

    #[glib::object_subclass]
    impl ObjectSubclass for IconCell {
        const NAME: &'static str = "ImproIconCell";
        type Type = super::IconCell;
        type ParentType = gtk::CellRendererPixbuf;
    }

    impl ObjectImpl for IconCell {}

    impl CellRendererImpl for IconCell {
        fn preferred_width<P: IsA<gtk::Widget>>(&self, _widget: &P) -> (i32, i32) {
            let width = menu_size() + 2 * self.obj().padding().0;
            (width, width)
        }

        fn preferred_height<P: IsA<gtk::Widget>>(&self, _widget: &P) -> (i32, i32) {
            let height = menu_size() + 2 * self.obj().padding().1;
            (height, height)
        }

        fn render<P: IsA<gtk::Widget>>(
            &self,
            cr: &cairo::Context,
            widget: &P,
            _background_area: &gdk::Rectangle,
            cell_area: &gdk::Rectangle,
            flags: gtk::CellRendererState,
        ) {
            let obj = self.obj();
            let name: Option<String> = obj.property("icon-name");
            let Some(name) = name.filter(|n| !n.is_empty()) else {
                return;
            };
            let widget: &gtk::Widget = widget.upcast_ref();
            let state = cell_state(widget, flags);
            let context = widget.style_context();
            let color = |state: gtk::StateFlags| {
                context.save();
                context.add_class("cell");
                context.set_state(state);
                let fg = context.color(state);
                context.restore();
                fg
            };
            let px = menu_size();
            let shown = surface(widget, &name, px, &color(state));
            for other in [gtk::StateFlags::PRELIGHT, gtk::StateFlags::SELECTED] {
                if !state.contains(other) {
                    preload(widget, &name, px, &color(state | other));
                }
            }
            let Some(surface) = shown else {
                return;
            };
            let ((xpad, ypad), (xalign, yalign)) = (obj.padding(), obj.alignment());
            let x =
                cell_area.x() + xpad + (xalign * (cell_area.width() - px - 2 * xpad) as f32) as i32;
            let y = cell_area.y()
                + ypad
                + (yalign * (cell_area.height() - px - 2 * ypad) as f32) as i32;
            let _ = cr.save();
            let _ = cr.set_source_surface(&surface, x as f64, y as f64);
            let _ = cr.paint();
            let _ = cr.restore();
        }
    }

    impl CellRendererPixbufImpl for IconCell {}
}

glib::wrapper! {
    /// A `GtkCellRendererPixbuf` (same `icon-name` property and attributes) that
    /// paints its symbolic icon from the asynchronous cache of this module.
    pub struct IconCell(ObjectSubclass<imp::IconCell>)
        @extends gtk::CellRendererPixbuf, gtk::CellRenderer;
}

impl IconCell {
    pub fn new() -> Self {
        glib::Object::new()
    }
}

impl Default for IconCell {
    fn default() -> Self {
        Self::new()
    }
}
