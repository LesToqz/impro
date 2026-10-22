//! Prepare the host's remaining native icon render points without feature-crate changes.
use crate::MainWindow;
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(crate) fn prepare(window: &MainWindow) {
    let tree = window.explorer.tree();
    for column in tree.columns() {
        for cell in CellLayoutExt::cells(&column) {
            let Ok(cell) = cell.downcast::<gtk::CellRendererPixbuf>() else {
                continue;
            };
            // The explorer subclass renders from its own asynchronous cache.
            // Setting a surface clears icon-name, which that renderer needs.
            if cell.is::<impro_explorer::icons::IconCell>() {
                continue;
            }
            let weak = tree.downgrade();
            TreeViewColumnExt::set_cell_data_func(
                &column,
                &cell,
                Some(Box::new(move |_, cell, model, iter| {
                    let Some(tree) = weak.upgrade() else { return };
                    let Some(cell) = cell.downcast_ref::<gtk::CellRendererPixbuf>() else {
                        return;
                    };
                    // The model retains its semantic icon names for directory handling.
                    let name = model
                        .value(iter, 0)
                        .get::<Option<String>>()
                        .ok()
                        .flatten()
                        .filter(|name| !name.is_empty())
                        .map(|name| format!("{name}-symbolic"));
                    cell.set_icon_name(name.as_deref());
                    let Some(name) = name else { return };
                    let flags = if model
                        .path(iter)
                        .is_some_and(|path| tree.selection().path_is_selected(&path))
                    {
                        gtk::CellRendererState::SELECTED
                    } else {
                        gtk::CellRendererState::empty()
                    };
                    let context = tree.style_context();
                    context.save();
                    context.add_class("image");
                    let state = cell.state(Some(&tree), flags);
                    context.set_state(state);
                    let surface = impro_ui::icons::symbolic_surface(
                        &name,
                        16,
                        tree.scale_factor(),
                        &context,
                        state,
                    );
                    context.restore();
                    if let Some(surface) = surface {
                        cell.set_surface(Some(&surface));
                    }
                })),
            );
        }
    }
    prepare_search_entry(&window.explorer.search().entry());
}

fn prepare_search_entry(entry: &gtk::SearchEntry) {
    let updating = Rc::new(Cell::new(false));
    let name = Rc::new(RefCell::new(
        entry
            .icon_name(gtk::EntryIconPosition::Primary)
            .map(|name| name.to_string()),
    ));
    let update = {
        let updating = updating.clone();
        let name = name.clone();
        Rc::new(move |entry: &gtk::SearchEntry| {
            if updating.replace(true) {
                return;
            }
            let name = name.borrow().clone();
            let context = gtk::StyleContext::new();
            let path = entry.path();
            let image = path.append_type(gtk::Image::static_type());
            path.iter_set_object_name(image, Some("image"));
            path.iter_add_class(image, "left");
            context.set_path(&path);
            context.set_parent(Some(&entry.style_context()));
            context.set_state(entry.state_flags());
            // GtkEntry's pixbuf API has no device-scale argument. Keep its native
            // renderer at other scales rather than resampling a 1x raster.
            let surface = name
                .as_deref()
                .filter(|_| entry.scale_factor() == 1)
                .and_then(|name| {
                    impro_ui::icons::symbolic_surface(name, 16, 1, &context, entry.state_flags())
                });
            if let Some(pixbuf) =
                surface.and_then(|s| gdk::pixbuf_get_from_surface(&s, 0, 0, 16, 16))
            {
                entry.style_context().add_class("impro-startup-raster");
                entry.set_icon_from_pixbuf(gtk::EntryIconPosition::Primary, Some(&pixbuf));
            } else {
                entry.style_context().remove_class("impro-startup-raster");
                entry.set_icon_from_icon_name(gtk::EntryIconPosition::Primary, name.as_deref());
            }
            updating.set(false);
        })
    };
    update(entry);
    let refresh = update.clone();
    entry.connect_primary_icon_name_notify(move |entry| {
        if !updating.get() {
            *name.borrow_mut() = entry
                .icon_name(gtk::EntryIconPosition::Primary)
                .map(|name| name.to_string());
            refresh(entry);
        }
    });
    let refresh = update.clone();
    entry.connect_style_updated(move |entry| refresh(entry));
    let refresh = update.clone();
    entry.connect_state_flags_changed(move |entry, _| refresh(entry));
    let refresh = update.clone();
    entry.connect_scale_factor_notify(move |entry| refresh(entry));
    if let Some(theme) = gtk::IconTheme::default() {
        let weak = entry.downgrade();
        let handler = theme.connect_changed(move |_| {
            if let Some(entry) = weak.upgrade() {
                update(&entry);
            }
        });
        let handler = RefCell::new(Some(handler));
        entry.connect_destroy(move |_| {
            if let Some(handler) = handler.borrow_mut().take() {
                theme.disconnect(handler);
            }
        });
    }
}
