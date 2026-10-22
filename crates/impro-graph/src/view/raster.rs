//! A small anti-aliased software rasteriser for the graph's primitives.
//!
//! Cairo's stroker needs about 6 µs per line and far more when many lines share
//! a path, which puts a 3 000-node map well over a frame. The primitives here
//! write straight into a premultiplied `ARGB32` buffer (the memory layout of
//! `cairo::Format::ARgb32`, little endian: B, G, R, A) and cost well under a
//! microsecond per marker or connection. Coordinates are device pixels.

/// Where the pixels live.
pub enum Pixels<'a> {
    Slice(&'a mut [u8]),
    Surface(cairo::ImageSurfaceData<'a>),
}

impl std::ops::Deref for Pixels<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Pixels::Slice(s) => s,
            Pixels::Surface(d) => d,
        }
    }
}

impl std::ops::DerefMut for Pixels<'_> {
    fn deref_mut(&mut self) -> &mut [u8] {
        match self {
            Pixels::Slice(s) => s,
            Pixels::Surface(d) => d,
        }
    }
}

/// An opaque pixel buffer.
pub struct Canvas<'a> {
    pub px: Pixels<'a>,
    pub w: usize,
    pub h: usize,
    pub stride: usize,
}

#[inline]
fn blend_channel(dst: u8, src: u8, a: u32) -> u8 {
    let d = dst as i32;
    let s = src as i32;
    (d + ((s - d) * a as i32 + 127) / 255) as u8
}

#[inline]
fn byte(v: f32) -> u8 {
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u8], w: usize, h: usize, stride: usize) -> Canvas<'a> {
        debug_assert!(px.len() >= stride * h && stride >= w * 4);
        Canvas {
            px: Pixels::Slice(px),
            w,
            h,
            stride,
        }
    }

    /// A canvas over the pixels of a cairo image surface.
    pub fn new_guarded(
        data: cairo::ImageSurfaceData<'a>,
        w: usize,
        h: usize,
        stride: usize,
    ) -> Canvas<'a> {
        Canvas {
            px: Pixels::Surface(data),
            w,
            h,
            stride,
        }
    }

    /// Fills the whole buffer with an opaque colour.
    pub fn clear(&mut self, rgb: [f32; 3]) {
        let pixel = [byte(rgb[2]), byte(rgb[1]), byte(rgb[0]), 255];
        for y in 0..self.h {
            let row = &mut self.px[y * self.stride..y * self.stride + self.w * 4];
            for p in row.chunks_exact_mut(4) {
                p.copy_from_slice(&pixel);
            }
        }
    }

    #[inline]
    fn blend(&mut self, x: usize, y: usize, src: [u8; 3], a: u32) {
        let i = y * self.stride + x * 4;
        if let Some(p) = self.px.get_mut(i..i + 3) {
            p[0] = blend_channel(p[0], src[2], a);
            p[1] = blend_channel(p[1], src[1], a);
            p[2] = blend_channel(p[2], src[0], a);
        }
    }

    /// Fills a run of fully covered pixels.
    #[inline]
    fn span(&mut self, y: usize, x0: usize, x1: usize, src: [u8; 3], a: u32) {
        if x1 <= x0 {
            return;
        }
        let row = &mut self.px[y * self.stride + x0 * 4..y * self.stride + x1 * 4];
        if a >= 255 {
            for p in row.chunks_exact_mut(4) {
                p[0] = src[2];
                p[1] = src[1];
                p[2] = src[0];
            }
        } else {
            for p in row.chunks_exact_mut(4) {
                p[0] = blend_channel(p[0], src[2], a);
                p[1] = blend_channel(p[1], src[1], a);
                p[2] = blend_channel(p[2], src[0], a);
            }
        }
    }

    /// An anti-aliased filled disc.
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, rgb: [f32; 3], alpha: f32) {
        if !(r > 0.0 && alpha > 0.0) || !cx.is_finite() || !cy.is_finite() {
            return;
        }
        let src = [byte(rgb[0]), byte(rgb[1]), byte(rgb[2])];
        let a = byte(alpha.min(1.0)) as u32;
        let outer = r + 0.5;
        let inner = (r - 0.5).max(0.0);
        let y0 = ((cy - outer).floor().max(0.0)) as usize;
        let y1 = ((cy + outer).ceil().min(self.h as f32)).max(0.0) as usize;
        for y in y0..y1 {
            let dy = y as f32 + 0.5 - cy;
            let o2 = outer * outer - dy * dy;
            if o2 <= 0.0 {
                continue;
            }
            let ox = o2.sqrt();
            let ix = (inner * inner - dy * dy).max(0.0).sqrt();
            let xa = ((cx - ox).floor().max(0.0)) as usize;
            let xb = ((cx + ox).ceil().min(self.w as f32)).max(0.0) as usize;
            // fully covered span
            let full0 = ((cx - ix).ceil().max(xa as f32)) as usize;
            let full1 = ((cx + ix).floor().min(xb as f32)).max(full0 as f32) as usize;
            let (full0, full1) = if inner > 0.0 && ix > 0.0 {
                (full0, full1)
            } else {
                (xb, xb)
            };
            for x in xa..full0.min(xb) {
                let dx = x as f32 + 0.5 - cx;
                let cov = (r + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, src, (a as f32 * cov) as u32);
                }
            }
            self.span(y, full0, full1, src, a);
            for x in full1.max(full0)..xb {
                let dx = x as f32 + 0.5 - cx;
                let cov = (r + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, src, (a as f32 * cov) as u32);
                }
            }
        }
    }

    /// An anti-aliased circle outline of the given line width.
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, width: f32, rgb: [f32; 3], alpha: f32) {
        if !(r > 0.0 && width > 0.0 && alpha > 0.0) {
            return;
        }
        let src = [byte(rgb[0]), byte(rgb[1]), byte(rgb[2])];
        let a = byte(alpha.min(1.0)) as f32;
        let reach = r + width / 2.0 + 1.0;
        let y0 = ((cy - reach).floor().max(0.0)) as usize;
        let y1 = ((cy + reach).ceil().min(self.h as f32)).max(0.0) as usize;
        let x0 = ((cx - reach).floor().max(0.0)) as usize;
        let x1 = ((cx + reach).ceil().min(self.w as f32)).max(0.0) as usize;
        for y in y0..y1 {
            let dy = y as f32 + 0.5 - cy;
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let d = ((dx * dx + dy * dy).sqrt() - r).abs();
                let cov = (width / 2.0 + 0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, src, (a * cov) as u32);
                }
            }
        }
    }

    /// A filled arrow head: tip, two wings and a notch (non-convex quad).
    #[allow(clippy::too_many_arguments)]
    pub fn dart(
        &mut self,
        tip: (f32, f32),
        wing_a: (f32, f32),
        notch: (f32, f32),
        wing_b: (f32, f32),
        rgb: [f32; 3],
        alpha: f32,
    ) {
        let src = [byte(rgb[0]), byte(rgb[1]), byte(rgb[2])];
        let a = byte(alpha.min(1.0)) as f32;
        let tris = [
            Triangle::new(tip, wing_a, notch),
            Triangle::new(tip, notch, wing_b),
        ];
        let xs = [tip.0, wing_a.0, notch.0, wing_b.0];
        let ys = [tip.1, wing_a.1, notch.1, wing_b.1];
        let min = |v: &[f32; 4]| v.iter().copied().fold(f32::MAX, f32::min);
        let max = |v: &[f32; 4]| v.iter().copied().fold(f32::MIN, f32::max);
        let x0 = ((min(&xs) - 1.0).floor().max(0.0)) as usize;
        let x1 = ((max(&xs) + 1.0).ceil().min(self.w as f32)).max(0.0) as usize;
        let y0 = ((min(&ys) - 1.0).floor().max(0.0)) as usize;
        let y1 = ((max(&ys) + 1.0).ceil().min(self.h as f32)).max(0.0) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let cov = (tris[0].coverage(px, py) + tris[1].coverage(px, py)).min(1.0);
                if cov > 0.0 {
                    self.blend(x, y, src, (a * cov) as u32);
                }
            }
        }
    }
}

/// Edge functions of one triangle, normalised to pixel distances.
struct Triangle {
    // a*x + b*y + c >= 0 inside
    e: [(f32, f32, f32); 3],
}

impl Triangle {
    fn new(p: (f32, f32), q: (f32, f32), r: (f32, f32)) -> Triangle {
        let area = (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
        let sign = if area >= 0.0 { 1.0 } else { -1.0 };
        let edge = |a: (f32, f32), b: (f32, f32)| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt().max(1e-6);
            // inside is to the left of a -> b for positive area
            let (na, nb) = (-dy / len * sign, dx / len * sign);
            (na, nb, -(na * a.0 + nb * a.1))
        };
        Triangle {
            e: [edge(p, q), edge(q, r), edge(r, p)],
        }
    }

    #[inline]
    fn coverage(&self, x: f32, y: f32) -> f32 {
        let mut c = 1.0f32;
        for &(a, b, k) in &self.e {
            c = c.min((a * x + b * y + k + 0.5).clamp(0.0, 1.0));
        }
        c
    }
}

/// Coverage mask for the union of many thick lines (one stroke of a single
/// path paints overlapping segments once). The mask is an `A8` cairo surface so
/// that cairo's vectorised compositing applies the colour in one operation.
#[derive(Default)]
pub struct LineMask {
    surface: Option<cairo::ImageSurface>,
    w: usize,
    h: usize,
    // touched region (inclusive start, exclusive end)
    bbox: [usize; 4],
    /// Per-band segment lists, reused between frames.
    lists: Vec<Vec<u32>>,
}

/// Write access to the mask while lines are added.
pub struct MaskLines<'a> {
    data: cairo::ImageSurfaceData<'a>,
    w: usize,
    h: usize,
    stride: usize,
    bbox: &'a mut [usize; 4],
    lists: &'a mut Vec<Vec<u32>>,
}

impl LineMask {
    /// Prepares an empty mask for a `w × h` device-pixel canvas drawn with the
    /// given device scale. Returns false if no surface can be created.
    pub fn begin(&mut self, w: usize, h: usize, scale: f64) -> bool {
        if self.surface.is_none() || self.w != w || self.h != h {
            self.surface = cairo::ImageSurface::create(cairo::Format::A8, w as i32, h as i32).ok();
            self.w = w;
            self.h = h;
        }
        if let Some(s) = &self.surface {
            s.set_device_scale(scale, scale);
        }
        self.bbox = [w, h, 0, 0];
        self.surface.is_some()
    }

    /// Exclusive access to the pixels.
    pub fn lines(&mut self) -> Option<MaskLines<'_>> {
        let surface = self.surface.as_mut()?;
        let stride = surface.stride() as usize;
        let data = surface.data().ok()?;
        Some(MaskLines {
            data,
            w: self.w,
            h: self.h,
            stride,
            bbox: &mut self.bbox,
            lists: &mut self.lists,
        })
    }

    /// Paints the accumulated coverage through `cr` in one colour and empties
    /// the mask. Release the [`MaskLines`] first.
    pub fn composite(&mut self, cr: &cairo::Context, rgb: [f32; 3], alpha: f32) {
        let [x0, y0, x1, y1] = self.bbox;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        cr.set_source_rgba(rgb[0] as f64, rgb[1] as f64, rgb[2] as f64, alpha as f64);
        let _ = cr.mask_surface(&*surface, 0.0, 0.0);
        let stride = surface.stride() as usize;
        if let Ok(mut data) = surface.data() {
            for y in y0..y1.min(self.h) {
                let row = &mut data[y * stride + x0..y * stride + x1.min(self.w)];
                row.fill(0);
            }
        }
        self.bbox = [self.w, self.h, 0, 0];
    }
}

/// Fewer segments than this are rasterised on the calling thread.
const PARALLEL_MIN_SEGMENTS: usize = 1500;

impl MaskLines<'_> {
    /// Adds one segment of the given width (butt ends).
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32) {
        self.segments(&[[x0, y0, x1, y1]], width, 1);
    }

    /// Adds many segments of one width. With `threads > 1` and enough work the
    /// canvas is cut into horizontal bands rasterised in parallel; the result
    /// is the same as in a single thread.
    pub fn segments(&mut self, segments: &[[f32; 4]], width: f32, threads: usize) {
        let bands = if segments.len() >= PARALLEL_MIN_SEGMENTS {
            threads
        } else {
            1
        };
        self.banded(segments, width, bands);
    }

    /// Like [`MaskLines::segments`] with an explicit thread count: the canvas
    /// is cut into many thin bands, each segment goes to the bands it touches,
    /// and the threads take bands from a shared queue.
    pub fn banded(&mut self, segments: &[[f32; 4]], width: f32, threads: usize) {
        if segments.is_empty() || self.h == 0 {
            return;
        }
        let (w, h, stride) = (self.w, self.h, self.stride);
        let threads = threads.clamp(1, 8);
        let buf: &mut [u8] = &mut self.data;
        let mut merged = [w as i32, h as i32, 0i32, 0i32];
        let mut merge = |b: [i32; 4]| {
            if b[2] > b[0] && b[3] > b[1] {
                merged[0] = merged[0].min(b[0]);
                merged[1] = merged[1].min(b[1]);
                merged[2] = merged[2].max(b[2]);
                merged[3] = merged[3].max(b[3]);
            }
        };
        if threads == 1 {
            let mut band = Band::new(buf, stride, w, 0, h);
            for s in segments {
                band.line(s[0], s[1], s[2], s[3], width);
            }
            merge(band.bounds);
        } else {
            let bands = (threads * 4).min(h.div_ceil(8)).max(1);
            let rows = h.div_ceil(bands);
            let bands = h.div_ceil(rows);
            let margin = width / 2.0 + 1.0;
            for list in self.lists.iter_mut() {
                list.clear();
            }
            self.lists.resize_with(bands, Vec::new);
            for (i, s) in segments.iter().enumerate() {
                let (top, bottom) = (s[1].min(s[3]) - margin, s[1].max(s[3]) + margin);
                if !(top.is_finite() && bottom.is_finite()) || bottom < 0.0 || top > h as f32 {
                    continue;
                }
                let first = (top.max(0.0) as usize / rows).min(bands - 1);
                let last = (bottom.max(0.0) as usize / rows).min(bands - 1);
                for list in &mut self.lists[first..=last] {
                    list.push(i as u32);
                }
            }
            let lists = &self.lists;
            let queue = std::sync::Mutex::new(buf.chunks_mut(rows * stride).enumerate());
            let work = || {
                let mut touched: Vec<[i32; 4]> = Vec::new();
                loop {
                    let item = queue.lock().ok().and_then(|mut q| q.next());
                    let Some((k, chunk)) = item else {
                        break;
                    };
                    let (y0, y1) = (k * rows, ((k + 1) * rows).min(h));
                    let mut band = Band::new(chunk, stride, w, y0, y1);
                    for &i in &lists[k] {
                        let s = &segments[i as usize];
                        band.line(s[0], s[1], s[2], s[3], width);
                    }
                    touched.push(band.bounds);
                }
                touched
            };
            let results: Vec<Vec<[i32; 4]>> = std::thread::scope(|scope| {
                let helpers: Vec<_> = (1..threads).map(|_| scope.spawn(work)).collect();
                let mut all = vec![work()];
                all.extend(helpers.into_iter().map(|h| h.join().unwrap_or_default()));
                all
            });
            for r in results.into_iter().flatten() {
                merge(r);
            }
        }
        self.bbox[0] = self.bbox[0].min(merged[0].max(0) as usize);
        self.bbox[1] = self.bbox[1].min(merged[1].max(0) as usize);
        self.bbox[2] = self.bbox[2].max(merged[2].max(0) as usize);
        self.bbox[3] = self.bbox[3].max(merged[3].max(0) as usize);
    }
}

/// Rows `y0..y1` of the mask.
struct Band<'b> {
    buf: &'b mut [u8],
    stride: usize,
    w: i32,
    y0: i32,
    y1: i32,
    /// Touched region: left, top, right, bottom (absolute rows).
    bounds: [i32; 4],
}

impl<'b> Band<'b> {
    fn new(buf: &'b mut [u8], stride: usize, w: usize, y0: usize, y1: usize) -> Band<'b> {
        Band {
            buf,
            stride,
            w: w as i32,
            y0: y0 as i32,
            y1: y1 as i32,
            bounds: [w as i32, y1 as i32, 0, y0 as i32],
        }
    }

    #[inline]
    fn cell(&mut self, row: i32, col: i32) -> &mut u8 {
        &mut self.buf[(row - self.y0) as usize * self.stride + col as usize]
    }

    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32) {
        const ONE: i32 = 1 << 16;
        let (w, band_top, band_bottom) = (self.w, self.y0, self.y1);
        let half = width / 2.0;
        let margin = half + 1.0;
        let (wf, bt, bb) = (w as f32, band_top as f32, band_bottom as f32);
        if (y0 < bt - margin && y1 < bt - margin) || (y0 > bb + margin && y1 > bb + margin) {
            return;
        }
        let inside = x0 >= 0.0
            && x0 <= wf
            && x1 >= 0.0
            && x1 <= wf
            && y0 >= bt
            && y0 <= bb
            && y1 >= bt
            && y1 <= bb;
        let (x0, y0, x1, y1) = if inside {
            (x0, y0, x1, y1)
        } else {
            match clip(
                x0,
                y0,
                x1,
                y1,
                -margin,
                bt - margin,
                wf + margin,
                bb + margin,
            ) {
                Some(c) => c,
                None => return,
            }
        };
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-3 {
            return;
        }
        if dx.abs() >= dy.abs() {
            let (x0, y0, x1, y1) = if x0 <= x1 {
                (x0, y0, x1, y1)
            } else {
                (x1, y1, x0, y0)
            };
            let slope = (y1 - y0) / (x1 - x0);
            let hv = (half * len / (x1 - x0) * 65536.0) as i32;
            let xs = (x0.floor() as i32).max(0);
            let xe = (x1.ceil() as i32).min(w);
            if xe <= xs {
                return;
            }
            let step = (slope * 65536.0) as i32;
            let mut yc = ((y0 + ((xs as f32 + 0.5) - x0) * slope) * 65536.0) as i32;
            let (mut lo, mut hi) = (band_bottom, band_top);
            for x in xs..xe {
                let top = yc - hv;
                let bot = yc + hv;
                yc += step;
                let r0 = top >> 16;
                let r1 = bot >> 16;
                if r0 < band_top || r1 >= band_bottom {
                    for r in r0.max(band_top)..=r1.min(band_bottom - 1) {
                        let cov = (bot.min((r + 1) << 16) - top.max(r << 16)).clamp(0, ONE);
                        let m = (cov >> 8).min(255) as u8;
                        let cell = self.cell(r, x);
                        *cell = (*cell).max(m);
                    }
                    if r1 >= band_top && r0 < band_bottom {
                        lo = lo.min(r0.max(band_top));
                        hi = hi.max((r1 + 1).min(band_bottom));
                    }
                    continue;
                }
                lo = lo.min(r0);
                hi = hi.max(r1 + 1);
                if r0 == r1 {
                    let m = ((bot - top) >> 8).min(255) as u8;
                    let cell = self.cell(r0, x);
                    *cell = (*cell).max(m);
                } else {
                    let first = ((((r0 + 1) << 16) - top) >> 8).min(255) as u8;
                    let cell = self.cell(r0, x);
                    *cell = (*cell).max(first);
                    for r in (r0 + 1)..r1 {
                        *self.cell(r, x) = 255;
                    }
                    let last = ((bot - (r1 << 16)) >> 8).min(255) as u8;
                    let cell = self.cell(r1, x);
                    *cell = (*cell).max(last);
                }
            }
            self.grow(xs, lo, xe, hi);
        } else {
            let (x0, y0, x1, y1) = if y0 <= y1 {
                (x0, y0, x1, y1)
            } else {
                (x1, y1, x0, y0)
            };
            let slope = (x1 - x0) / (y1 - y0);
            let hv = (half * len / (y1 - y0) * 65536.0) as i32;
            let ys = (y0.floor() as i32).max(band_top);
            let ye = (y1.ceil() as i32).min(band_bottom);
            if ye <= ys {
                return;
            }
            let step = (slope * 65536.0) as i32;
            let mut xc = ((x0 + ((ys as f32 + 0.5) - y0) * slope) * 65536.0) as i32;
            let (mut lo, mut hi) = (w, 0);
            for y in ys..ye {
                let left = xc - hv;
                let right = xc + hv;
                xc += step;
                let c0 = left >> 16;
                let c1 = right >> 16;
                if c0 < 0 || c1 >= w {
                    for c in c0.max(0)..=c1.min(w - 1) {
                        let cov = (right.min((c + 1) << 16) - left.max(c << 16)).clamp(0, ONE);
                        let m = (cov >> 8).min(255) as u8;
                        let cell = self.cell(y, c);
                        *cell = (*cell).max(m);
                    }
                    if c1 >= 0 && c0 < w {
                        lo = lo.min(c0.max(0));
                        hi = hi.max((c1 + 1).min(w));
                    }
                    continue;
                }
                lo = lo.min(c0);
                hi = hi.max(c1 + 1);
                let base = (y - band_top) as usize * self.stride;
                if c0 == c1 {
                    let m = ((right - left) >> 8).min(255) as u8;
                    let cell = &mut self.buf[base + c0 as usize];
                    *cell = (*cell).max(m);
                } else {
                    let first = ((((c0 + 1) << 16) - left) >> 8).min(255) as u8;
                    let cell = &mut self.buf[base + c0 as usize];
                    *cell = (*cell).max(first);
                    self.buf[base + (c0 + 1) as usize..base + c1 as usize].fill(255);
                    let last = ((right - (c1 << 16)) >> 8).min(255) as u8;
                    let cell = &mut self.buf[base + c1 as usize];
                    *cell = (*cell).max(last);
                }
            }
            self.grow(lo, ys, hi, ye);
        }
    }

    #[inline]
    fn grow(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        if x1 > x0 && y1 > y0 {
            self.bounds[0] = self.bounds[0].min(x0);
            self.bounds[1] = self.bounds[1].min(y0);
            self.bounds[2] = self.bounds[2].max(x1);
            self.bounds[3] = self.bounds[3].max(y1);
        }
    }
}

/// Liang–Barsky clipping of a segment to a rectangle.
#[allow(clippy::too_many_arguments)]
fn clip(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) -> Option<(f32, f32, f32, f32)> {
    if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return None;
    }
    let (dx, dy) = (x1 - x0, y1 - y0);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-dx, x0 - left),
        (dx, right - x0),
        (-dy, y0 - top),
        (dy, bottom - y0),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return None;
                }
                t1 = t1.min(r);
            }
        }
    }
    Some((x0 + t0 * dx, y0 + t0 * dy, x0 + t1 * dx, y0 + t1 * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 64;
    const H: usize = 48;

    fn lum(px: &[u8], x: usize, y: usize) -> u8 {
        px[(y * W + x) * 4 + 1]
    }

    fn total(px: &[u8]) -> f64 {
        px.chunks_exact(4).map(|p| p[1] as f64 / 255.0).sum()
    }

    /// Runs `f` on a zeroed canvas and returns the pixels.
    fn paint(f: impl FnOnce(&mut Canvas<'_>)) -> Vec<u8> {
        let mut px = vec![0u8; W * H * 4];
        {
            let mut c = Canvas::new(&mut px, W, H, W * 4);
            f(&mut c);
        }
        px
    }

    #[test]
    fn clear_fills_opaque() {
        let px = paint(|c| c.clear([1.0, 0.5, 0.0]));
        assert_eq!(&px[..4], &[0, 128, 255, 255]);
        assert_eq!(&px[(W * H - 1) * 4..], &[0, 128, 255, 255]);
    }

    #[test]
    fn disc_area_matches_pi_r_squared() {
        for r in [1.0f32, 2.5, 6.0, 13.3] {
            let px = paint(|c| c.disc(32.0, 24.0, r, [0.0, 1.0, 0.0], 1.0));
            let area = total(&px);
            let want = std::f64::consts::PI * (r as f64).powi(2);
            assert!((area - want).abs() / want < 0.06, "r={r}: {area} vs {want}");
        }
    }

    #[test]
    fn disc_alpha_scales_coverage_and_edges_are_soft() {
        let px = paint(|c| c.disc(32.0, 24.0, 8.0, [0.0, 1.0, 0.0], 0.5));
        let centre = lum(&px, 32, 24);
        assert!((centre as i32 - 128).abs() <= 2);
        // some pixels lie strictly between empty and the centre value
        assert!(px.chunks_exact(4).any(|p| p[1] > 5 && p[1] < centre - 5));
    }

    #[test]
    fn discs_are_clipped_to_the_canvas() {
        let px = paint(|c| {
            c.disc(-3.0, -3.0, 10.0, [1.0; 3], 1.0);
            c.disc(W as f32 + 2.0, H as f32 / 2.0, 9.0, [1.0; 3], 1.0);
            c.disc(f32::NAN, 0.0, 5.0, [1.0; 3], 1.0);
            c.disc(10.0, 10.0, -1.0, [1.0; 3], 1.0);
        });
        assert!(total(&px) > 10.0);
    }

    #[test]
    fn ring_covers_the_outline_only() {
        let px = paint(|c| c.ring(32.0, 24.0, 10.0, 1.0, [0.0, 1.0, 0.0], 1.0));
        assert_eq!(lum(&px, 32, 24), 0);
        assert!(lum(&px, 42, 24) > 100 || lum(&px, 41, 24) > 100);
        let area = total(&px);
        let want = 2.0 * std::f64::consts::PI * 10.0;
        assert!((area - want).abs() / want < 0.15, "{area} vs {want}");
    }

    /// Renders lines through the mask onto a fresh canvas and returns its pixels.
    fn stroke(lines: &[(f32, f32, f32, f32, f32)], alpha: f32) -> Vec<u8> {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, W as i32, H as i32)
            .expect("surface");
        let cr = cairo::Context::new(&surface).expect("cr");
        cr.set_source_rgb(0.0, 0.0, 0.0);
        let _ = cr.paint();
        let mut mask = LineMask::default();
        assert!(mask.begin(W, H, 1.0));
        {
            let mut m = mask.lines().expect("pixels");
            for l in lines {
                m.line(l.0, l.1, l.2, l.3, l.4);
            }
        }
        mask.composite(&cr, [0.0, 1.0, 0.0], alpha);
        drop(cr);
        let mut surface = surface;
        let stride = surface.stride() as usize;
        let data = surface.data().expect("data");
        let mut out = vec![0u8; W * H * 4];
        for y in 0..H {
            out[y * W * 4..(y + 1) * W * 4].copy_from_slice(&data[y * stride..y * stride + W * 4]);
        }
        out
    }

    #[test]
    fn lines_have_the_requested_width() {
        for width in [1.0f32, 1.25, 2.0, 4.0] {
            for (a, b) in [
                ((5.0, 24.0), (59.0, 24.0)),
                ((32.0, 3.0), (32.0, 45.0)),
                ((6.0, 6.0), (58.0, 42.0)),
            ] {
                let px = stroke(&[(a.0, a.1, b.0, b.1, width)], 1.0);
                let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt() as f64;
                let area = total(&px);
                let want = len * width as f64;
                assert!(
                    (area - want).abs() / want < 0.1,
                    "{a:?}->{b:?} w{width}: {area} vs {want}"
                );
            }
        }
    }

    #[test]
    fn bands_in_parallel_equal_the_single_thread_result() {
        let mut state = 99u64;
        let mut rnd = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as f32) / (1u64 << 31) as f32
        };
        let segs: Vec<[f32; 4]> = (0..2200)
            .map(|_| {
                let (x, y) = (rnd() * 90.0 - 13.0, rnd() * 70.0 - 11.0);
                [x, y, x + (rnd() - 0.5) * 80.0, y + (rnd() - 0.5) * 60.0]
            })
            .collect();
        let render_some = |segs: &[[f32; 4]], threads: usize| {
            let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, W as i32, H as i32)
                .expect("surface");
            let cr = cairo::Context::new(&surface).expect("cr");
            let mut mask = LineMask::default();
            assert!(mask.begin(W, H, 1.0));
            {
                let mut m = mask.lines().expect("pixels");
                m.banded(segs, 1.25, threads);
            }
            mask.composite(&cr, [0.0, 1.0, 0.0], 1.0);
            drop(cr);
            let mut surface = surface;
            let stride = surface.stride() as usize;
            let data = surface.data().expect("data");
            (0..H)
                .flat_map(|y| data[y * stride..y * stride + W * 4].to_vec())
                .collect::<Vec<u8>>()
        };
        let render = |threads: usize| render_some(&segs, threads);
        for (k, seg) in segs.iter().enumerate().take(300) {
            let one = render_some(&[*seg], 1);
            let many = render_some(&[*seg], 3);
            let worst = one
                .iter()
                .zip(&many)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            let at: Vec<(usize, usize, u8, u8)> = one
                .iter()
                .zip(&many)
                .enumerate()
                .filter(|(i, (a, b))| i % 4 == 1 && a.abs_diff(**b) > 3)
                .map(|(i, (a, b))| ((i / 4) % W, (i / 4) / W, *a, *b))
                .collect();
            assert!(worst <= 3, "segment {k} {seg:?}: worst {worst} at {at:?}");
        }
        // clipping per band moves the start of the DDA by sub-pixel amounts, so
        // coverage may differ by a level or two, never more
        let single = render(1);
        for threads in [3, 4] {
            let banded = render(threads);
            let worst = single
                .iter()
                .zip(&banded)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            let off = single
                .iter()
                .zip(&banded)
                .filter(|(a, b)| a.abs_diff(**b) > 2)
                .count();
            let first: Vec<(usize, usize, u8, u8)> = single
                .iter()
                .zip(&banded)
                .enumerate()
                .filter(|(i, (a, b))| i % 4 == 1 && a.abs_diff(**b) > 20)
                .map(|(i, (a, b))| ((i / 4) % W, (i / 4) / W, *a, *b))
                .take(8)
                .collect();
            assert!(
                worst <= 6 && off == 0,
                "threads {threads}: worst {worst}, {off} pixels differ, first {first:?}"
            );
        }
    }

    #[test]
    fn lines_are_positioned_to_a_tenth_of_a_pixel() {
        for seg in [
            [53.4f32, 28.7, 25.3, 57.7],
            [4.2, 40.1, 60.3, 8.8],
            [10.0, 5.5, 50.0, 6.5],
            [31.3, 2.0, 33.9, 46.0],
        ] {
            let px = stroke(&[(seg[0], seg[1], seg[2], seg[3], 1.25)], 1.0);
            let steep = (seg[3] - seg[1]).abs() > (seg[2] - seg[0]).abs();
            let range = if steep {
                (
                    seg[1].min(seg[3]) as usize + 3,
                    seg[1].max(seg[3]).min(44.0) as usize - 3,
                )
            } else {
                (
                    seg[0].min(seg[2]) as usize + 3,
                    seg[0].max(seg[2]).min(60.0) as usize - 3,
                )
            };
            for k in range.0..range.1 {
                let (sum, moment) = if steep {
                    (0..W).fold((0.0f32, 0.0f32), |(s, m), x| {
                        let v = px[(k * W + x) * 4 + 1] as f32;
                        (s + v, m + v * (x as f32 + 0.5))
                    })
                } else {
                    (0..H).fold((0.0f32, 0.0f32), |(s, m), y| {
                        let v = px[(y * W + k) * 4 + 1] as f32;
                        (s + v, m + v * (y as f32 + 0.5))
                    })
                };
                let centre = moment / sum;
                let t = if steep {
                    (k as f32 + 0.5 - seg[1]) / (seg[3] - seg[1])
                } else {
                    (k as f32 + 0.5 - seg[0]) / (seg[2] - seg[0])
                };
                let want = if steep {
                    seg[0] + t * (seg[2] - seg[0])
                } else {
                    seg[1] + t * (seg[3] - seg[1])
                };
                assert!(
                    (centre - want).abs() < 0.1,
                    "{seg:?} at {k}: {centre} vs {want}"
                );
            }
        }
    }

    #[test]
    fn overlapping_lines_paint_the_union_once() {
        let five = stroke(&[(4.0, 24.0, 60.0, 24.5, 2.0); 5], 0.5);
        let one = stroke(&[(4.0, 24.0, 60.0, 24.5, 2.0)], 0.5);
        assert_eq!(five, one);
    }

    #[test]
    fn the_mask_is_empty_after_compositing() {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, W as i32, H as i32)
            .expect("surface");
        let cr = cairo::Context::new(&surface).expect("cr");
        let mut mask = LineMask::default();
        assert!(mask.begin(W, H, 1.0));
        {
            let mut m = mask.lines().expect("pixels");
            m.line(0.0, 0.0, 63.0, 47.0, 3.0);
        }
        mask.composite(&cr, [1.0; 3], 1.0);
        let mut again = mask.surface.take().expect("surface");
        let stride = again.stride() as usize;
        let data = again.data().expect("data");
        assert!((0..H).all(|y| data[y * stride..y * stride + W].iter().all(|v| *v == 0)));
    }

    #[test]
    fn far_away_endpoints_are_clipped_cheaply_and_correctly() {
        let px = stroke(
            &[
                (-1.0e7, 24.0, 1.0e7, 24.0, 2.0),
                (-1.0e7, -1.0e7, -1.0e6, -1.0e6, 2.0),
                (f32::NAN, 0.0, 10.0, 10.0, 2.0),
                (10.0, 10.0, 10.0, 10.0, 2.0),
            ],
            1.0,
        );
        let area = total(&px);
        assert!((area - 2.0 * W as f64).abs() < 6.0, "{area}");
    }

    #[test]
    fn dart_is_solid_inside_and_clean_at_the_seam() {
        let px = paint(|c| {
            c.dart(
                (50.0, 24.0),
                (38.0, 16.0),
                (41.0, 24.0),
                (38.0, 32.0),
                [0.0, 1.0, 0.0],
                1.0,
            )
        });
        // along the shared edge (tip -> notch) there is no gap
        for x in 42..49 {
            assert!(lum(&px, x, 24) > 220 || lum(&px, x, 23) > 220, "gap at {x}");
        }
        assert_eq!(lum(&px, 20, 24), 0);
        // two triangles of 36 square pixels each
        let area = total(&px);
        assert!((area - 72.0).abs() < 8.0, "{area}");
    }
}
