//! Vector glyphs painted with cairo: stroke icons on a 24-unit grid.
//!
//! They need no icon theme, image loader or bundled raster, so they look the same on every
//! platform, at every display scale and in every theme, and they take the CSS colour of the
//! widget that paints them. The Windows build ships only part of the Adwaita theme, which is why
//! a themed name such as `panel-bottom-symbolic` could fall back to a text glyph there.
//!
//! Glyphs marked "Lucide" follow the path data of the Lucide icon set (ISC licence, see
//! `THIRD_PARTY_NOTICES.md`); the others were drawn for impro. Paths are a subset of SVG path
//! syntax (`M L H V C S Q T A Z`, absolute and relative); a path starting with `F` is filled
//! instead of stroked.
use gtk::cairo;
use gtk::prelude::*;

/// Stroke width on the 24-unit grid: 1.2 px at 16 px.
pub const STROKE: f64 = 1.8;

type Paths = &'static [&'static str];

const RECT_3_3_18: &str = "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z";

const GLYPHS: &[(&str, Paths)] = &[
    // Lucide `eye`.
    (
        "eye",
        &[
            "M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0",
            "M9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0Z",
        ],
    ),
    // Lucide `code-xml`.
    ("code", &["m18 16 4-4-4-4", "m6 8-4 4 4 4", "m14.5 4-5 16"]),
    // Lucide `panel-left`, `panel-right`, `panel-bottom`.
    ("panel-left", &[RECT_3_3_18, "M9 3v18"]),
    ("panel-right", &[RECT_3_3_18, "M15 3v18"]),
    ("panel-bottom", &[RECT_3_3_18, "M3 15h18"]),
    // Lucide `chevron-right`, `chevron-down`.
    ("chevron-right", &["m9 18 6-6-6-6"]),
    ("chevron-down", &["m6 9 6 6 6-6"]),
    // Lucide `x`, `plus`.
    ("close", &["M18 6 6 18", "m6 6 12 12"]),
    ("plus", &["M5 12h14", "M12 5v14"]),
    // Lucide `folder`, `folder-open`.
    (
        "folder",
        &["M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"],
    ),
    (
        "folder-open",
        &["m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"],
    ),
    // Three lines of text: any file without a more specific glyph.
    ("file", &["M4.5 7.5h12", "M4.5 12h15", "M4.5 16.5h9"]),
    // Lucide `book-open`: Markdown and documents.
    (
        "book",
        &[
            "M12 7v14",
            "M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z",
        ],
    ),
    // Lucide `braces`: JSON and other data.
    (
        "braces",
        &[
            "M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1",
            "M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1",
        ],
    ),
    // Brackets around a T.
    (
        "toml",
        &[
            "M7.5 4.5h-2a1 1 0 0 0-1 1v13a1 1 0 0 0 1 1h2",
            "M16.5 4.5h2a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-2",
            "M9 9h6",
            "M12 9v7.5",
        ],
    ),
    // Lucide `settings`.
    (
        "settings",
        &[
            "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
            "M9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0Z",
        ],
    ),
    // Lucide `lock`.
    (
        "lock",
        &[
            "M5 11h14a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2Z",
            "M7 11V7a5 5 0 0 1 10 0v4",
        ],
    ),
    // Lucide `image`.
    (
        "image",
        &[
            RECT_3_3_18,
            "M7 9a2 2 0 1 0 4 0a2 2 0 1 0-4 0Z",
            "m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21",
        ],
    ),
    // Lucide `git-branch`: Git metadata files.
    (
        "git",
        &[
            "M6 3v12",
            "M15 6a3 3 0 1 0 6 0a3 3 0 1 0-6 0Z",
            "M3 18a3 3 0 1 0 6 0a3 3 0 1 0-6 0Z",
            "M18 9a9 9 0 0 1-9 9",
        ],
    ),
    // Lucide `terminal`: shell scripts.
    ("terminal", &["m4 17 6-6-6-6", "M12 19h8"]),
    // Lucide `database`.
    (
        "database",
        &[
            "M3 5a9 3 0 1 0 18 0a9 3 0 1 0-18 0Z",
            "M3 5v14a9 3 0 0 0 18 0V5",
            "M3 12a9 3 0 0 0 18 0",
        ],
    ),
    // Lucide `package`: containers and archives.
    (
        "package",
        &[
            "M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z",
            "M12 22V12",
            "m3.3 7 7.703 4.734a2 2 0 0 0 1.994 0L20.7 7",
            "m7.5 4.27 9 5.15",
        ],
    ),
    // Lucide `music`.
    (
        "audio",
        &[
            "M9 18V5l12-2v13",
            "M3 18a3 3 0 1 0 6 0a3 3 0 1 0-6 0Z",
            "M15 16a3 3 0 1 0 6 0a3 3 0 1 0-6 0Z",
        ],
    ),
    // Lucide `video`.
    (
        "video",
        &[
            "m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5",
            "M4 6h10a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2Z",
        ],
    ),
    // Lucide `type`: fonts.
    ("font", &["M4 7V4h16v3", "M9 20h6", "M12 4v16"]),
    // Lucide `diff`.
    ("diff", &["M12 3v14", "M5 10h14", "M5 21h14"]),
    // Lucide `info`: logs.
    (
        "info",
        &["M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0Z", "M12 16v-4", "M12 8h.01"],
    ),
    // Lucide `hash`: style sheets.
    ("hash", &["M4 9h16", "M4 15h16", "M10 3 8 21", "M16 3l-2 18"]),
    // A ring around an R.
    (
        "rust",
        &[
            "M3.5 12a8.5 8.5 0 1 0 17 0a8.5 8.5 0 1 0-17 0Z",
            "M9.5 16.5v-9h3.25a2.25 2.25 0 0 1 0 4.5H9.5",
            "m12.75 12 2.25 4.5",
        ],
    ),
];

/// Whether `name` is a glyph (`badge:` names included).
pub fn exists(name: &str) -> bool {
    badge_text(name).is_some() || paths(name).is_some()
}

fn paths(name: &str) -> Option<Paths> {
    GLYPHS.iter().find(|(n, _)| *n == name).map(|(_, p)| *p)
}

/// The letters of a `badge:XY` glyph (one to three characters).
fn badge_text(name: &str) -> Option<&str> {
    name.strip_prefix("badge:")
        .filter(|text| (1..=3).contains(&text.chars().count()))
}

/// Paint glyph `name` in a `size` square at (`x`, `y`). `false` for an unknown name.
pub fn draw(cr: &cairo::Context, name: &str, x: f64, y: f64, size: f64, color: &gdk::RGBA) -> bool {
    let badge = badge_text(name);
    let paths = paths(name);
    if badge.is_none() && paths.is_none() {
        return false;
    }
    let _ = cr.save();
    cr.translate(x, y);
    cr.scale(size / 24.0, size / 24.0);
    cr.set_source_rgba(color.red(), color.green(), color.blue(), color.alpha());
    cr.set_line_width(STROKE);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    if let Some(text) = badge {
        draw_badge(cr, text);
    }
    for path in paths.unwrap_or_default() {
        cr.new_path();
        match path.strip_prefix('F') {
            Some(fill) => {
                trace(cr, fill);
                let _ = cr.fill();
            }
            None => {
                trace(cr, path);
                let _ = cr.stroke();
            }
        }
    }
    let _ = cr.restore();
    true
}

/// A rounded square holding up to three letters (language files without a drawn glyph).
fn draw_badge(cr: &cairo::Context, text: &str) {
    cr.new_path();
    trace(cr, "M6 3.5h12a2.5 2.5 0 0 1 2.5 2.5v12a2.5 2.5 0 0 1-2.5 2.5H6a2.5 2.5 0 0 1-2.5-2.5V6a2.5 2.5 0 0 1 2.5-2.5Z");
    let _ = cr.stroke();
    cr.select_font_face(
        "sans-serif",
        cairo::FontSlant::Normal,
        cairo::FontWeight::Bold,
    );
    let count = text.chars().count();
    cr.set_font_size(match count {
        1 => 10.5,
        2 => 8.5,
        _ => 6.5,
    });
    if let Ok(extents) = cr.text_extents(text) {
        let x = 12.0 - extents.width() / 2.0 - extents.x_bearing();
        let y = 12.0 - extents.height() / 2.0 - extents.y_bearing();
        cr.move_to(x, y);
        let _ = cr.show_text(text);
    }
}

/// The glyph shown for a file named `name` (no directory part), following the editor's
/// association of names and suffixes with icon kinds.
pub fn file_glyph(name: &str) -> &'static str {
    if let Some(glyph) = by_stem(name) {
        return glyph;
    }
    // The whole name, then every suffix after a dot: `docker-compose.yml`, `Cargo.toml` ->
    // `toml`, `.gitignore` -> `gitignore`, `a.stories.tsx` -> `stories.tsx` -> `tsx`.
    let mut suffix = name;
    loop {
        if let Some(glyph) = by_suffix(suffix) {
            return glyph;
        }
        match suffix.split_once('.') {
            Some((_, rest)) if !rest.is_empty() => suffix = rest,
            _ => return "file",
        }
    }
}

fn by_stem(name: &str) -> Option<&'static str> {
    Some(match name {
        "Containerfile" | "Dockerfile" | ".dockerignore" => "package",
        "Podfile" | "Gemfile" | "Rakefile" => "badge:RB",
        "Procfile" | "Makefile" | "makefile" | "Justfile" | "justfile" => "terminal",
        "LICENSE" | "LICENCE" | "COPYING" => "book",
        _ => return None,
    })
}

fn by_suffix(suffix: &str) -> Option<&'static str> {
    let lower = suffix.to_ascii_lowercase();
    Some(match lower.as_str() {
        "aac" | "flac" | "m4a" | "mka" | "mp3" | "ogg" | "opus" | "wav" | "wma" | "wv" => "audio",
        "c" | "h" => "badge:C",
        "c++" | "h++" | "cc" | "cpp" | "cppm" | "cxx" | "hh" | "hpp" | "hxx" | "inl" | "ixx" => {
            "badge:C++"
        }
        "cs" => "badge:C#",
        "css" | "pcss" | "postcss" | "sass" | "scss" | "less" => "hash",
        "dart" => "badge:DT",
        "diff" | "patch" => "diff",
        "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml" => "package",
        "doc" | "docx" | "mdx" | "odp" | "ods" | "odt" | "pdf" | "ppt" | "pptx" | "rtf" | "txt"
        | "xls" | "xlsx" | "markdown" | "md" | "rst" | "adoc" | "ipynb" => "book",
        "editorconfig" | "conf" | "ini" | "cfg" | "prettierrc" | "eslintrc" | "stylelintrc"
        | "npmrc" | "env" | "properties" => "settings",
        "ex" | "exs" | "eex" | "heex" | "leex" => "badge:EX",
        "otf" | "ttf" | "woff" | "woff2" => "font",
        "go" | "mod" | "work" => "badge:GO",
        "hs" => "badge:HS",
        "htm" | "html" | "xml" | "hbs" | "plist" | "svelte" | "vue" | "astro" | "handlebars" => {
            "code"
        }
        "avif" | "bmp" | "gif" | "heic" | "heif" | "ico" | "j2k" | "jfif" | "jp2" | "jpeg"
        | "jpg" | "jxl" | "png" | "psd" | "qoi" | "svg" | "tiff" | "webp" => "image",
        "java" => "badge:J",
        "cjs" | "js" | "mjs" => "badge:JS",
        "jsx" | "tsx" | "cjsx" | "ctsx" | "mjsx" | "mtsx" => "code",
        "json" | "jsonc" | "json5" | "yaml" | "yml" | "kdl" => "braces",
        "kt" | "kts" => "badge:KT",
        "lock" | "lockb" => "lock",
        "log" => "info",
        "lua" | "luau" => "badge:LUA",
        "php" => "badge:PHP",
        "py" | "pyi" | "pyw" => "badge:PY",
        "r" => "badge:R",
        "rb" => "badge:RB",
        "rs" => "rust",
        "scala" | "sc" => "badge:SC",
        "accdb" | "csv" | "dat" | "db" | "dbf" | "mdb" | "sql" | "sqlite" | "sqlite3" | "tsv"
        | "psv" | "ssv" | "parquet" => "database",
        "swift" => "badge:SW",
        "bash" | "bash_aliases" | "bash_login" | "bash_logout" | "bash_profile" | "bashrc"
        | "fish" | "nu" | "profile" | "ps1" | "psm1" | "sh" | "zsh" | "zshrc" | "zshenv"
        | "zprofile" | "bat" | "cmd" => "terminal",
        "toml" => "toml",
        "ts" | "cts" | "mts" => "badge:TS",
        "commit_editmsg" | "merge_msg" | "gitattributes" | "gitignore" | "gitkeep"
        | "gitmodules" | "gitconfig" => "git",
        "avi" | "m4v" | "mkv" | "mov" | "mp4" | "webm" | "wmv" => "video",
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "deb" | "rpm" => {
            "package"
        }
        "zig" => "badge:ZIG",
        "nix" => "badge:NIX",
        "ml" | "mli" => "badge:ML",
        "elm" => "badge:ELM",
        "erl" | "hrl" => "badge:ERL",
        "clj" | "cljs" | "cljc" | "edn" => "badge:CLJ",
        "gleam" => "badge:GL",
        "graphql" | "gql" => "badge:GQL",
        "proto" => "braces",
        "tf" | "tfvars" | "hcl" => "badge:TF",
        "wgsl" | "glsl" | "hlsl" | "metal" => "code",
        _ => return None,
    })
}

// ---- drawing areas -----------------------------------------------------------------------

const NAME_KEY: &str = "impro-glyph-name";

/// A drawing area showing glyph `name` at `size` logical pixels (enlarged by the display scale,
/// see [`crate::scale`]) in the CSS colour of its own node, which inherits its parent's colour:
/// a glyph in a button follows the button's hover and checked colours.
pub fn image(name: &str, size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.style_context().add_class("impro-glyph");
    let side = crate::scale::px(size);
    area.set_size_request(side, side);
    area.set_halign(gtk::Align::Center);
    area.set_valign(gtk::Align::Center);
    set_name(&area, name);
    area.connect_draw(|area, cr| {
        if let Some(name) = name_of(area) {
            let width = f64::from(area.allocated_width());
            let height = f64::from(area.allocated_height());
            let side = width.min(height);
            let context = area.style_context();
            let color = context.color(area.state_flags());
            draw(
                cr,
                &name,
                ((width - side) / 2.0).round(),
                ((height - side) / 2.0).round(),
                side,
                &color,
            );
        }
        glib::Propagation::Proceed
    });
    // The colour is inherited: a parent's state change restyles this node.
    area.connect_style_updated(|area| area.queue_draw());
    area
}

/// Change the glyph a drawing area made by [`image`] shows.
pub fn set_name(area: &gtk::DrawingArea, name: &str) {
    if name_of(area).as_deref() == Some(name) {
        return;
    }
    // SAFETY: the key is only ever used with a `String` value, here and in `name_of`.
    unsafe {
        area.set_data(NAME_KEY, name.to_owned());
    }
    area.queue_draw();
}

/// The glyph a drawing area made by [`image`] shows.
pub fn name_of(area: &gtk::DrawingArea) -> Option<String> {
    // SAFETY: the key is only ever set to a `String` (see `set_name`).
    unsafe {
        area.data::<String>(NAME_KEY)
            .map(|name| name.as_ref().clone())
    }
}

// ---- SVG path subset -----------------------------------------------------------------------

struct Tokens<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Tokens<'_> {
    fn skip(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b',')
        {
            self.at += 1;
        }
    }

    fn done(&mut self) -> bool {
        self.skip();
        self.at >= self.bytes.len()
    }

    fn command(&mut self) -> Option<u8> {
        self.skip();
        let byte = *self.bytes.get(self.at)?;
        if byte.is_ascii_alphabetic() && byte != b'e' && byte != b'E' {
            self.at += 1;
            Some(byte)
        } else {
            None
        }
    }

    fn number(&mut self) -> Option<f64> {
        self.skip();
        let start = self.at;
        let mut i = self.at;
        if matches!(self.bytes.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let mut digits = false;
        let mut dot = false;
        while let Some(&b) = self.bytes.get(i) {
            if b.is_ascii_digit() {
                digits = true;
                i += 1;
            } else if b == b'.' && !dot {
                dot = true;
                i += 1;
            } else {
                break;
            }
        }
        if !digits {
            return None;
        }
        if matches!(self.bytes.get(i), Some(b'e' | b'E')) {
            let mut j = i + 1;
            if matches!(self.bytes.get(j), Some(b'+' | b'-')) {
                j += 1;
            }
            if self.bytes.get(j).is_some_and(u8::is_ascii_digit) {
                while self.bytes.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                i = j;
            }
        }
        let value = std::str::from_utf8(&self.bytes[start..i])
            .ok()?
            .parse()
            .ok()?;
        self.at = i;
        Some(value)
    }

    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let value = match self.bytes.get(self.at)? {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.at += 1;
        Some(value)
    }
}

/// Add path `d` (SVG syntax subset) to the current cairo path.
pub fn trace(cr: &cairo::Context, d: &str) {
    let mut t = Tokens {
        bytes: d.as_bytes(),
        at: 0,
    };
    let (mut x, mut y) = (0.0f64, 0.0f64);
    let (mut start_x, mut start_y) = (0.0f64, 0.0f64);
    // Reflection points for S and T.
    let mut cubic: Option<(f64, f64)> = None;
    let mut quad: Option<(f64, f64)> = None;
    let mut command: Option<u8> = None;
    while !t.done() {
        if let Some(next) = t.command() {
            command = Some(next);
        }
        let Some(current) = command else { return };
        let relative = current.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        let mut next_cubic = None;
        let mut next_quad = None;
        match current.to_ascii_uppercase() {
            b'M' => {
                let (Some(nx), Some(ny)) = (t.number(), t.number()) else {
                    return;
                };
                (x, y) = (nx + ox, ny + oy);
                (start_x, start_y) = (x, y);
                cr.move_to(x, y);
                // Further pairs are line segments.
                command = Some(if relative { b'l' } else { b'L' });
            }
            b'L' => {
                let (Some(nx), Some(ny)) = (t.number(), t.number()) else {
                    return;
                };
                (x, y) = (nx + ox, ny + oy);
                cr.line_to(x, y);
            }
            b'H' => {
                let Some(nx) = t.number() else { return };
                x = nx + ox;
                cr.line_to(x, y);
            }
            b'V' => {
                let Some(ny) = t.number() else { return };
                y = ny + oy;
                cr.line_to(x, y);
            }
            b'C' => {
                let mut v = [0.0; 6];
                for value in &mut v {
                    let Some(n) = t.number() else { return };
                    *value = n;
                }
                let (x1, y1, x2, y2) = (v[0] + ox, v[1] + oy, v[2] + ox, v[3] + oy);
                (x, y) = (v[4] + ox, v[5] + oy);
                cr.curve_to(x1, y1, x2, y2, x, y);
                next_cubic = Some((x2, y2));
            }
            b'S' => {
                let mut v = [0.0; 4];
                for value in &mut v {
                    let Some(n) = t.number() else { return };
                    *value = n;
                }
                let (x1, y1) = cubic.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                let (x2, y2) = (v[0] + ox, v[1] + oy);
                (x, y) = (v[2] + ox, v[3] + oy);
                cr.curve_to(x1, y1, x2, y2, x, y);
                next_cubic = Some((x2, y2));
            }
            b'Q' => {
                let mut v = [0.0; 4];
                for value in &mut v {
                    let Some(n) = t.number() else { return };
                    *value = n;
                }
                let (qx, qy) = (v[0] + ox, v[1] + oy);
                let (ex, ey) = (v[2] + ox, v[3] + oy);
                quadratic(cr, (x, y), (qx, qy), (ex, ey));
                (x, y) = (ex, ey);
                next_quad = Some((qx, qy));
            }
            b'T' => {
                let (Some(nx), Some(ny)) = (t.number(), t.number()) else {
                    return;
                };
                let (qx, qy) = quad.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                let (ex, ey) = (nx + ox, ny + oy);
                quadratic(cr, (x, y), (qx, qy), (ex, ey));
                (x, y) = (ex, ey);
                next_quad = Some((qx, qy));
            }
            b'A' => {
                let (Some(rx), Some(ry), Some(rotation)) = (t.number(), t.number(), t.number())
                else {
                    return;
                };
                let (Some(large), Some(sweep)) = (t.flag(), t.flag()) else {
                    return;
                };
                let (Some(nx), Some(ny)) = (t.number(), t.number()) else {
                    return;
                };
                let (ex, ey) = (nx + ox, ny + oy);
                arc(cr, (x, y), (rx, ry), rotation, large, sweep, (ex, ey));
                (x, y) = (ex, ey);
            }
            b'Z' => {
                cr.close_path();
                (x, y) = (start_x, start_y);
                // Z takes no arguments: anything but a command ends the path.
                command = None;
            }
            _ => return,
        }
        cubic = next_cubic;
        quad = next_quad;
    }
}

fn quadratic(cr: &cairo::Context, from: (f64, f64), control: (f64, f64), to: (f64, f64)) {
    let c1 = (
        from.0 + 2.0 / 3.0 * (control.0 - from.0),
        from.1 + 2.0 / 3.0 * (control.1 - from.1),
    );
    let c2 = (
        to.0 + 2.0 / 3.0 * (control.0 - to.0),
        to.1 + 2.0 / 3.0 * (control.1 - to.1),
    );
    cr.curve_to(c1.0, c1.1, c2.0, c2.1, to.0, to.1);
}

/// SVG elliptical arc from `from` to `to` (endpoint parameterisation, SVG 1.1 appendix F.6).
fn arc(
    cr: &cairo::Context,
    from: (f64, f64),
    radii: (f64, f64),
    rotation: f64,
    large: bool,
    sweep: bool,
    to: (f64, f64),
) {
    let (mut rx, mut ry) = (radii.0.abs(), radii.1.abs());
    if rx == 0.0 || ry == 0.0 || from == to {
        cr.line_to(to.0, to.1);
        return;
    }
    let phi = rotation.to_radians();
    let (sin, cos) = phi.sin_cos();
    let dx = (from.0 - to.0) / 2.0;
    let dy = (from.1 - to.1) / 2.0;
    let x1 = cos * dx + sin * dy;
    let y1 = -sin * dx + cos * dy;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coefficient = if denominator == 0.0 {
        0.0
    } else {
        (numerator / denominator).max(0.0).sqrt()
    };
    if large == sweep {
        coefficient = -coefficient;
    }
    let cx1 = coefficient * rx * y1 / ry;
    let cy1 = -coefficient * ry * x1 / rx;
    let cx = cos * cx1 - sin * cy1 + (from.0 + to.0) / 2.0;
    let cy = sin * cx1 + cos * cy1 + (from.1 + to.1) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let dot = ux * vx + uy * vy;
        let length = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut value = (dot / length).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            value = -value;
        }
        value
    };
    let ux = (x1 - cx1) / rx;
    let uy = (y1 - cy1) / ry;
    let vx = (-x1 - cx1) / rx;
    let vy = (-y1 - cy1) / ry;
    let start = angle(1.0, 0.0, ux, uy);
    let mut delta = angle(ux, uy, vx, vy);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let matrix = cr.matrix();
    cr.translate(cx, cy);
    cr.rotate(phi);
    cr.scale(rx, ry);
    if delta >= 0.0 {
        cr.arc(0.0, 0.0, 1.0, start, start + delta);
    } else {
        cr.arc_negative(0.0, 0.0, 1.0, start, start + delta);
    }
    cr.set_matrix(matrix);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(name: &str) -> (usize, (f64, f64, f64, f64)) {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 48, 48).unwrap();
        let extents;
        {
            let cr = cairo::Context::new(&surface).unwrap();
            assert!(draw(&cr, name, 0.0, 0.0, 48.0, &gdk::RGBA::BLACK), "{name}");
            // Bounds of the stroked paths on the 24-unit grid.
            let probe = cairo::Context::new(&surface).unwrap();
            probe.set_line_width(STROKE);
            for path in paths(name).unwrap_or_default() {
                trace(&probe, path.trim_start_matches('F'));
            }
            extents = probe.path_extents().unwrap();
        }
        let data = surface.take_data().unwrap();
        let alpha = data.chunks(4).filter(|px| px[3] > 0).count();
        (alpha, extents)
    }

    #[test]
    fn every_glyph_paints_inside_its_grid() {
        for (name, _) in GLYPHS {
            let (alpha, (x0, y0, x1, y1)) = painted(name);
            assert!(alpha > 20, "{name} painted {alpha} pixels");
            assert!(x0 >= 0.0 && y0 >= 0.0, "{name} starts at {x0},{y0}");
            assert!(x1 <= 24.0 && y1 <= 24.0, "{name} ends at {x1},{y1}");
        }
        let (alpha, _) = painted("badge:TS");
        assert!(alpha > 20);
        assert!(!draw(
            &cairo::Context::new(cairo::ImageSurface::create(cairo::Format::ARgb32, 4, 4).unwrap())
                .unwrap(),
            "no-such-glyph",
            0.0,
            0.0,
            4.0,
            &gdk::RGBA::BLACK
        ));
    }

    #[test]
    fn arcs_reach_their_end_points() {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 4, 4).unwrap();
        let cr = cairo::Context::new(&surface).unwrap();
        trace(&cr, "M3 12a9 9 0 0 1 18 0");
        let (x, y) = cr.current_point().unwrap();
        assert!(
            (x - 21.0).abs() < 1e-6 && (y - 12.0).abs() < 1e-6,
            "{x},{y}"
        );
        // The upper half circle: its top is at y = 3.
        let (_, top, _, _) = cr.path_extents().unwrap();
        assert!((top - 3.0).abs() < 1e-6, "{top}");
        cr.new_path();
        // Compact numbers and implicit repetition.
        trace(&cr, "M1 1l1-.5.5.5h-1.5z");
        let (x, y) = cr.current_point().unwrap();
        assert!((x - 1.0).abs() < 1e-9 && (y - 1.0).abs() < 1e-9, "{x},{y}");
    }

    #[test]
    fn files_map_to_glyphs_by_name_then_suffix() {
        for (name, glyph) in [
            ("main.rs", "rust"),
            ("Cargo.toml", "toml"),
            ("Cargo.lock", "lock"),
            ("README.md", "book"),
            (".gitignore", "git"),
            ("package.json", "braces"),
            ("logo.PNG", "image"),
            ("Dockerfile", "package"),
            ("docker-compose.yml", "package"),
            ("build.sh", "terminal"),
            ("index.ts", "badge:TS"),
            ("App.stories.tsx", "code"),
            ("notes", "file"),
            ("archive.", "file"),
            (".env", "settings"),
        ] {
            assert_eq!(file_glyph(name), glyph, "{name}");
            assert!(exists(file_glyph(name)), "{name}");
        }
    }
}
