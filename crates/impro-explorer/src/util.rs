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
        Some(Box::new(move |_layout, cell, model, iter| {
            func(cell, model, iter)
        })),
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
    model
        .value(iter, column)
        .get::<Option<String>>()
        .ok()
        .flatten()
}

pub fn int(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter, column: i32) -> i32 {
    model.value(iter, column).get::<i32>().unwrap_or_default()
}

/// Path of a live row; rows handed out by the model always have one.
pub fn path_of(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter) -> gtk::TreePath {
    model.path(iter).unwrap_or_default()
}

/// First of the 128 private-use characters that stand for the raw bytes 0x80-0xFF in
/// a path stored in a tree model.
const RAW_BASE: u32 = 0xF700;

fn raw_char(c: char) -> Option<u8> {
    let c = c as u32;
    (RAW_BASE + 0x80..=RAW_BASE + 0xFF)
        .contains(&c)
        .then(|| (c - RAW_BASE) as u8)
}

/// A path as the string kept in tree models and signals, which must be UTF-8.
///
/// A path that is valid UTF-8 is kept as it is. Bytes that are not (and the rare
/// private-use characters that would be mistaken for them) become one private-use
/// character each, so [`decode_path`] gives back exactly the file on disk; a lossy
/// conversion would name a different (or no) file.
pub fn encode_path(path: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    if let Ok(text) = std::str::from_utf8(bytes) {
        if !text.chars().any(|c| raw_char(c).is_some()) {
            return text.to_owned();
        }
    }
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            if raw_char(c).is_some() {
                let mut buffer = [0u8; 4];
                for byte in c.encode_utf8(&mut buffer).bytes() {
                    out.push(char::from_u32(RAW_BASE + byte as u32).unwrap_or('\u{FFFD}'));
                }
            } else {
                out.push(c);
            }
        }
        for byte in chunk.invalid() {
            out.push(char::from_u32(RAW_BASE + *byte as u32).unwrap_or('\u{FFFD}'));
        }
    }
    out
}

/// The path that [`encode_path`] turned into `text`.
pub fn decode_path(text: &str) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStringExt;
    if !text.chars().any(|c| raw_char(c).is_some()) {
        return std::path::PathBuf::from(text);
    }
    let mut bytes = Vec::with_capacity(text.len());
    let mut buffer = [0u8; 4];
    for c in text.chars() {
        match raw_char(c) {
            Some(byte) => bytes.push(byte),
            None => bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes()),
        }
    }
    std::path::PathBuf::from(std::ffi::OsString::from_vec(bytes))
}

/// A file name for display: invalid bytes show as the replacement character.
pub fn display_name(name: &std::ffi::OsStr) -> String {
    name.to_string_lossy().into_owned()
}
