//! Camera paths, label selection, timing and table output shared by every
//! candidate binary.

use std::time::Instant;

use crate::fixture::Fixture;

/// World → screen: `screen = world · zoom + pan` in logical pixels.
#[derive(Clone, Copy, Debug)]
pub struct Cam {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Cam {
    pub fn to_screen(&self, wx: f32, wy: f32) -> (f32, f32) {
        (
            (wx as f64 * self.zoom + self.x) as f32,
            (wy as f64 * self.zoom + self.y) as f32,
        )
    }
    /// Marker radius shrinks with the square root of the zoom, as the app.
    pub fn screen_radius(&self, r: f32) -> f32 {
        (r as f64 * self.zoom.sqrt()) as f32
    }
}

pub const FRAMES: usize = 60;

/// Frame `k` of a constant-velocity pan at zoom 1 (25 logical px per frame).
pub fn pan_cam(k: usize, w: f64, h: f64) -> Cam {
    Cam {
        x: w / 2.0 - 25.0 * k as f64,
        y: h / 2.0,
        zoom: 1.0,
    }
}

/// Frame `k` of a zoom-out from 2 to 0.04 about the screen centre.
pub fn zoom_cam(k: usize, w: f64, h: f64) -> Cam {
    let t = k as f64 / (FRAMES - 1) as f64;
    let zoom = 2.0 * (0.04f64 / 2.0).powf(t);
    Cam {
        x: w / 2.0,
        y: h / 2.0,
        zoom,
    }
}

/// One label to draw: node index, anchor (below the marker, centred) and font px.
#[derive(Clone, Copy, Debug)]
pub struct LabelDraw {
    pub node: u32,
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

pub const LABEL_CAP: usize = 300;
/// Labels fade out below this zoom (the app hides them near 0.12-0.3).
pub const LABEL_MIN_ZOOM: f64 = 0.25;

/// Up to 300 on-screen labels at 11-14 px, evenly strided through the
/// visible nodes (the app picks by importance; the cost is the same).
pub fn pick_labels(fx: &Fixture, cam: &Cam, w: f64, h: f64, out: &mut Vec<LabelDraw>) {
    out.clear();
    if cam.zoom < LABEL_MIN_ZOOM {
        return;
    }
    let visible: Vec<u32> = (0..fx.x.len() as u32)
        .filter(|&i| {
            let (sx, sy) = cam.to_screen(fx.x[i as usize], fx.y[i as usize]);
            sx >= 0.0 && sy >= 0.0 && (sx as f64) < w && (sy as f64) < h
        })
        .collect();
    let step = (visible.len() / LABEL_CAP).max(1);
    for &i in visible.iter().step_by(step).take(LABEL_CAP) {
        let (sx, sy) = cam.to_screen(fx.x[i as usize], fx.y[i as usize]);
        let r = cam.screen_radius(fx.radius[i as usize]);
        out.push(LabelDraw {
            node: i,
            x: sx,
            y: sy + r + 2.0,
            size: 11.0 + (i % 4) as f32,
        });
    }
}

/// A candidate renderer bound to one surface size.
pub trait Renderer {
    /// Draws one frame completely (all work finished, pixels in place).
    fn frame(&mut self, fx: &Fixture, cam: &Cam, labels: &[LabelDraw]);
    /// Extra text for the table (e.g. counters of the last frame).
    fn note(&self) -> String {
        String::new()
    }
}

pub struct Opts {
    pub res: Vec<(u32, u32, u32)>,
    pub labels: bool,
    pub fixture: String,
    pub frames: usize,
    pub rest: Vec<String>,
}

impl Opts {
    /// `--res WxH@S[,…]`, `--labels`, `--fixture app|dense`, `--frames N`;
    /// other arguments are passed through in `rest`.
    pub fn parse() -> Opts {
        let mut o = Opts {
            res: Vec::new(),
            labels: false,
            fixture: "app".into(),
            frames: FRAMES,
            rest: Vec::new(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--labels" => o.labels = true,
                "--fixture" => o.fixture = args.next().unwrap_or_default(),
                "--frames" => o.frames = args.next().and_then(|v| v.parse().ok()).unwrap_or(FRAMES),
                "--res" => {
                    for item in args.next().unwrap_or_default().split(',') {
                        let (dim, s) = item.split_once('@').unwrap_or((item, "1"));
                        if let Some((w, h)) = dim.split_once('x') {
                            if let (Ok(w), Ok(h), Ok(s)) = (w.parse(), h.parse(), s.parse()) {
                                o.res.push((w, h, s));
                            }
                        }
                    }
                }
                _ => o.rest.push(a),
            }
        }
        if o.res.is_empty() {
            for s in [1, 2] {
                for (w, h) in [(1920, 1080), (2560, 1440), (3840, 2160)] {
                    o.res.push((w, h, s));
                }
            }
        }
        o
    }
    pub fn has(&self, flag: &str) -> bool {
        self.rest.iter().any(|a| a == flag)
    }
    pub fn value(&self, flag: &str) -> Option<&str> {
        let i = self.rest.iter().position(|a| a == flag)?;
        self.rest.get(i + 1).map(|s| s.as_str())
    }
}

pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

/// Resident set size and its high-water mark in MB.
pub fn rss_mb() -> (f64, f64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kb = |key: &str| {
        s.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
            / 1024.0
    };
    (kb("VmRSS:"), kb("VmHWM:"))
}

pub fn header(candidate: &str, o: &Opts) {
    println!(
        "\n### {candidate} — fixture `{}`, labels {}, {} frames per scenario",
        o.fixture,
        if o.labels { "300" } else { "off" },
        o.frames
    );
    println!("| res (logical@scale → device) | scenario | setup ms | cold frame ms | p50 | p95 | p99 | max | RSS / peak MB | note |");
    println!("|---|---|---:|---:|---:|---:|---:|---:|---|---|");
}

/// Runs the cold frame, a constant-velocity pan and a zoom-out at each
/// requested size and prints one table row per scenario.
pub fn sweep<F>(fx: &Fixture, o: &Opts, mut make: F)
where
    F: FnMut(u32, u32, u32) -> Option<Box<dyn Renderer>>,
{
    let mut labels = Vec::new();
    for &(w, h, s) in &o.res {
        let res = format!("{w}x{h}@{s} → {}x{}", w * s, h * s);
        let t = Instant::now();
        let Some(mut r) = make(w, h, s) else {
            println!("| {res} | – | skipped: renderer unavailable | | | | | | | |");
            continue;
        };
        let setup = t.elapsed().as_secs_f64() * 1e3;
        let (wf, hf) = (w as f64, h as f64);
        let first = pan_cam(0, wf, hf);
        if o.labels {
            pick_labels(fx, &first, wf, hf, &mut labels);
        }
        let t = Instant::now();
        r.frame(fx, &first, &labels);
        let cold = t.elapsed().as_secs_f64() * 1e3;
        for (name, path) in [
            ("pan", pan_cam as fn(usize, f64, f64) -> Cam),
            ("zoom-out", zoom_cam as fn(usize, f64, f64) -> Cam),
        ] {
            let mut ms = Vec::with_capacity(o.frames);
            let mut label_n = 0;
            for k in 0..o.frames {
                let cam = path(k % FRAMES, wf, hf);
                if o.labels {
                    pick_labels(fx, &cam, wf, hf, &mut labels);
                    label_n = label_n.max(labels.len());
                }
                let t = Instant::now();
                r.frame(fx, &cam, &labels);
                ms.push(t.elapsed().as_secs_f64() * 1e3);
            }
            ms.sort_by(|a, b| a.total_cmp(b));
            let (rss, peak) = rss_mb();
            println!(
                "| {res} | {name} | {setup:.1} | {cold:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {rss:.0} / {peak:.0} | {} |",
                percentile(&ms, 0.5),
                percentile(&ms, 0.95),
                percentile(&ms, 0.99),
                ms[ms.len() - 1],
                {
                    let n = r.note();
                    if o.labels { format!("{n} labels≤{label_n}") } else { n }
                }
            );
        }
        drop(r);
    }
}
