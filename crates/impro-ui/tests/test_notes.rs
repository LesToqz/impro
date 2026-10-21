mod common;
use common::*;
use gtk::prelude::*;
use impro_ui::notes::*;
use std::{cell::RefCell, rc::Rc};
#[test]
fn test_apply_to_buffer_is_one_user_action_and_selects_result() {
    run(|| {
        let buffer = gtk::TextBuffer::new(gtk::TextTagTable::NONE);
        buffer.set_text("hello world");
        buffer.select_range(&buffer.iter_at_offset(6), &buffer.iter_at_offset(11));
        let actions = Rc::new(RefCell::new(Vec::new()));
        let a = actions.clone();
        buffer.connect_begin_user_action(move |_| a.borrow_mut().push("begin"));
        let a = actions.clone();
        buffer.connect_end_user_action(move |_| a.borrow_mut().push("end"));
        apply_to_buffer("bold", &buffer).unwrap();
        assert_eq!(
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .unwrap(),
            "hello **world**"
        );
        assert_eq!(*actions.borrow(), ["begin", "end"]);
        let (a, b) = buffer.selection_bounds().unwrap();
        assert_eq!((a.offset(), b.offset()), (8, 13));
    });
}
#[test]
fn test_toolbar_has_a_button_per_command_and_emits_its_id() {
    run(|| {
        let toolbar = NoteToolbar::new();
        assert_eq!(toolbar.buttons().len(), 10);
        let emitted = Rc::new(RefCell::new(Vec::new()));
        let e = emitted.clone();
        toolbar.connect_command(move |_, id| e.borrow_mut().push(id.to_string()));
        for c in impro_core::notes::NOTE_COMMANDS {
            let b = toolbar.buttons()[c.id].clone();
            assert_eq!(b.tooltip_text().as_deref(), Some(c.label));
            assert!(!b.can_focus());
            b.emit_clicked();
        }
        assert_eq!(emitted.borrow().len(), 10);
        unsafe {
            toolbar.destroy();
        }
    });
}
#[test]
fn test_toolbar_controls_are_compact_and_groups_have_dividers() {
    run(|| {
        impro_ui::init();
        impro_ui::style::apply_style(None).unwrap();
        let toolbar = NoteToolbar::new();
        let window = window(&toolbar);
        for b in toolbar.buttons().values() {
            assert_eq!((b.allocated_width(), b.allocated_height()), (28, 28));
        }
        assert_eq!(
            toolbar
                .children()
                .iter()
                .filter(|w| w.is::<gtk::Separator>())
                .count(),
            3
        );
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_css_parses_without_transitions_for_every_theme() {
    run(|| {
        for t in impro_core::themes::themes() {
            let css = css(&t.ui);
            impro_ui::style::create_provider(&css).unwrap();
            assert!(!css.contains("transition"));
        }
    });
}
