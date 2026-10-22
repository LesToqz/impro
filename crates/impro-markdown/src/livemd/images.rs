//! Background work: local images decode and scale on a worker thread (once per
//! width), fenced code highlights on the same worker. Results return through a
//! `glib` channel and are applied only while the block they belong to is unchanged.
use super::{CodeHighlighter, CodeRun, ImageState, Inner};
use crate::mdview::{self, LinkTarget};
use gdk_pixbuf::{Colorspace, Pixbuf};
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Weak,
    sync::mpsc,
};

/// A decoded, scaled image ready for a `GtkImage`.
#[derive(Clone)]
pub struct Loaded {
    pub pixbuf: Pixbuf,
    pub natural_width: i32,
    pub path: PathBuf,
}

/// Work for the background thread.
pub(super) enum Job {
    Image {
        id: u64,
        slot: usize,
        target: String,
        source: PathBuf,
        width: i32,
        generation: u64,
    },
    Code {
        id: u64,
        hash: u64,
        language: String,
        content: String,
        highlighter: CodeHighlighter,
    },
}
struct RawImage {
    bytes: glib::Bytes,
    width: i32,
    height: i32,
    stride: i32,
    alpha: bool,
    natural_width: i32,
    path: PathBuf,
}
/// Worker results, delivered on the main thread.
pub(super) enum Msg {
    Image {
        id: u64,
        slot: usize,
        generation: u64,
        result: Option<RawImage>,
    },
    Code {
        id: u64,
        hash: u64,
        runs: Vec<CodeRun>,
        generation: u64,
    },
}

/// The worker thread and its return channel (both created on first use).
#[derive(Default)]
pub(super) struct Jobs {
    sender: RefCell<Option<mpsc::Sender<(u64, Job)>>>,
    source: RefCell<Option<glib::SourceId>>,
    generation: Cell<u64>,
    in_flight: Cell<usize>,
}
impl Jobs {
    pub fn in_flight(&self) -> usize {
        self.in_flight.get()
    }
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }
    /// Forget outstanding work: results of older generations are ignored.
    pub fn cancel(&self) {
        self.generation.set(self.generation.get() + 1);
        self.in_flight.set(0);
    }
    /// Stop the worker and the channel source.
    pub fn close(&self) {
        self.cancel();
        self.sender.borrow_mut().take();
        if let Some(source) = self.source.borrow_mut().take() {
            source.remove();
        }
    }
    pub fn submit(&self, weak: &Weak<Inner>, job: Job) {
        let mut sender = self.sender.borrow_mut();
        if sender.is_none() {
            let (jobs, queue) = mpsc::channel::<(u64, Job)>();
            let (back, front) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT_IDLE);
            let weak = weak.clone();
            let source = front.attach(None, move |msg| {
                if let Some(inner) = weak.upgrade() {
                    inner.jobs.in_flight.set(inner.jobs.in_flight.get().saturating_sub(1));
                    inner.deliver(msg);
                }
                glib::ControlFlow::Continue
            });
            *self.source.borrow_mut() = Some(source);
            std::thread::Builder::new()
                .name("impro-live-markdown".into())
                .spawn(move || {
                    for (generation, job) in queue {
                        let message = run(job, generation);
                        if back.send(message).is_err() {
                            break;
                        }
                    }
                })
                .ok();
            *sender = Some(jobs);
        }
        if let Some(tx) = sender.as_ref() {
            if tx.send((self.generation.get(), job)).is_ok() {
                self.in_flight.set(self.in_flight.get() + 1);
            }
        }
    }
}

fn run(job: Job, generation: u64) -> Msg {
    match job {
        Job::Code {
            id,
            hash,
            language,
            content,
            highlighter,
        } => Msg::Code {
            id,
            hash,
            runs: highlighter(&language, &content),
            generation,
        },
        Job::Image {
            id,
            slot,
            target,
            source,
            width,
            generation,
        } => Msg::Image {
            id,
            slot,
            generation,
            result: load(&target, &source, width),
        },
    }
}

fn load(target: &str, source: &std::path::Path, width: i32) -> Option<RawImage> {
    let Some(LinkTarget::File(path)) = mdview::link_target(target, source) else {
        return None;
    };
    let (_, natural_width, _) = Pixbuf::file_info(&path)?;
    let pixbuf = Pixbuf::from_file_at_scale(&path, natural_width.min(width), -1, true).ok()?;
    Some(RawImage {
        bytes: pixbuf.read_pixel_bytes(),
        width: pixbuf.width(),
        height: pixbuf.height(),
        stride: pixbuf.rowstride(),
        alpha: pixbuf.has_alpha(),
        natural_width,
        path,
    })
}

impl Inner {
    pub(super) fn deliver(&self, msg: Msg) {
        match msg {
            Msg::Code {
                id,
                hash,
                runs,
                generation,
            } if generation == self.jobs.generation() => self.highlight_done(id, hash, runs),
            Msg::Image {
                id,
                slot,
                generation,
                result,
            } if generation == self.jobs.generation() => self.image_done(id, slot, result),
            _ => {}
        }
    }

    /// Start loading image `slot` of block `id` on the worker.
    pub(super) fn request_image(&self, id: u64, slot: usize) {
        let target = {
            let blocks = self.blocks.borrow();
            let Some(index) = blocks.find_id(id) else {
                return;
            };
            match blocks.items[index].images.get(slot) {
                Some(image) if matches!(image.state, ImageState::Pending) => image.target.clone(),
                _ => return,
            }
        };
        let Some(source) = (self.source_path)() else {
            self.mark_failed(id, slot);
            return;
        };
        let width = self.column_width();
        self.set_state(id, slot, ImageState::Requested);
        self.jobs.submit(
            &self.weak,
            Job::Image {
                id,
                slot,
                target,
                source,
                width,
                generation: self.jobs.generation(),
            },
        );
    }

    fn set_state(&self, id: u64, slot: usize, state: ImageState) {
        let mut blocks = self.blocks.borrow_mut();
        if let Some(index) = blocks.find_id(id) {
            if let Some(image) = blocks.items[index].images.get_mut(slot) {
                image.state = state;
            }
        }
    }
    fn mark_failed(&self, id: u64, slot: usize) {
        self.set_state(id, slot, ImageState::Failed);
    }

    fn image_done(&self, id: u64, slot: usize, result: Option<RawImage>) {
        enum Was {
            Requested,
            Rescaling(Loaded),
        }
        let was = {
            let blocks = self.blocks.borrow();
            let Some(index) = blocks.find_id(id) else {
                return;
            };
            match blocks.items[index].images.get(slot).map(|i| &i.state) {
                Some(ImageState::Requested) => Was::Requested,
                Some(ImageState::Rescaling(old)) => Was::Rescaling(old.clone()),
                _ => return,
            }
        };
        let Some(raw) = result else {
            match was {
                Was::Requested => self.mark_failed(id, slot),
                Was::Rescaling(old) => self.set_state(id, slot, ImageState::Ready(old)),
            }
            return;
        };
        let pixbuf = Pixbuf::from_bytes(
            &raw.bytes,
            Colorspace::Rgb,
            raw.alpha,
            8,
            raw.width,
            raw.height,
            raw.stride,
        );
        let loaded = Loaded {
            pixbuf,
            natural_width: raw.natural_width,
            path: raw.path,
        };
        match was {
            Was::Requested => self.install_image(id, slot, loaded),
            Was::Rescaling(_) => self.swap_image(id, slot, loaded),
        }
    }

    /// Reserve the space of a freshly loaded image and realize its widget.
    fn install_image(&self, id: u64, slot: usize, loaded: Loaded) {
        let (index, a, lead, first) = {
            let blocks = self.blocks.borrow();
            let Some(index) = blocks.find_id(id) else {
                return;
            };
            let deco = &blocks.items[index];
            let Some(image) = deco.images.get(slot) else {
                return;
            };
            if !deco.painted || !matches!(image.state, ImageState::Requested) {
                return;
            }
            let a = blocks.spans[index].0 + image.rel_a;
            (index, a, deco.lead, image.rel_a == 0)
        };
        let height = loaded.pixbuf.height();
        let tag = self.image_tag(height, if first { lead } else { 0 });
        let end = {
            let blocks = self.blocks.borrow();
            blocks.spans[index].0 + blocks.items[index].images[slot].rel_b
        };
        let hidden = self.tag("hidden");
        self.buffer.apply_tag(&hidden, &self.iter(a + 1), &self.iter(end));
        self.buffer.apply_tag(&tag, &self.iter(a), &self.iter(a + 1));
        self.buffer.remove_tag(&hidden, &self.iter(a), &self.iter(a + 1));
        let stale_pad = {
            let mut blocks = self.blocks.borrow_mut();
            let deco = &mut blocks.items[index];
            deco.tags.push(tag.clone());
            let image = &mut deco.images[slot];
            image.state = ImageState::Ready(loaded);
            image.tag = Some(tag.clone());
            if !deco.applied.contains(&hidden) {
                deco.applied.push(hidden);
            }
            if first {
                deco.spacing = None;
                deco.lead_tag = Some(tag);
                deco.pad.take()
            } else {
                None
            }
        };
        if let Some(pad) = stale_pad {
            let start = self.blocks.borrow().spans[index].0;
            self.buffer.remove_tag(&pad, &self.iter(start), &self.iter(start + 1));
        }
        // Realize now when the block already has widgets (it is near the viewport).
        let realized = !self.blocks.borrow().items[index].widgets.is_empty();
        if realized {
            self.release(index);
            self.realize(index);
        }
        self.queue_layout();
    }

    /// Private tag that holds the space of an image line.
    pub(super) fn image_tag(&self, height: i32, lead: i32) -> gtk::TextTag {
        let tag = gtk::TextTag::new(None);
        tag.set_property("size-points", 1.0f64);
        tag.set_property(
            "foreground",
            self.style
                .borrow()
                .chrome
                .get("gutter_bg")
                .cloned()
                .unwrap_or_default()
                .as_str(),
        );
        tag.set_property("pixels-below-lines", 0);
        tag.set_property("pixels-above-lines", height + 8 + lead);
        if let Some(table) = self.buffer.tag_table() {
            table.add(&tag);
        }
        tag
    }

    /// Re-decode loaded images whose column width changed.
    pub(super) fn rescale_images(&self) {
        let width = self.column_width();
        let stale: Vec<(u64, usize, String)> = {
            let blocks = self.blocks.borrow();
            blocks
                .items
                .iter()
                .filter(|b| b.painted && !b.widgets.is_empty())
                .flat_map(|b| {
                    b.images.iter().enumerate().filter_map(move |(k, i)| match &i.state {
                        ImageState::Ready(l) if l.pixbuf.width() != width.min(l.natural_width) => {
                            Some((b.id, k, i.target.clone()))
                        }
                        _ => None,
                    })
                })
                .collect()
        };
        for (id, slot, _) in stale {
            self.request_rescale(id, slot, width);
        }
    }

    fn request_rescale(&self, id: u64, slot: usize, width: i32) {
        let (target, path) = {
            let blocks = self.blocks.borrow();
            let Some(index) = blocks.find_id(id) else {
                return;
            };
            match blocks.items[index].images.get(slot) {
                Some(image) => match &image.state {
                    ImageState::Ready(l) => (image.target.clone(), l.path.clone()),
                    _ => return,
                },
                None => return,
            }
        };
        let _ = path;
        let Some(source) = (self.source_path)() else {
            return;
        };
        // Keep the old pixbuf visible until the new one arrives.
        self.set_state_keep(id, slot);
        self.jobs.submit(
            &self.weak,
            Job::Image {
                id,
                slot,
                target,
                source,
                width,
                generation: self.jobs.generation(),
            },
        );
    }

    fn set_state_keep(&self, id: u64, slot: usize) {
        let mut blocks = self.blocks.borrow_mut();
        if let Some(index) = blocks.find_id(id) {
            if let Some(image) = blocks.items[index].images.get_mut(slot) {
                if let ImageState::Ready(l) = &image.state {
                    image.state = ImageState::Rescaling(l.clone());
                }
            }
        }
    }

    /// Swap in a rescaled pixbuf; update the reserved height and the widget.
    pub(super) fn swap_image(&self, id: u64, slot: usize, loaded: Loaded) {
        let (index, tag, first, lead, widget) = {
            let mut blocks = self.blocks.borrow_mut();
            let Some(index) = blocks.find_id(id) else {
                return;
            };
            let deco = &mut blocks.items[index];
            let lead = deco.lead;
            let Some(image) = deco.images.get_mut(slot) else {
                return;
            };
            if !matches!(image.state, ImageState::Rescaling(_)) {
                return;
            }
            image.state = ImageState::Ready(loaded.clone());
            let first = image.rel_a == 0;
            let tag = image.tag.clone();
            let widget = deco
                .widgets
                .iter()
                .find(|s| s.role == super::Role::Image)
                .map(|s| s.widget.clone());
            (index, tag, first, lead, widget)
        };
        let _ = index;
        if let Some(tag) = tag {
            tag.set_property(
                "pixels-above-lines",
                loaded.pixbuf.height() + 8 + if first { lead } else { 0 },
            );
        }
        if let Some(image) = widget.and_then(|w| w.downcast::<gtk::Image>().ok()) {
            image.set_from_pixbuf(Some(&loaded.pixbuf));
        }
        self.queue_layout();
    }

    /// Reservation tag for a decoded image at paint time (cache hit on repaint).
    pub(super) fn reserve_image(&self, out: &mut super::paint::Out, slot: usize, block_start: i32, lead: i32) {
        let image = &out.images[slot];
        let ImageState::Ready(loaded) = &image.state else {
            return;
        };
        let height = loaded.pixbuf.height();
        let a = block_start + image.rel_a;
        let end = block_start + image.rel_b;
        let first = image.rel_a == 0;
        let hidden = self.tag("hidden");
        self.buffer.apply_tag(&hidden, &self.iter(a + 1), &self.iter(end));
        let tag = self.image_tag(height, if first { lead } else { 0 });
        self.buffer.apply_tag(&tag, &self.iter(a), &self.iter(a + 1));
        self.buffer.remove_tag(&hidden, &self.iter(a), &self.iter(a + 1));
        out.tags.push(tag.clone());
        if !out.applied.contains(&hidden) {
            out.applied.push(hidden);
        }
        if first {
            out.spacing = None;
            out.lead_tag = Some(tag.clone());
        }
        out.images[slot].tag = Some(tag);
    }
}
