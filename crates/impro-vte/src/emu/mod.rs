//! Portable terminal state and input protocols; independent of GTK and Win32.
pub mod keys;
pub mod quoting;
mod tracking;

pub use alacritty_terminal::term::cell::Cell;
pub use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use alacritty_terminal::{
    event::{Event, EventListener},
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionType},
    term::{cell::Flags, Config, TermMode},
    vte::{self, ansi},
    Term,
};
pub use alacritty_terminal::{grid, index, term};
use std::{cell::RefCell, rc::Rc, sync::OnceLock};

#[derive(Clone, Default)]
pub struct Events(Rc<RefCell<Vec<Event>>>);
impl EventListener for Events {
    fn send_event(&self, event: Event) {
        self.0.borrow_mut().push(event);
    }
}

struct Size(usize, usize);
impl Dimensions for Size {
    fn columns(&self) -> usize {
        self.0
    }
    fn screen_lines(&self) -> usize {
        self.1
    }
    fn total_lines(&self) -> usize {
        self.1
    }
}

#[derive(Default)]
struct Metadata {
    cwd: Option<String>,
    x10: bool,
}
impl vte::Perform for Metadata {
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if !ignore && intermediates == b"?" && matches!(action, 'h' | 'l') {
            for parameter in params {
                match parameter.first() {
                    Some(9) => self.x10 = action == 'h',
                    Some(1000 | 1002 | 1003) if action == 'h' => self.x10 = false,
                    _ => (),
                }
            }
        }
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if !ignore && intermediates.is_empty() && byte == b'c' {
            self.x10 = false;
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _: bool) {
        if params.first() == Some(&b"7".as_slice()) {
            self.cwd = params
                .get(1)
                .and_then(|s| std::str::from_utf8(s).ok())
                .map(str::to_owned);
        }
    }
}

pub struct Screen {
    pub term: Term<Events>,
    processor: ansi::Processor,
    metadata_parser: vte::Parser,
    metadata: Metadata,
    events: Events,
    pub title: Option<String>,
    config: Config,
    tracking: tracking::Tracking,
}
impl Screen {
    pub fn new(columns: usize, rows: usize, scrollback: usize) -> Self {
        let events = Events::default();
        Self {
            term: Term::new(
                Config {
                    scrolling_history: scrollback,
                    ..Config::default()
                },
                &Size(columns.max(2), rows.max(1)),
                events.clone(),
            ),
            processor: ansi::Processor::new(),
            metadata_parser: vte::Parser::new(),
            metadata: Metadata::default(),
            events,
            tracking: tracking::Tracking::new(columns.max(2), rows.max(1)),
            title: None,
            config: Config {
                scrolling_history: scrollback,
                ..Config::default()
            },
        }
    }
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.processor.advance(
            &mut tracking::Tracked {
                term: &mut self.term,
                state: &mut self.tracking,
            },
            bytes,
        );
        self.metadata_parser.advance(&mut self.metadata, bytes);
        let events = std::mem::take(&mut *self.events.0.borrow_mut());
        for event in &events {
            match event {
                Event::Title(title) => self.title = Some(title.clone()),
                Event::ResetTitle => self.title = None,
                _ => (),
            }
        }
        events
    }
    pub fn cwd(&self) -> Option<&str> {
        self.metadata.cwd.as_deref()
    }
    pub fn columns(&self) -> usize {
        self.term.columns()
    }
    pub fn rows(&self) -> usize {
        self.term.screen_lines()
    }
    pub fn history(&self) -> usize {
        self.term.history_size()
    }
    pub fn offset(&self) -> usize {
        self.term.grid().display_offset()
    }
    pub fn mode(&self) -> TermMode {
        let mut mode = *self.term.mode();
        if self.metadata.x10 {
            mode.insert(TermMode::MOUSE_REPORT_CLICK);
        }
        mode
    }
    pub fn x10_mouse(&self) -> bool {
        self.metadata.x10
    }
    pub fn set_cursor_style(&mut self, style: ansi::CursorStyle) {
        self.config.default_cursor_style = style;
        self.term.set_options(self.config.clone());
    }
    pub fn resize(&mut self, columns: usize, rows: usize, rewrap: bool) {
        let size = Size(columns.clamp(2, 32767), rows.clamp(1, 32767));
        if (size.0, size.1) != (self.columns(), self.rows()) {
            self.tracking.resize(size.0, size.1);
        }
        if rewrap || self.mode().contains(TermMode::ALT_SCREEN) {
            self.term.resize(size);
        } else if (size.0, size.1) != (self.columns(), self.rows()) {
            // Let Term resize its tabs, margins, inactive grid and damage tracking,
            // while retaining ownership of the active grid for a non-reflow resize.
            let placeholder = grid::Grid::new(self.rows(), self.columns(), 0);
            let mut active = std::mem::replace(self.term.grid_mut(), placeholder);
            self.term.resize(Size(size.0, size.1));
            active.resize(false, size.1, size.0);
            *self.term.grid_mut() = active;
        }
    }
    pub fn set_scrollback(&mut self, lines: usize) {
        let old = self.history();
        self.config.scrolling_history = lines;
        self.term.set_options(self.config.clone());
        self.tracking.origin += old.saturating_sub(self.history()) as i64;
    }
    pub fn scroll(&mut self, delta: i32) {
        self.term.scroll_display(Scroll::Delta(delta));
    }
    pub fn bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }
    pub fn reset(&mut self, clear_tabstops: bool, clear_history: bool) {
        use ansi::Handler;
        if self.mode().contains(TermMode::ALT_SCREEN) {
            self.term.swap_alt();
        }
        let columns = self.columns();
        let rows = self.rows();
        let pending_wrap = self.term.grid().cursor.input_needs_wrap;
        let origin = self.tracking.origin;
        let tabs = if !clear_tabstops {
            Some(self.tracking.tabs.clone())
        } else {
            None
        };
        let active = if !clear_history {
            Some(std::mem::replace(
                self.term.grid_mut(),
                grid::Grid::new(rows, columns, 0),
            ))
        } else {
            None
        };
        self.term.reset_state();
        self.tracking = tracking::Tracking::new(columns, rows);
        if let Some(mut active) = active {
            let point = active.cursor.point;
            active.cursor = grid::Cursor {
                point,
                input_needs_wrap: pending_wrap,
                ..Default::default()
            };
            active.saved_cursor = Default::default();
            *self.term.grid_mut() = active;
            self.tracking.origin = origin;
        }
        if let Some(tabs) = tabs {
            self.term.clear_tabs(ansi::TabulationClearMode::All);
            let point = self.term.grid().cursor.point;
            for (column, set) in tabs.iter().enumerate() {
                if *set {
                    self.term.goto(0, column);
                    self.term.set_horizontal_tabstop();
                }
            }
            self.term.goto(point.line.0, point.column.0);
            self.tracking.tabs = tabs;
        }
        self.term.grid_mut().cursor.input_needs_wrap = pending_wrap && !clear_history;
        self.processor = ansi::Processor::new();
        self.metadata.x10 = false;
    }
    pub fn cursor(&self) -> (i64, i64) {
        let p = self.term.grid().cursor.point;
        (
            p.column.0 as i64
                + if self.term.grid().cursor.input_needs_wrap {
                    1
                } else {
                    0
                },
            p.line.0 as i64
                + if self.mode().contains(TermMode::ALT_SCREEN) {
                    0
                } else {
                    self.tracking.origin + self.history() as i64
                },
        )
    }
    pub fn visible_point(&self, column: usize, row: usize) -> Point {
        Point::new(
            Line(row.min(self.rows() - 1) as i32 - self.offset() as i32),
            Column(column.min(self.columns() - 1)),
        )
    }
    pub fn select_start(&mut self, column: usize, row: usize) {
        self.term.selection = Some(Selection::new(
            SelectionType::Simple,
            self.visible_point(column, row),
            Side::Left,
        ));
    }
    pub fn select_update(&mut self, column: usize, row: usize) {
        let p = self.visible_point(column, row);
        if let Some(s) = &mut self.term.selection {
            s.update(p, Side::Right);
        }
    }
    pub fn select_all(&mut self) {
        let mut last = Line(self.term.grid().cursor.point.line.0.saturating_sub(1));
        for row in (-(self.history() as i32)..self.rows() as i32).rev() {
            let line = &self.term.grid()[Line(row)];
            if (0..self.columns()).any(|col| {
                let cell = &line[Column(col)];
                cell.c != ' ' || cell.zerowidth().is_some_and(|s| !s.is_empty())
            }) {
                last = Line(last.0.max(row));
                break;
            }
        }
        let mut selection = Selection::new(
            SelectionType::Lines,
            Point::new(Line(-(self.history() as i32)), Column(0)),
            Side::Left,
        );
        selection.update(
            Point::new(Line(last.0.max(0)), Column(self.columns() - 1)),
            Side::Right,
        );
        self.term.selection = Some(selection);
    }
    pub fn selection(&self) -> Option<String> {
        self.term.selection_to_string()
    }
    /// The viewport text follows VTE: soft wraps join and hard lines end in LF.
    pub fn viewport_text(&self) -> String {
        let mut text = String::new();
        let origin = if self.mode().contains(TermMode::ALT_SCREEN) {
            0
        } else {
            self.tracking.origin
        };
        let top = origin + self.history() as i64 - self.offset() as i64;
        for row in top..top + self.rows() as i64 {
            text.push_str(&self.range(row, 0, row, self.columns() as i64));
            if !self.term.grid()[Line((row - origin - self.history() as i64) as i32)]
                [Column(self.columns() - 1)]
            .flags
            .contains(Flags::WRAPLINE)
            {
                text.push('\n');
            }
        }
        text
    }
    pub fn text(&self) -> String {
        self.term.bounds_to_string(
            Point::new(Line(-(self.history() as i32)), Column(0)),
            Point::new(Line(self.rows() as i32 - 1), Column(self.columns() - 1)),
        )
    }
    /// VTE's native range uses a linear span and an exclusive final column.
    /// Soft wraps join; wide continuation slots never duplicate their glyph.
    pub fn range(
        &self,
        start_row: i64,
        start_column: i64,
        end_row: i64,
        end_column: i64,
    ) -> String {
        let origin = if self.mode().contains(TermMode::ALT_SCREEN) {
            0
        } else {
            self.tracking.origin
        };
        let start_row = start_row - origin;
        let end_row = end_row - origin;
        let count = (self.history() + self.rows()) as i64;
        if start_row > end_row || end_row < 0 || start_row >= count {
            return String::new();
        }
        let top = start_row.max(0);
        let bottom = end_row.min(count - 1);
        let mut result = String::new();
        for row in top..=bottom {
            let line = &self.term.grid()[Line((row - self.history() as i64) as i32)];
            let left = if row == start_row {
                start_column.max(0).min(self.columns() as i64) as usize
            } else {
                0
            };
            let right = if row == end_row {
                end_column.max(0).min(self.columns() as i64) as usize
            } else {
                self.columns()
            };
            let begin = result.len();
            let mut tab_mode = (0..left)
                .rev()
                .find_map(|col| {
                    let c = line[Column(col)].c;
                    (c != ' ').then_some(c)
                })
                .is_some_and(|c| c == '\t');
            for col in left..right {
                let cell = &line[Column(col)];
                if tab_mode {
                    if self.tracking.tabs[col] || cell.c != ' ' {
                        tab_mode = false;
                    } else {
                        continue;
                    }
                }
                if cell.c == '\t' {
                    tab_mode = true;
                }
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                result.push(cell.c);
                if let Some(zero) = cell.zerowidth() {
                    result.extend(zero);
                }
            }
            while result.len() > begin && result.ends_with(' ') {
                result.pop();
            }
            if row < bottom
                && !line[Column(self.columns() - 1)]
                    .flags
                    .contains(Flags::WRAPLINE)
            {
                result.push('\n');
            }
        }
        result
    }
    pub fn hyperlink(&self, column: usize, row: usize) -> Option<String> {
        let point = self.visible_point(column, row);
        let cell = &self.term.grid()[point];
        if let Some(link) = cell.hyperlink() {
            return Some(link.uri().to_owned());
        }
        // Gather the logical line, including soft wraps, and map the target cell to
        // its UTF-8 byte offset so links around wide/combining glyphs match correctly.
        let mut first = point.line;
        while first.0 > -(self.history() as i32)
            && self.term.grid()[Line(first.0 - 1)][Column(self.columns() - 1)]
                .flags
                .contains(Flags::WRAPLINE)
        {
            first -= 1;
        }
        let mut last = point.line;
        while last.0 < self.rows() as i32 - 1
            && self.term.grid()[last][Column(self.columns() - 1)]
                .flags
                .contains(Flags::WRAPLINE)
        {
            last += 1;
        }
        let mut text = String::new();
        let mut target = 0;
        for line in first.0..=last.0 {
            for col in 0..self.columns() {
                let cell = &self.term.grid()[Line(line)][Column(col)];
                if line == point.line.0 && col == point.column.0 {
                    target = text.len();
                }
                if !cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    text.push(cell.c);
                    if let Some(zero) = cell.zerowidth() {
                        text.extend(zero);
                    }
                }
            }
        }
        hyperlink_at(&text, target).map(str::to_owned)
    }
    pub fn hyperlink_points(&self, uri: &str) -> Vec<Point> {
        let top = -(self.offset() as i32);
        let bottom = top + self.rows() as i32 - 1;
        let mut first = top;
        while first > -(self.history() as i32)
            && self.term.grid()[Line(first - 1)][Column(self.columns() - 1)]
                .flags
                .contains(Flags::WRAPLINE)
        {
            first -= 1;
        }
        let mut last = bottom;
        while last < self.rows() as i32 - 1
            && self.term.grid()[Line(last)][Column(self.columns() - 1)]
                .flags
                .contains(Flags::WRAPLINE)
        {
            last += 1;
        }
        let mut result = Vec::new();
        let mut text = String::new();
        let mut positions = Vec::new();
        for row in first..=last {
            let line = &self.term.grid()[Line(row)];
            for col in 0..self.columns() {
                let cell = &line[Column(col)];
                let point = Point::new(Line(row), Column(col));
                if (top..=bottom).contains(&row) && cell.hyperlink().is_some_and(|h| h.uri() == uri)
                {
                    result.push(point);
                }
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                let start = text.len();
                text.push(cell.c);
                if let Some(zero) = cell.zerowidth() {
                    text.extend(zero);
                }
                positions.push((start, text.len(), point));
            }
            if !line[Column(self.columns() - 1)]
                .flags
                .contains(Flags::WRAPLINE)
                || row == last
            {
                for found in url_regex().find_iter(&text) {
                    if trim_url(found.as_str()) == uri {
                        result.extend(
                            positions
                                .iter()
                                .filter(|(start, end, point)| {
                                    *start < found.start() + uri.len()
                                        && *end > found.start()
                                        && (top..=bottom).contains(&point.line.0)
                                })
                                .map(|(_, _, p)| *p),
                        );
                    }
                }
                text.clear();
                positions.clear();
            }
        }
        result.sort_unstable();
        result.dedup();
        result
    }
    pub fn html(
        &self,
        selected: bool,
        resolve: impl Fn(Color, bool) -> (u8, u8, u8),
        bold_bright: bool,
    ) -> Option<String> {
        self.html_inner(selected, None, resolve, bold_bright)
    }
    pub fn html_range(
        &self,
        bounds: (i64, i64, i64, i64),
        resolve: impl Fn(Color, bool) -> (u8, u8, u8),
        bold_bright: bool,
    ) -> String {
        self.html_inner(false, Some(bounds), resolve, bold_bright)
            .unwrap_or_default()
    }
    fn html_inner(
        &self,
        selected: bool,
        bounds: Option<(i64, i64, i64, i64)>,
        resolve: impl Fn(Color, bool) -> (u8, u8, u8),
        bold_bright: bool,
    ) -> Option<String> {
        use std::fmt::Write;
        let selection = if selected {
            Some(self.term.selection.as_ref()?.to_range(&self.term)?)
        } else {
            None
        };
        let origin = if self.mode().contains(TermMode::ALT_SCREEN) {
            0
        } else {
            self.tracking.origin
        };
        let bounds = bounds.map(|(a, b, c, d)| {
            (
                (a - origin - self.history() as i64)
                    .clamp(-(self.history() as i64), self.rows() as i64) as i32,
                b,
                (c - origin - self.history() as i64)
                    .clamp(-(self.history() as i64) - 1, self.rows() as i64 - 1)
                    as i32,
                d,
            )
        });
        let first = bounds.map_or_else(
            || selection.map_or(-(self.offset() as i32), |s| s.start.line.0),
            |b| b.0,
        );
        let last = bounds.map_or_else(
            || selection.map_or(first + self.rows() as i32 - 1, |s| s.end.line.0),
            |b| b.2,
        );
        let mut output = String::from("<pre>");
        for row in first..=last {
            let line = &self.term.grid()[Line(row)];
            for col in 0..self.columns() {
                let point = Point::new(Line(row), Column(col));
                if selection.is_some_and(|s| !s.contains(point))
                    || bounds.is_some_and(|(a, b, c, d)| {
                        (row == a && (col as i64) < b) || (row == c && (col as i64) >= d)
                    })
                {
                    continue;
                }
                let cell = &line[Column(col)];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                let mut fg = resolve(cell.fg, bold_bright && cell.flags.contains(Flags::BOLD));
                let mut bg = resolve(cell.bg, false);
                if cell.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let _=write!(output,"<span style=\"color:#{:02x}{:02x}{:02x};background-color:#{:02x}{:02x}{:02x};{}{}{}{}{}\">",fg.0,fg.1,fg.2,bg.0,bg.1,bg.2,
                    if cell.flags.contains(Flags::BOLD){"font-weight:bold;"}else{""},
                    if cell.flags.contains(Flags::ITALIC){"font-style:italic;"}else{""},
                    if cell.flags.intersects(Flags::ALL_UNDERLINES){"text-decoration:underline;"}else{""},
                    if cell.flags.contains(Flags::STRIKEOUT){"text-decoration:line-through;"}else{""},
                    if cell.flags.contains(Flags::DIM){"opacity:0.66;"}else{""});
                if !cell.flags.contains(Flags::HIDDEN) {
                    let mut text = cell.c.to_string();
                    if let Some(zero) = cell.zerowidth() {
                        text.extend(zero);
                    }
                    let escaped = html(&text);
                    output.push_str(&escaped[5..escaped.len() - 6]);
                }
                output.push_str("</span>");
            }
            if (bounds.is_none() || row < last)
                && !line[Column(self.columns() - 1)]
                    .flags
                    .contains(Flags::WRAPLINE)
            {
                output.push('\n');
            }
        }
        output.push_str("</pre>");
        Some(output)
    }
    pub fn paste(&self, text: &str) -> Vec<u8> {
        // Remove control terminators so pasted text cannot escape bracketed mode.
        let clean = text.replace(['\x1b', '\0'], "").replace("\r\n", "\n");
        if self.mode().contains(TermMode::BRACKETED_PASTE) {
            format!("\x1b[200~{clean}\x1b[201~").into_bytes()
        } else {
            clean.replace('\n', "\r").into_bytes()
        }
    }
}

fn url_regex() -> &'static regex::Regex {
    static URL: OnceLock<regex::Regex> = OnceLock::new();
    URL.get_or_init(|| regex::Regex::new(r#"(?:https?|ftp|file)://[^\s<>\x00-\x20\"']+"#).unwrap())
}
fn trim_url(text: &str) -> &str {
    text.trim_end_matches(['.', ',', ';', ':', ')', ']', '}'])
}
pub fn hyperlink_at(text: &str, byte: usize) -> Option<&str> {
    url_regex().find_iter(text).find_map(|m| {
        let link = trim_url(m.as_str());
        (byte >= m.start() && byte < m.start() + link.len()).then_some(link)
    })
}
pub fn html(text: &str) -> String {
    format!(
        "<pre>{}</pre>",
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    )
}

/// Encode an xterm mouse packet. `button`: 0/1/2, 64/65 (wheel), 3 (release).
pub fn mouse(
    mode: TermMode,
    button: u8,
    column: usize,
    row: usize,
    modifiers: u8,
    release: bool,
) -> Vec<u8> {
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return Vec::new();
    }
    let code = button | modifiers;
    if mode.contains(TermMode::SGR_MOUSE) {
        format!(
            "\x1b[<{code};{};{}{}",
            column + 1,
            row + 1,
            if release { 'm' } else { 'M' }
        )
        .into_bytes()
    } else if mode.contains(TermMode::UTF8_MOUSE) && column < 2015 && row < 2015 {
        let mut packet = String::from("\x1b[M");
        packet.push(char::from(32 + if release { 3 | modifiers } else { code }));
        packet.push(char::from_u32(column as u32 + 33).unwrap());
        packet.push(char::from_u32(row as u32 + 33).unwrap());
        packet.into_bytes()
    } else if column < 223 && row < 223 {
        vec![
            27,
            b'[',
            b'M',
            32 + if release { 3 | modifiers } else { code },
            (column + 33) as u8,
            (row + 33) as u8,
        ]
    } else {
        Vec::new()
    }
}
