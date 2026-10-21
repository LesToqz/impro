//! Natural-width wrapping and centered height-for-width containers.
use gtk::{prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
mod centered {
    use super::*;
    pub struct CenteredColumn {
        pub max_width: Cell<i32>,
        pub side: Cell<i32>,
        pub top: Cell<i32>,
        pub bottom: Cell<i32>,
    }
    impl Default for CenteredColumn {
        fn default() -> Self {
            Self {
                max_width: Cell::new(700),
                side: Cell::new(32),
                top: Cell::new(48),
                bottom: Cell::new(64),
            }
        }
    }
    #[glib::object_subclass]
    impl ObjectSubclass for CenteredColumn {
        const NAME: &'static str = "ImproCenteredColumn";
        type Type = super::CenteredColumn;
        type ParentType = gtk::Bin;
    }
    impl ObjectImpl for CenteredColumn {}
    impl WidgetImpl for CenteredColumn {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }
        fn preferred_width(&self) -> (i32, i32) {
            let (low, nat) = self
                .obj()
                .child()
                .filter(|c| c.is_visible())
                .map(|c| c.preferred_width())
                .unwrap_or((0, 0));
            (
                low + 2 * self.side.get(),
                nat.max(self.max_width.get()) + 2 * self.side.get(),
            )
        }
        fn preferred_height_for_width(&self, width: i32) -> (i32, i32) {
            let extra = self.top.get() + self.bottom.get();
            let (low, nat) = self
                .obj()
                .child()
                .filter(|c| c.is_visible())
                .map(|c| {
                    c.preferred_height_for_width(
                        (width - 2 * self.side.get()).clamp(1, self.max_width.get()),
                    )
                })
                .unwrap_or((0, 0));
            (low + extra, nat + extra)
        }
        fn preferred_height(&self) -> (i32, i32) {
            self.preferred_height_for_width(self.max_width.get() + 2 * self.side.get())
        }
        fn preferred_width_for_height(&self, _height: i32) -> (i32, i32) {
            self.preferred_width()
        }
        fn size_allocate(&self, allocation: &gtk::Allocation) {
            self.obj().set_allocation(allocation);
            if let Some(child) = self.obj().child().filter(|c| c.is_visible()) {
                let width =
                    (allocation.width() - 2 * self.side.get()).clamp(1, self.max_width.get());
                child.size_allocate(&gtk::Allocation::new(
                    allocation.x() + (allocation.width() - width) / 2,
                    allocation.y() + self.top.get(),
                    width,
                    (allocation.height() - self.top.get() - self.bottom.get()).max(1),
                ));
            }
        }
    }
    impl ContainerImpl for CenteredColumn {}
    impl BinImpl for CenteredColumn {}
}
glib::wrapper! {pub struct CenteredColumn(ObjectSubclass<centered::CenteredColumn>) @extends gtk::Bin,gtk::Container,gtk::Widget,@implements gtk::Buildable;}
impl Default for CenteredColumn {
    fn default() -> Self {
        Self::new(700, 32, 48, 64)
    }
}
impl CenteredColumn {
    pub fn new(max_width: i32, side: i32, top: i32, bottom: i32) -> Self {
        let this: Self = glib::Object::new();
        this.imp().max_width.set(max_width.max(1));
        this.imp().side.set(side.max(0));
        this.imp().top.set(top.max(0));
        this.imp().bottom.set(bottom.max(0));
        this
    }
}
mod wrapping {
    use super::*;
    pub struct WrapBox {
        pub children: RefCell<Vec<gtk::Widget>>,
        pub spacing: Cell<i32>,
        pub line_spacing: Cell<i32>,
    }
    impl Default for WrapBox {
        fn default() -> Self {
            Self {
                children: RefCell::new(Vec::new()),
                spacing: Cell::new(6),
                line_spacing: Cell::new(6),
            }
        }
    }
    #[glib::object_subclass]
    impl ObjectSubclass for WrapBox {
        const NAME: &'static str = "ImproWrapBox";
        type Type = super::WrapBox;
        type ParentType = gtk::Container;
    }
    impl ObjectImpl for WrapBox {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_has_window(false);
        }
        fn dispose(&self) {
            let children = std::mem::take(&mut *self.children.borrow_mut());
            for child in children {
                child.unparent();
            }
        }
    }
    impl WrapBox {
        fn lines(&self, width: i32) -> Vec<Vec<(gtk::Widget, i32, i32)>> {
            let children = self.children.borrow().clone();
            let mut lines = Vec::new();
            let mut line = Vec::new();
            let mut used = 0;
            for child in children.into_iter().filter(|c| c.is_visible()) {
                let w = child.preferred_width().1;
                let h = child.preferred_height_for_width(w).1;
                if !line.is_empty() && used + self.spacing.get() + w > width {
                    lines.push(line);
                    line = Vec::new();
                    used = 0;
                }
                used += if line.is_empty() {
                    w
                } else {
                    self.spacing.get() + w
                };
                line.push((child, w, h));
            }
            if !line.is_empty() {
                lines.push(line);
            }
            lines
        }
    }
    impl WidgetImpl for WrapBox {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }
        fn preferred_width(&self) -> (i32, i32) {
            let children = self.children.borrow().clone();
            let widths: Vec<_> = children
                .iter()
                .filter(|c| c.is_visible())
                .map(|c| c.preferred_width())
                .collect();
            (
                widths.iter().map(|w| w.0).max().unwrap_or(0),
                widths.iter().map(|w| w.1).sum::<i32>()
                    + self.spacing.get() * (widths.len() as i32 - 1).max(0),
            )
        }
        fn preferred_height_for_width(&self, width: i32) -> (i32, i32) {
            let lines = self.lines(width);
            let h = lines
                .iter()
                .map(|line| line.iter().map(|(_, _, h)| *h).max().unwrap_or(0))
                .sum::<i32>()
                + self.line_spacing.get() * (lines.len() as i32 - 1).max(0);
            (h, h)
        }
        fn preferred_height(&self) -> (i32, i32) {
            self.preferred_height_for_width(self.preferred_width().1)
        }
        fn preferred_width_for_height(&self, _height: i32) -> (i32, i32) {
            self.preferred_width()
        }
        fn size_allocate(&self, allocation: &gtk::Allocation) {
            self.obj().set_allocation(allocation);
            let mut y = allocation.y();
            for line in self.lines(allocation.width()) {
                let mut x = allocation.x();
                let height = line.iter().map(|(_, _, h)| *h).max().unwrap_or(0);
                for (child, w, _) in line {
                    child.size_allocate(&gtk::Allocation::new(
                        x,
                        y,
                        w.min(allocation.width()),
                        height,
                    ));
                    x += w + self.spacing.get();
                }
                y += height + self.line_spacing.get();
            }
        }
    }
    impl ContainerImpl for WrapBox {
        fn add(&self, child: &gtk::Widget) {
            self.children.borrow_mut().push(child.clone());
            child.set_parent(&*self.obj());
            self.obj().queue_resize();
        }
        fn remove(&self, child: &gtk::Widget) {
            let removed = {
                let mut children = self.children.borrow_mut();
                children
                    .iter()
                    .position(|w| w == child)
                    .map(|i| children.remove(i))
            };
            if let Some(child) = removed {
                child.unparent();
                self.obj().queue_resize();
            }
        }
        fn child_type(&self) -> glib::Type {
            gtk::Widget::static_type()
        }
        fn forall(&self, _internals: bool, callback: &gtk::subclass::container::Callback) {
            let children = self.children.borrow().clone();
            for child in children {
                callback.call(&child);
            }
        }
    }
}
glib::wrapper! {pub struct WrapBox(ObjectSubclass<wrapping::WrapBox>) @extends gtk::Container,gtk::Widget,@implements gtk::Buildable;}
impl Default for WrapBox {
    fn default() -> Self {
        Self::new(6, 6)
    }
}
impl WrapBox {
    pub fn new(spacing: i32, line_spacing: i32) -> Self {
        let this: Self = glib::Object::new();
        this.imp().spacing.set(spacing.max(0));
        this.imp().line_spacing.set(line_spacing.max(0));
        this
    }
}
