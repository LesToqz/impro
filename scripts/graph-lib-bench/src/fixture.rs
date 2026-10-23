//! Deterministic synthetic graphs shaped like what the app's layout settles
//! into: a big phyllotaxis disc of isolated notes, small linked clusters in
//! a ring around it and one hub with fifty spokes.

/// Background of the dark theme.
pub const BG: [f32; 3] = [0x17 as f32 / 255.0, 0x21 as f32 / 255.0, 0x2b as f32 / 255.0];
/// Node fills: grey, blue, green.
pub const NODE_COLORS: [[f32; 3]; 3] = [
    [0.54, 0.56, 0.60],
    [0.36, 0.62, 0.84],
    [0.42, 0.75, 0.42],
];
/// Edge colour: semi-transparent grey, one device pixel wide.
pub const EDGE_COLOR: [f32; 4] = [0.62, 0.66, 0.72, 0.45];
/// Node count cap of the app.
pub const NODES: usize = 3000;

pub struct Fixture {
    pub name: &'static str,
    pub x: Vec<f32>,
    pub y: Vec<f32>,
    /// World radius (`1.15 · min(24, 6 + 2·log2(1 + degree))`, as the app).
    pub radius: Vec<f32>,
    /// Index into [`NODE_COLORS`].
    pub class: Vec<u8>,
    pub edges: Vec<(u32, u32)>,
    pub names: Vec<String>,
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const GOLDEN: f32 = 2.399_963_2;

fn marker_radius(degree: f32) -> f32 {
    1.15 * (6.0 + 2.0 * (1.0 + degree.max(0.0)).log2()).min(24.0)
}

struct Builder {
    x: Vec<f32>,
    y: Vec<f32>,
    class: Vec<u8>,
    edges: Vec<(u32, u32)>,
}

impl Builder {
    fn node(&mut self, x: f32, y: f32, class: u8) -> u32 {
        self.x.push(x);
        self.y.push(y);
        self.class.push(class);
        (self.x.len() - 1) as u32
    }
    fn finish(self, name: &'static str) -> Fixture {
        let n = self.x.len();
        let mut degree = vec![0f32; n];
        for &(a, b) in &self.edges {
            degree[a as usize] += 1.0;
            degree[b as usize] += 1.0;
        }
        Fixture {
            name,
            radius: degree.iter().map(|d| marker_radius(*d)).collect(),
            names: (0..n).map(|i| format!("n{i:06}.md")).collect(),
            x: self.x,
            y: self.y,
            class: self.class,
            edges: self.edges,
        }
    }
}

/// 3000 nodes: 2600 isolated, 80 small clusters (stars, rings, chains),
/// one 50-spoke hub. About 350 edges. `spread` scales every coordinate: the
/// app's own layout settles isolated notes about 60 world units apart
/// (`spread` 2.8); `spread` 1 packs them 21 apart, a harder case.
pub fn app_like(spread: f32) -> Fixture {
    let mut b = Builder {
        x: Vec::new(),
        y: Vec::new(),
        class: Vec::new(),
        edges: Vec::new(),
    };
    let mut rng = Rng::new(11);
    let isolated = 2600;
    for i in 0..isolated {
        let r = 12.0 * (i as f32 + 0.5).sqrt();
        let a = i as f32 * GOLDEN;
        let class = match i % 7 {
            0..=3 => 0,
            4 | 5 => 1,
            _ => 2,
        };
        b.node(r * a.cos(), r * a.sin(), class);
    }
    // hub with fifty spokes, off to the left
    let (hx, hy) = (-2000.0, 0.0);
    let hub = b.node(hx, hy, 2);
    for k in 0..50 {
        let a = k as f32 / 50.0 * std::f32::consts::TAU;
        let s = b.node(hx + 170.0 * a.cos(), hy + 170.0 * a.sin(), 1);
        b.edges.push((hub, s));
    }
    // clusters of 3..=10 nodes, 349 in all
    let mut left = NODES - isolated - 51;
    for k in 0..80usize {
        let remaining = 80 - k;
        let size = if remaining == 1 {
            left
        } else {
            let want = 3 + rng.below(4) as usize;
            want.min(left - 3 * (remaining - 1)).max(3)
        };
        left -= size;
        let ring_r = 800.0 + 900.0 * (k as f32 / 80.0).sqrt();
        let ang = k as f32 * GOLDEN;
        let (cx, cy) = (ring_r * ang.cos(), ring_r * ang.sin());
        let kind = k % 3;
        let ids: Vec<u32> = (0..size)
            .map(|j| {
                let (dx, dy) = match kind {
                    0 if j == 0 => (0.0, 0.0),
                    0 => {
                        let t = (j - 1) as f32 / (size - 1) as f32 * std::f32::consts::TAU;
                        (42.0 * t.cos(), 42.0 * t.sin())
                    }
                    1 => {
                        let t = j as f32 / size as f32 * std::f32::consts::TAU;
                        (36.0 * t.cos(), 36.0 * t.sin())
                    }
                    _ => (j as f32 * 34.0 - size as f32 * 17.0, 14.0 * (j as f32).sin()),
                };
                b.node(cx + dx, cy + dy, 1)
            })
            .collect();
        match kind {
            0 => ids[1..].iter().for_each(|&s| b.edges.push((ids[0], s))),
            1 => (0..size).for_each(|j| b.edges.push((ids[j], ids[(j + 1) % size]))),
            _ => (1..size).for_each(|j| b.edges.push((ids[j - 1], ids[j]))),
        }
    }
    debug_assert_eq!(b.x.len(), NODES);
    b.x.iter_mut().chain(b.y.iter_mut()).for_each(|v| *v *= spread);
    b.finish(if spread == 1.0 { "packed" } else { "app" })
}

/// Stress variant: 3000 nodes on one phyllotaxis disc, ~8000 edges between
/// near neighbours plus ~600 long cross links.
pub fn dense() -> Fixture {
    let mut b = Builder {
        x: Vec::new(),
        y: Vec::new(),
        class: Vec::new(),
        edges: Vec::new(),
    };
    let mut rng = Rng::new(5);
    for i in 0..NODES {
        let r = 15.0 * (i as f32 + 0.5).sqrt();
        let a = i as f32 * GOLDEN;
        b.node(r * a.cos(), r * a.sin(), (i % 3) as u8);
    }
    for i in 0..NODES as u32 {
        for off in [13u32, 21, 34] {
            if rng.unit() < 0.9 && i + off < NODES as u32 {
                b.edges.push((i, i + off));
            }
        }
    }
    for _ in 0..600 {
        let a = rng.below(NODES as u64) as u32;
        let c = rng.below(NODES as u64) as u32;
        if a != c {
            b.edges.push((a, c));
        }
    }
    b.finish("dense")
}

pub fn by_name(name: &str) -> Fixture {
    match name {
        "dense" => dense(),
        "packed" => app_like(1.0),
        _ => app_like(2.8),
    }
}
