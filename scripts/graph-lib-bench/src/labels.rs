//! Pango label bitmaps, rendered once per (text, size) and cached.

use std::collections::HashMap;

/// A premultiplied ARGB32 (native endian) bitmap of one label, white text.
pub struct LabelBitmap {
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub data: Vec<u8>,
}

pub struct LabelCache {
    map: HashMap<(String, u32), usize>,
    pub items: Vec<LabelBitmap>,
    /// Bitmaps rendered (cache misses) and the time they took.
    pub renders: usize,
    pub render_ms: f64,
    scale: f64,
}

impl LabelCache {
    /// `scale`: device pixels per logical pixel.
    pub fn new(scale: f64) -> LabelCache {
        LabelCache {
            map: HashMap::new(),
            items: Vec::new(),
            renders: 0,
            render_ms: 0.0,
            scale,
        }
    }

    pub fn get(&mut self, text: &str, size: f32) -> usize {
        let key = (text.to_string(), size.round() as u32);
        if let Some(&i) = self.map.get(&key) {
            return i;
        }
        let t = std::time::Instant::now();
        let bitmap = render(text, size as f64 * self.scale);
        self.render_ms += t.elapsed().as_secs_f64() * 1e3;
        self.renders += 1;
        self.items.push(bitmap);
        self.map.insert(key, self.items.len() - 1);
        self.items.len() - 1
    }
}

fn render(text: &str, px: f64) -> LabelBitmap {
    let probe = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).expect("surface");
    let cr = cairo::Context::new(&probe).expect("context");
    let layout = pangocairo::functions::create_layout(&cr);
    let mut font = pango::FontDescription::from_string("Sans");
    font.set_absolute_size(px * pango::SCALE as f64);
    layout.set_font_description(Some(&font));
    layout.set_text(text);
    let (w, h) = layout.pixel_size();
    let (w, h) = (w.max(1) + 2, h.max(1) + 2);
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).expect("surface");
    {
        let cr = cairo::Context::new(&surface).expect("context");
        cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        cr.move_to(1.0, 1.0);
        pangocairo::functions::show_layout(&cr, &layout);
    }
    surface.flush();
    let stride = surface.stride();
    let data = surface.data().expect("pixels").to_vec();
    LabelBitmap {
        width: w,
        height: h,
        stride,
        data,
    }
}
