//! Preserve the VTE coordinate epoch and tab stops around the upstream VT engine.
use super::{Events, Term};
use alacritty_terminal::{
    grid::{Dimensions, GridCell},
    index::{Column, Line},
    term::TermMode,
    vte::ansi::{self, cursor_icon::CursorIcon, *},
};
use unicode_width::UnicodeWidthChar;

pub(super) struct Tracking {
    pub origin: i64,
    pub top: usize,
    pub bottom: usize,
    pub tabs: Vec<bool>,
}
impl Tracking {
    pub fn new(columns: usize, rows: usize) -> Self {
        Self {
            origin: 0,
            top: 0,
            bottom: rows,
            tabs: (0..columns).map(|c| c % 8 == 0).collect(),
        }
    }
    pub fn resize(&mut self, columns: usize, rows: usize) {
        self.top = 0;
        self.bottom = rows;
        let old = self.tabs.len();
        self.tabs.resize(columns, false);
        for c in old..columns {
            self.tabs[c] = c % 8 == 0;
        }
    }
}
pub(super) struct Tracked<'a> {
    pub term: &'a mut Term<Events>,
    pub state: &'a mut Tracking,
}
impl Tracked<'_> {
    fn scrolls(&self) -> bool {
        self.state.top == 0 && !self.term.mode().contains(TermMode::ALT_SCREEN)
    }
    fn record(&mut self, before: usize, lines: usize) {
        if self.scrolls() {
            self.state.origin +=
                (lines as i64 - (self.term.history_size() as i64 - before as i64)).max(0);
        }
    }
    fn at_bottom(&self) -> bool {
        self.scrolls() && self.term.grid().cursor.point.line.0 as usize + 1 == self.state.bottom
    }
}
macro_rules! forward {($( $name:ident($( $arg:ident:$ty:ty),*); )*)=>{$(fn $name(&mut self,$($arg:$ty),*){self.term.$name($($arg),*);})*};}
impl ansi::Handler for Tracked<'_> {
    fn input(&mut self, c: char) {
        let before = self.term.history_size();
        let cursor = &self.term.grid().cursor;
        let width = c.width().unwrap_or(0);
        let wraps = width > 0
            && self.at_bottom()
            && self.term.mode().contains(TermMode::LINE_WRAP)
            && (cursor.input_needs_wrap
                || (width == 2 && cursor.point.column.0 + 1 == self.term.columns()));
        self.term.input(c);
        self.record(before, usize::from(wraps));
    }
    fn linefeed(&mut self) {
        let before = self.term.history_size();
        let scroll = self.at_bottom();
        self.term.linefeed();
        self.record(before, usize::from(scroll));
    }
    fn newline(&mut self) {
        let before = self.term.history_size();
        let scroll = self.at_bottom();
        self.term.newline();
        self.record(before, usize::from(scroll));
    }
    fn scroll_up(&mut self, lines: usize) {
        let before = self.term.history_size();
        self.term.scroll_up(lines);
        self.record(before, lines.min(self.state.bottom - self.state.top));
    }
    fn delete_lines(&mut self, lines: usize) {
        let before = self.term.history_size();
        let from_top = self.term.grid().cursor.point.line.0 == 0;
        self.term.delete_lines(lines);
        self.record(
            before,
            if from_top {
                lines.min(self.state.bottom - self.state.top)
            } else {
                0
            },
        );
    }
    fn clear_screen(&mut self, mode: ClearMode) {
        let before = self.term.history_size();
        let lines = if matches!(mode, ClearMode::All) && self.scrolls() {
            (0..self.term.screen_lines())
                .rev()
                .find(|r| {
                    (0..self.term.columns())
                        .any(|c| !self.term.grid()[Line(*r as i32)][Column(c)].is_empty())
                })
                .map_or(0, |r| r + 1)
        } else {
            0
        };
        let saved = matches!(mode, ClearMode::Saved);
        self.term.clear_screen(mode);
        if saved && !self.term.mode().contains(TermMode::ALT_SCREEN) {
            self.state.origin += before as i64;
        } else {
            self.record(before, lines);
        }
    }
    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        let end = bottom.unwrap_or(self.term.screen_lines());
        if top < end {
            self.state.top = top.saturating_sub(1).min(self.term.screen_lines());
            self.state.bottom = end.min(self.term.screen_lines());
        }
        self.term.set_scrolling_region(top, bottom);
    }
    fn set_horizontal_tabstop(&mut self) {
        self.state.tabs[self.term.grid().cursor.point.column.0] = true;
        self.term.set_horizontal_tabstop();
    }
    fn clear_tabs(&mut self, mode: TabulationClearMode) {
        match mode {
            TabulationClearMode::All => self.state.tabs.fill(false),
            TabulationClearMode::Current => {
                self.state.tabs[self.term.grid().cursor.point.column.0] = false
            }
        };
        self.term.clear_tabs(mode);
    }
    fn reset_state(&mut self) {
        self.term.reset_state();
        *self.state = Tracking::new(self.term.columns(), self.term.screen_lines());
    }
    forward! {
        set_title(arg0: Option<String>);
        set_cursor_style(arg0: Option<CursorStyle>);
        set_cursor_shape(arg0: CursorShape);
        goto(arg0: i32, arg1: usize);
        goto_line(arg0: i32);
        goto_col(arg0: usize);
        insert_blank(arg0: usize);
        move_up(arg0: usize);
        move_down(arg0: usize);
        identify_terminal(arg0: Option<char>);
        device_status(arg0: usize);
        move_forward(arg0: usize);
        move_backward(arg0: usize);
        move_down_and_cr(arg0: usize);
        move_up_and_cr(arg0: usize);
        put_tab(arg0: u16);
        backspace();
        carriage_return();
        bell();
        substitute();
        scroll_down(arg0: usize);
        insert_blank_lines(arg0: usize);
        erase_chars(arg0: usize);
        delete_chars(arg0: usize);
        move_backward_tabs(arg0: u16);
        move_forward_tabs(arg0: u16);
        save_cursor_position();
        restore_cursor_position();
        clear_line(arg0: LineClearMode);
        set_tabs(arg0: u16);
        reverse_index();
        terminal_attribute(arg0: Attr);
        set_mode(arg0: Mode);
        unset_mode(arg0: Mode);
        report_mode(arg0: Mode);
        set_private_mode(arg0: PrivateMode);
        unset_private_mode(arg0: PrivateMode);
        report_private_mode(arg0: PrivateMode);
        set_keypad_application_mode();
        unset_keypad_application_mode();
        set_active_charset(arg0: CharsetIndex);
        configure_charset(arg0: CharsetIndex, arg1: StandardCharset);
        set_color(arg0: usize, arg1: Rgb);
        dynamic_color_sequence(arg0: String, arg1: usize, arg2: &str);
        reset_color(arg0: usize);
        clipboard_store(arg0: u8, arg1: &[u8]);
        clipboard_load(arg0: u8, arg1: &str);
        decaln();
        push_title();
        pop_title();
        text_area_size_pixels();
        text_area_size_chars();
        set_hyperlink(arg0: Option<Hyperlink>);
        set_mouse_cursor_icon(arg0: CursorIcon);
        report_keyboard_mode();
        push_keyboard_mode(arg0: KeyboardModes);
        pop_keyboard_modes(arg0: u16);
        set_keyboard_mode(arg0: KeyboardModes, arg1: KeyboardModesApplyBehavior);
        set_modify_other_keys(arg0: ModifyOtherKeys);
        report_modify_other_keys();
        set_scp(arg0: ScpCharPath, arg1: ScpUpdateMode);
    }
}
