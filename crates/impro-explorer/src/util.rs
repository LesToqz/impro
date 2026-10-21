//! Small GTK helpers shared by the panels.
use gtk::prelude::*;

/// `column.pack_start`, unambiguous between `CellLayout` and `TreeViewColumn`.
pub fn pack(column: &gtk::TreeViewColumn, cell: &impl IsA<gtk::CellRenderer>, expand: bool) {
    CellLayoutExt::pack_start(column, cell, expand);
}

pub fn pack_end(column: &gtk::TreeViewColumn, cell: &impl IsA<gtk::CellRenderer>, expand: bool) {
    CellLayoutExt::pack_end(column, cell, expand);
}

pub fn attribute(
    column: &gtk::TreeViewColumn,
    cell: &impl IsA<gtk::CellRenderer>,
    attribute: &str,
    model_column: i32,
) {
    CellLayoutExt::add_attribute(column, cell, attribute, model_column);
}

pub fn data_func(
    column: &gtk::TreeViewColumn,
    cell: &impl IsA<gtk::CellRenderer>,
    func: impl Fn(&gtk::CellRenderer, &gtk::TreeModel, &gtk::TreeIter) + 'static,
) {
    CellLayoutExt::set_cell_data_func(
        column,
        cell,
        Some(Box::new(move |_layout, cell, model, iter| func(cell, model, iter))),
    );
}

/// A string cell of a tree model row (empty for NULL).
pub fn string(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter, column: i32) -> String {
    model
        .value(iter, column)
        .get::<Option<String>>()
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// A string cell that may be NULL.
pub fn opt_string(
    model: &impl IsA<gtk::TreeModel>,
    iter: &gtk::TreeIter,
    column: i32,
) -> Option<String> {
    model.value(iter, column).get::<Option<String>>().ok().flatten()
}

pub fn int(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter, column: i32) -> i32 {
    model.value(iter, column).get::<i32>().unwrap_or_default()
}

/// Path of a live row; rows handed out by the model always have one.
pub fn path_of(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter) -> gtk::TreePath {
    model.path(iter).unwrap_or_default()
}
