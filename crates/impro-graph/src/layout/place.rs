//! Initial placement of new nodes.

use crate::model::Adjacency;

/// Tiny deterministic generator (splitmix64).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Area per new node (72 × 72 square units).
pub const AREA_PER_NODE: f32 = 72.0 * 72.0;
/// Largest offset on either axis from the centre of surviving neighbours.
pub const NEIGHBOUR_JITTER: f32 = 24.0;

/// Fills the `None` entries of `pos`. New nodes land in a disc (or an annulus
/// outside the existing layout) with `AREA_PER_NODE` per node; a new node with
/// already placed neighbours appears beside their centre instead.
pub fn place_new(adj: &Adjacency, pos: &mut [Option<(f32, f32)>], rng: &mut Rng) {
    let n = pos.len();
    let new_count = pos.iter().filter(|p| p.is_none()).count();
    if new_count == 0 {
        return;
    }
    let existing_radius = pos
        .iter()
        .flatten()
        .map(|(x, y)| x.hypot(*y))
        .fold(0.0f32, f32::max);
    let area = AREA_PER_NODE * new_count as f32;
    let outer = (area / std::f32::consts::PI + existing_radius * existing_radius).sqrt();
    let disc = |rng: &mut Rng| {
        let angle = std::f32::consts::TAU * rng.unit();
        let radius = existing_radius + rng.unit().sqrt() * (outer - existing_radius);
        (radius * angle.cos(), radius * angle.sin())
    };
    if new_count == n {
        for p in pos.iter_mut() {
            *p = Some(disc(rng));
        }
        return;
    }
    // new nodes next to placed neighbours first, repeating so chains of new
    // nodes follow each other outward
    let mut progress = true;
    while progress {
        progress = false;
        for i in 0..n {
            if pos[i].is_some() {
                continue;
            }
            let (mut sx, mut sy, mut k) = (0.0f32, 0.0f32, 0u32);
            for &j in adj.outgoing(i).iter().chain(adj.incoming(i)) {
                if let Some((x, y)) = pos[j as usize] {
                    sx += x;
                    sy += y;
                    k += 1;
                }
            }
            if k > 0 {
                let jx = (rng.unit() - 0.5) * 2.0 * NEIGHBOUR_JITTER;
                let jy = (rng.unit() - 0.5) * 2.0 * NEIGHBOUR_JITTER;
                pos[i] = Some((sx / k as f32 + jx, sy / k as f32 + jy));
                progress = true;
            }
        }
    }
    for p in pos.iter_mut() {
        if p.is_none() {
            *p = Some(disc(rng));
        }
    }
}
