//! Portable terminal state and input protocols; independent of GTK and Win32.
pub mod keys;
pub mod quoting;

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
}
impl vte::Perform for Metadata {
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
    scrollback: usize,
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
            title: None,
            scrollback,
        }
    }
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.processor.advance(&mut self.term, bytes);
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
        *self.term.mode()
    }
    pub fn resize(&mut self, columns: usize, rows: usize, rewrap: bool) {
        let size = Size(columns.clamp(2, 32767), rows.clamp(1, 32767));
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
        self.scrollback = lines;
        self.term.set_options(Config {
            scrolling_history: lines,
            ..Config::default()
        });
    }
    pub fn scroll(&mut self, delta: i32) {
        self.term.scroll_display(Scroll::Delta(delta));
    }
    pub fn bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }
    pub fn reset(&mut self, history: bool) {
        use ansi::Handler;
        self.term.reset_state();
        if history {
            self.term.grid_mut().clear_history();
        }
        self.processor = ansi::Processor::new();
    }
    pub fn cursor(&self) -> (i64, i64) {
        let p = self.term.grid().cursor.point;
        (p.column.0 as i64, p.line.0 as i64 + self.history() as i64)
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
        let mut selection = Selection::new(
            SelectionType::Simple,
            Point::new(Line(-(self.history() as i32)), Column(0)),
            Side::Left,
        );
        selection.update(
            Point::new(Line(self.rows() as i32 - 1), Column(self.columns() - 1)),
            Side::Right,
        );
        self.term.selection = Some(selection);
    }
    pub fn selection(&self) -> Option<String> {
        self.term.selection_to_string()
    }
    pub fn text(&self) -> String {
        self.term.bounds_to_string(
            Point::new(Line(-(self.history() as i32)), Column(0)),
            Point::new(Line(self.rows() as i32 - 1), Column(self.columns() - 1)),
        )
    }
    /// Inclusive cell rectangle; wide-cell continuation slots never duplicate glyphs.
    pub fn range(
        &self,
        start_row: i64,
        start_column: i64,
        end_row: i64,
        end_column: i64,
    ) -> String {
        let top = start_row.max(0).min((self.history() + self.rows()) as i64);
        let bottom = end_row.min((self.history() + self.rows()) as i64 - 1);
        let left = start_column.max(0) as usize;
        let right = end_column.max(0).min(self.columns() as i64 - 1) as usize;
        let mut result = String::new();
        for row in top..=bottom {
            let line = Line((row - self.history() as i64) as i32);
            let begin = result.len();
            for col in left..=right {
                let cell = &self.term.grid()[line][Column(col)];
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
            result.push('\n');
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

pub fn hyperlink_at(text: &str, byte: usize) -> Option<&str> {
    static URL: OnceLock<regex::Regex> = OnceLock::new();
    URL.get_or_init(|| regex::Regex::new(r#"(?:https?|ftp|file)://[^\s<>\x00-\x20\"']+"#).unwrap())
        .find_iter(text)
        .find_map(|m| {
            let link = m
                .as_str()
                .trim_end_matches(['.', ',', ';', ':', ')', ']', '}']);
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
