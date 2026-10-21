//! Overlap removal between equally sized circles using a hashed uniform grid.

pub struct CollideGrid {
    head: Vec<i32>,
    next: Vec<i32>,
    cell_x: Vec<i32>,
    cell_y: Vec<i32>,
    px: Vec<f32>,
    py: Vec<f32>,
}

impl Default for CollideGrid {
    fn default() -> Self {
        CollideGrid {
            head: Vec::new(),
            next: Vec::new(),
            cell_x: Vec::new(),
            cell_y: Vec::new(),
            px: Vec::new(),
            py: Vec::new(),
        }
    }
}

fn hash(cx: i32, cy: i32, mask: usize) -> usize {
    ((cx as u32).wrapping_mul(73_856_093) ^ (cy as u32).wrapping_mul(19_349_663)) as usize & mask
}

impl CollideGrid {
    pub fn reserve(&mut self, n: usize) {
        let table = (n * 2).max(16).next_power_of_two();
        self.head.resize(table, -1);
        self.next.resize(n, -1);
        self.cell_x.resize(n, 0);
        self.cell_y.resize(n, 0);
        self.px.resize(n, 0.0);
        self.py.resize(n, 0.0);
    }

    /// Pushes overlapping pairs apart by adding to the velocities. Positions
    /// are the ones the nodes will have after this tick's velocity.
    pub fn apply(
        &mut self,
        radius: f32,
        strength: f32,
        x: &[f32],
        y: &[f32],
        vx: &mut [f32],
        vy: &mut [f32],
    ) {
        let n = x.len();
        if n < 2 {
            return;
        }
        self.reserve(n);
        let diameter = 2.0 * radius;
        let d2_limit = diameter * diameter;
        let mask = self.head.len() - 1;
        self.head.iter_mut().for_each(|h| *h = -1);
        for i in 0..n {
            self.px[i] = x[i] + vx[i];
            self.py[i] = y[i] + vy[i];
            let cx = (self.px[i] / diameter).floor() as i32;
            let cy = (self.py[i] / diameter).floor() as i32;
            self.cell_x[i] = cx;
            self.cell_y[i] = cy;
            let h = hash(cx, cy, mask);
            self.next[i] = self.head[h];
            self.head[h] = i as i32;
        }
        for i in 0..n {
            let (xi, yi) = (self.px[i], self.py[i]);
            let (ci, cj) = (self.cell_x[i], self.cell_y[i]);
            for dx_cell in -1..=1 {
                for dy_cell in -1..=1 {
                    let (cx, cy) = (ci.wrapping_add(dx_cell), cj.wrapping_add(dy_cell));
                    let mut j = self.head[hash(cx, cy, mask)];
                    while j >= 0 {
                        let ju = j as usize;
                        j = self.next[ju];
                        if ju <= i || self.cell_x[ju] != cx || self.cell_y[ju] != cy {
                            continue;
                        }
                        let mut dx = xi - self.px[ju];
                        let mut dy = yi - self.py[ju];
                        let mut l = dx * dx + dy * dy;
                        if l >= d2_limit {
                            continue;
                        }
                        if dx == 0.0 {
                            dx = if i < ju { -0.01 } else { 0.01 };
                            l += dx * dx;
                        }
                        if dy == 0.0 {
                            dy = if i < ju { -0.01 } else { 0.01 };
                            l += dy * dy;
                        }
                        let l = l.sqrt();
                        let push = (diameter - l) / l * strength * 0.5;
                        let (mx, my) = (dx * push, dy * push);
                        vx[i] += mx;
                        vy[i] += my;
                        vx[ju] -= mx;
                        vy[ju] -= my;
                    }
                }
            }
        }
    }
}
