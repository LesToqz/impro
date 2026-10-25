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
#[cfg(unix)]
const RAW_BASE: u32 = 0xF700;

#[cfg(unix)]
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
#[cfg(unix)]
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
#[cfg(unix)]
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

// GTK advances and removes rows through a mutable `GtkTreeIter`, which the bindings
// only hand out as `&TreeIter`: mutating through it is not allowed, and in release
// builds the optimiser may keep using the copy from before the call. These helpers
// pass a real mutable copy to GTK instead.

/// The row after `iter`, if there is one.
pub fn next_iter(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter) -> Option<gtk::TreeIter> {
    use glib::translate::*;
    // SAFETY: `raw` is a private copy of a valid iterator of `model`.
    unsafe {
        let mut raw = *iter.to_glib_none().0;
        let found = gtk::ffi::gtk_tree_model_iter_next(model.as_ref().to_glib_none().0, &mut raw);
        (found != 0).then(|| from_glib_none(&raw as *const gtk::ffi::GtkTreeIter))
    }
}

/// Remove the row `iter` of a list; the row that followed it, if there is one.
pub fn remove_list_row(store: &gtk::ListStore, iter: &gtk::TreeIter) -> Option<gtk::TreeIter> {
    use glib::translate::*;
    // SAFETY: as for `next_iter`; GTK leaves the copy on the next row when it exists.
    unsafe {
        let mut raw = *iter.to_glib_none().0;
        let more = gtk::ffi::gtk_list_store_remove(store.to_glib_none().0, &mut raw);
        (more != 0).then(|| from_glib_none(&raw as *const gtk::ffi::GtkTreeIter))
    }
}

/// Remove the row `iter` of a tree; the sibling that followed it, if there is one.
pub fn remove_tree_row(store: &gtk::TreeStore, iter: &gtk::TreeIter) -> Option<gtk::TreeIter> {
    use glib::translate::*;
    // SAFETY: as for `next_iter`.
    unsafe {
        let mut raw = *iter.to_glib_none().0;
        let more = gtk::ffi::gtk_tree_store_remove(store.to_glib_none().0, &mut raw);
        (more != 0).then(|| from_glib_none(&raw as *const gtk::ffi::GtkTreeIter))
    }
}

/// The first row below `parent` (the top level for `None`) whose `column` holds `wanted`.
pub fn find_child(
    model: &impl IsA<gtk::TreeModel>,
    parent: Option<&gtk::TreeIter>,
    column: i32,
    wanted: &str,
) -> Option<gtk::TreeIter> {
    let mut row = match parent {
        Some(parent) => model.iter_children(Some(parent)),
        None => model.iter_first(),
    };
    while let Some(candidate) = row {
        if string(model, &candidate, column) == wanted {
            return Some(candidate);
        }
        row = next_iter(model, &candidate);
    }
    None
}

/// Lossless UTF-16 encoding for Windows model strings, including unpaired
/// surrogates. Encodings only round-trip within this process/platform. A reserved
/// prefix is escaped too; decoding arbitrary model strings remains memory-safe.
#[cfg(windows)]
pub fn encode_path(path: &std::path::Path) -> String {
    use std::os::windows::ffi::OsStrExt;
    if let Some(text) = path.to_str().filter(|s| !s.starts_with('\u{E000}')) {
        return text.to_owned();
    }
    let mut text = String::from("\u{E000}");
    for unit in path.as_os_str().encode_wide() {
        use std::fmt::Write;
        write!(text, "{unit:04x}").expect("string write");
    }
    text
}

#[cfg(windows)]
pub fn decode_path(text: &str) -> std::path::PathBuf {
    use std::os::windows::ffi::OsStringExt;
    if let Some(hex) = text
        .strip_prefix('\u{E000}')
        .filter(|s| s.is_ascii() && s.len() % 4 == 0)
    {
        let units: Option<Vec<_>> = hex
            .as_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| u16::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok())
            .collect();
        if let Some(units) = units {
            return std::ffi::OsString::from_wide(&units).into();
        }
    }
    text.into()
}

#[cfg(test)]
mod path_tests {
    use super::*;
    #[test]
    fn unicode_and_reserved_characters_round_trip() {
        for text in [
            "folder/日本語.txt",
            "\u{E000}file",
            "\u{E080}file",
            "C:\\folder\\file",
        ] {
            let path = std::path::Path::new(text);
            assert_eq!(decode_path(&encode_path(path)), path);
        }
    }
    #[cfg(windows)]
    #[test]
    fn unpaired_surrogate_round_trip() {
        use std::os::windows::ffi::OsStringExt;
        let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&[0x61, 0xd800, 0x62]));
        assert_eq!(decode_path(&encode_path(&path)), path);
    }
}
