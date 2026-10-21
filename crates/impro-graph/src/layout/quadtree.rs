//! Barnes–Hut quadtree over point masses of one, rebuilt every tick into
//! buffers that are reused (no allocation once warmed up).

/// A square region with the aggregate of the points inside.
#[derive(Clone, Copy, Default)]
struct Cell {
    /// Centroid of the points.
    cx: f32,
    cy: f32,
    /// Number of points.
    mass: f32,
    /// Squared side length.
    size2: f32,
    /// Square origin and side (for partitioning).
    x0: f32,
    y0: f32,
    side: f32,
    /// Range of `order` covered by this cell.
    start: u32,
    end: u32,
    /// First child; children of one cell are consecutive. 0 = leaf.
    first_child: u32,
    child_count: u8,
    depth: u8,
}

const LEAF_CAPACITY: u32 = 8;
const MAX_DEPTH: u8 = 28;

#[derive(Default)]
pub struct QuadTree {
    cells: Vec<Cell>,
    /// Point indices grouped by cell.
    order: Vec<u32>,
    scratch: Vec<u32>,
    pending: Vec<u32>,
}

impl QuadTree {
    /// Pre-sizes the buffers for `n` points so that rebuilding the tree every
    /// tick does not allocate (a tree has far fewer than `4n + 64` cells).
    pub fn reserve(&mut self, n: usize) {
        self.cells
            .reserve((4 * n + 64).saturating_sub(self.cells.len()));
        self.order.reserve(n.saturating_sub(self.order.len()));
        self.scratch.reserve(n.saturating_sub(self.scratch.len()));
        self.pending.reserve(512);
    }

    /// Point indices in tree order: iterating them keeps consecutive queries
    /// spatially close, which is much friendlier to the cache.
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    pub fn build(&mut self, xs: &[f32], ys: &[f32]) {
        let n = xs.len().min(ys.len());
        self.cells.clear();
        self.order.clear();
        self.order.extend(0..n as u32);
        self.scratch.clear();
        self.scratch.resize(n, 0);
        if n == 0 {
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for i in 0..n {
            x0 = x0.min(xs[i]);
            x1 = x1.max(xs[i]);
            y0 = y0.min(ys[i]);
            y1 = y1.max(ys[i]);
        }
        let side = (x1 - x0).max(y1 - y0).max(1.0) * 1.0001 + 1.0;
        self.cells.push(Cell {
            side,
            size2: side * side,
            x0,
            y0,
            start: 0,
            end: n as u32,
            ..Cell::default()
        });
        self.pending.clear();
        self.pending.push(0);
        while let Some(ci) = self.pending.pop() {
            self.fill(ci as usize, xs, ys);
        }
    }

    /// Computes the aggregate of cell `ci` and splits it if needed.
    fn fill(&mut self, ci: usize, xs: &[f32], ys: &[f32]) {
        let c = self.cells[ci];
        let (start, end) = (c.start as usize, c.end as usize);
        let (mut sx, mut sy) = (0.0f64, 0.0f64);
        for &p in &self.order[start..end] {
            sx += xs[p as usize] as f64;
            sy += ys[p as usize] as f64;
        }
        let count = (end - start) as f32;
        {
            let cell = &mut self.cells[ci];
            cell.mass = count;
            cell.cx = (sx / count as f64) as f32;
            cell.cy = (sy / count as f64) as f32;
        }
        if (end - start) as u32 <= LEAF_CAPACITY || c.depth >= MAX_DEPTH || c.side < 1e-3 {
            return;
        }
        let half = c.side * 0.5;
        let (mx, my) = (c.x0 + half, c.y0 + half);
        let quadrant = |p: u32| -> usize {
            (xs[p as usize] >= mx) as usize | (((ys[p as usize] >= my) as usize) << 1)
        };
        let mut counts = [0u32; 4];
        for &p in &self.order[start..end] {
            counts[quadrant(p)] += 1;
        }
        // all points in one quadrant and the cell cannot shrink the bounding
        // box any more: stay a leaf holding coincident points
        let mut offsets = [0u32; 4];
        let mut acc = start as u32;
        for q in 0..4 {
            offsets[q] = acc;
            acc += counts[q];
        }
        let mut cursor = offsets;
        for k in start..end {
            let p = self.order[k];
            let q = quadrant(p);
            self.scratch[cursor[q] as usize] = p;
            cursor[q] += 1;
        }
        self.order[start..end].copy_from_slice(&self.scratch[start..end]);
        let first = self.cells.len() as u32;
        let mut made = 0u8;
        for q in 0..4 {
            if counts[q] == 0 {
                continue;
            }
            let cx0 = c.x0 + if q & 1 == 1 { half } else { 0.0 };
            let cy0 = c.y0 + if q & 2 == 2 { half } else { 0.0 };
            self.cells.push(Cell {
                side: half,
                size2: half * half,
                x0: cx0,
                y0: cy0,
                start: offsets[q],
                end: offsets[q] + counts[q],
                depth: c.depth + 1,
                ..Cell::default()
            });
            self.pending.push(first + made as u32);
            made += 1;
        }
        let cell = &mut self.cells[ci];
        cell.first_child = first;
        cell.child_count = made;
    }

    /// Sum of the pairwise `strength / distance²`-style accelerations on the
    /// point at `(xi, yi)` (index `i`), approximating cells that are far enough
    /// (`size² / θ² < distance²`). `min_d2` softens very close pairs.
    /// Requires `theta < 0.7` so a cell containing the point is always opened.
    #[allow(clippy::too_many_arguments)]
    pub fn acceleration(
        &self,
        i: u32,
        xi: f32,
        yi: f32,
        xs: &[f32],
        ys: &[f32],
        strength: f32,
        inv_theta2: f32,
        min_d2: f32,
    ) -> (f32, f32) {
        let (mut ax, mut ay) = (0.0f32, 0.0f32);
        if self.cells.is_empty() {
            return (ax, ay);
        }
        let mut stack = [0u32; 128];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let c = &self.cells[stack[sp] as usize];
            let dx = c.cx - xi;
            let dy = c.cy - yi;
            let d2 = dx * dx + dy * dy;
            if c.size2 * inv_theta2 < d2 {
                let l = if d2 < min_d2 {
                    (min_d2 * d2).sqrt()
                } else {
                    d2
                };
                let f = c.mass * strength / l;
                ax += dx * f;
                ay += dy * f;
            } else if c.child_count > 0 {
                let first = c.first_child;
                for k in 0..c.child_count as u32 {
                    stack[sp] = first + k;
                    sp += 1;
                }
            } else {
                for &j in &self.order[c.start as usize..c.end as usize] {
                    if j == i {
                        continue;
                    }
                    let mut dx = xs[j as usize] - xi;
                    let mut dy = ys[j as usize] - yi;
                    if dx == 0.0 {
                        dx = if i < j { 0.5 } else { -0.5 };
                    }
                    if dy == 0.0 {
                        dy = if i < j { 0.5 } else { -0.5 };
                    }
                    let d2 = dx * dx + dy * dy;
                    let l = if d2 < min_d2 {
                        (min_d2 * d2).sqrt()
                    } else {
                        d2
                    };
                    let f = strength / l;
                    ax += dx * f;
                    ay += dy * f;
                }
            }
        }
        (ax, ay)
    }
}

/// Exact O(n²) reference used by tests and diagnostics.
pub fn brute_acceleration(xs: &[f32], ys: &[f32], strength: f32, min_d2: f32) -> Vec<(f32, f32)> {
    let n = xs.len();
    (0..n)
        .map(|i| {
            let (mut ax, mut ay) = (0.0f32, 0.0f32);
            for j in 0..n {
                if i == j {
                    continue;
                }
                let mut dx = xs[j] - xs[i];
                let mut dy = ys[j] - ys[i];
                if dx == 0.0 {
                    dx = if i < j { 0.5 } else { -0.5 };
                }
                if dy == 0.0 {
                    dy = if i < j { 0.5 } else { -0.5 };
                }
                let d2 = dx * dx + dy * dy;
                let l = if d2 < min_d2 {
                    (min_d2 * d2).sqrt()
                } else {
                    d2
                };
                let f = strength / l;
                ax += dx * f;
                ay += dy * f;
            }
            (ax, ay)
        })
        .collect()
}
