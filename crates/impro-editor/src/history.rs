//! Edit records retain changed ranges rather than whole document snapshots.
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[derive(Clone)]
struct Edit {
    offset: i32,
    text: String,
    inserted: bool,
}
struct Action {
    edits: Vec<Edit>,
    before: i32,
    after: i32,
    id: u64,
}
#[derive(Default)]
struct State {
    actions: Vec<Action>,
    index: usize,
    pending: Vec<Edit>,
    before: i32,
    next: u64,
    saved: u64,
}
#[derive(Clone)]
pub(crate) struct History(Rc<Inner>);
struct Inner {
    state: RefCell<State>,
    depth: Cell<usize>,
    restoring: Cell<bool>,
    buffer: glib::WeakRef<gtk::TextBuffer>,
}
impl History {
    pub fn new(buffer: &gtk::TextBuffer) -> Self {
        let this = Self(Rc::new(Inner {
            state: RefCell::new(State::default()),
            depth: Cell::new(0),
            restoring: Cell::new(false),
            buffer: buffer.downgrade(),
        }));
        let weak = Rc::downgrade(&this.0);
        buffer.connect_begin_user_action(move |b| {
            if let Some(h) = weak.upgrade() {
                if h.depth.get() == 0 {
                    h.state.borrow_mut().before = b
                        .iter_at_mark(&b.get_insert().expect("buffer insert mark"))
                        .offset();
                }
                h.depth.set(h.depth.get() + 1);
            }
        });
        let weak = Rc::downgrade(&this.0);
        buffer.connect_end_user_action(move |_| {
            if let Some(h) = weak.upgrade() {
                h.depth.set(h.depth.get().saturating_sub(1));
                if h.depth.get() == 0 {
                    Self(h).commit();
                }
            }
        });
        let weak = Rc::downgrade(&this.0);
        buffer.connect_insert_text(move |b, at, text| {
            if let Some(h) = weak.upgrade() {
                if !h.restoring.get() {
                    let mut s = h.state.borrow_mut();
                    if s.pending.is_empty() && h.depth.get() == 0 {
                        s.before = b
                            .iter_at_mark(&b.get_insert().expect("buffer insert mark"))
                            .offset();
                    }
                    s.pending.push(Edit {
                        offset: at.offset(),
                        text: text.to_string(),
                        inserted: true,
                    });
                }
            }
        });
        let weak = Rc::downgrade(&this.0);
        buffer.connect_delete_range(move |b, a, z| {
            if let Some(h) = weak.upgrade() {
                if !h.restoring.get() {
                    let text = b.text(a, z, true).unwrap_or_default().to_string();
                    let mut s = h.state.borrow_mut();
                    if s.pending.is_empty() && h.depth.get() == 0 {
                        s.before = b
                            .iter_at_mark(&b.get_insert().expect("buffer insert mark"))
                            .offset();
                    }
                    s.pending.push(Edit {
                        offset: a.offset(),
                        text,
                        inserted: false,
                    });
                }
            }
        });
        let weak = Rc::downgrade(&this.0);
        buffer.connect_changed(move |_| {
            if let Some(h) = weak.upgrade() {
                if h.depth.get() == 0 {
                    Self(h).commit();
                }
            }
        });
        this
    }
    fn commit(&self) {
        if self.0.restoring.get() {
            return;
        }
        let Some(b) = self.0.buffer.upgrade() else {
            return;
        };
        let cursor = b
            .iter_at_mark(&b.get_insert().expect("buffer insert mark"))
            .offset();
        let mut s = self.0.state.borrow_mut();
        if s.pending.is_empty() {
            return;
        }
        let index = s.index;
        s.actions.truncate(index);
        s.next += 1;
        let action = Action {
            edits: std::mem::take(&mut s.pending),
            before: s.before,
            after: cursor,
            id: s.next,
        };
        s.actions.push(action);
        s.index += 1;
    }
    pub fn mark_saved(&self) {
        self.commit();
        let mut s = self.0.state.borrow_mut();
        s.saved = if s.index == 0 {
            0
        } else {
            s.actions[s.index - 1].id
        };
    }
    pub fn invalidate_saved(&self) {
        self.0.state.borrow_mut().saved = u64::MAX;
    }
    pub fn at_saved(&self) -> bool {
        let s = self.0.state.borrow();
        let id = if s.index == 0 {
            0
        } else {
            s.actions[s.index - 1].id
        };
        s.pending.is_empty() && id == s.saved
    }
    pub fn undo(&self, redo: bool) {
        self.commit();
        let Some(b) = self.0.buffer.upgrade() else {
            return;
        };
        let (mut edits, cursor) = {
            let mut s = self.0.state.borrow_mut();
            let index = if redo {
                if s.index >= s.actions.len() {
                    return;
                }
                s.index
            } else {
                if s.index == 0 {
                    return;
                }
                s.index - 1
            };
            let action = &s.actions[index];
            let out = (
                action.edits.clone(),
                if redo { action.after } else { action.before },
            );
            s.index = if redo { index + 1 } else { index };
            out
        };
        if !redo {
            edits.reverse();
        }
        self.0.restoring.set(true);
        b.begin_user_action();
        for edit in edits {
            let mut at = b.iter_at_offset(edit.offset);
            if edit.inserted == redo {
                b.insert(&mut at, &edit.text);
            } else {
                let mut end = b.iter_at_offset(edit.offset + edit.text.chars().count() as i32);
                b.delete(&mut at, &mut end);
            }
        }
        b.place_cursor(&b.iter_at_offset(cursor.min(b.char_count())));
        b.end_user_action();
        self.0.restoring.set(false);
    }
    pub fn restoring(&self) -> bool {
        self.0.restoring.get()
    }
    pub fn len(&self) -> usize {
        self.0.state.borrow().actions.len()
    }
}
