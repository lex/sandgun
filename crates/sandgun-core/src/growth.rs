use crate::cell::{Material, FLAG_BURNING, FLAG_FRUITED};
use crate::params::*;
use crate::world::World;

/// A cell on the growing edge of the mycelium network.
#[derive(Clone, Copy)]
pub struct FrontierCell {
    pub x: usize,
    pub y: usize,
    /// How many empty cells this growth is from solid soil/mycelium mass.
    /// 0 = grew from/into soil; increments when bridging empty. Lives here, never in `aux`.
    pub reach: u8,
}

/// A mushroom being revealed cell-by-cell. Shape fields set at fruiting (Task 4).
#[derive(Clone, Copy)]
pub struct GrowingMushroom {
    pub x: usize,
    pub base_y: usize,
    pub height: u8,
    pub cap_r: u8,
    pub progress: u16,
    /// Per-mushroom seed (drawn from the sim RNG at fruiting time) driving the stem's gentle
    /// horizontal wander -- see `stem_dx`. Keeps each mushroom's sway deterministic but distinct.
    pub sway_seed: u32,
}

impl World {
    /// Scan once (at load / after worldgen) and enqueue mycelium cells that border
    /// colonizable space. O(width*height) but runs only on demand, not per frame.
    pub fn seed_frontier(&mut self) {
        self.frontier.clear();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) == Material::Mycelium && self.has_colonizable_neighbor(x, y) {
                    self.push_frontier(FrontierCell { x, y, reach: 0 });
                }
            }
        }
    }

    /// Seed the frontier for a bounded region around an active event (e.g. an ammo impact),
    /// so mycelium painted outside the normal colonize/reseed paths (which push_frontier
    /// themselves) still joins the growth frontier and isn't permanently inert. Scans only
    /// the box [cx-radius-1 .. cx+radius+1] x [cy-radius-1 .. cy+radius+1] (clamped to world
    /// bounds), so it's O(radius^2) and safe to call at impact time -- it does not run per
    /// frame and so cannot threaten chunk-sleep.
    pub(crate) fn seed_frontier_around(&mut self, cx: isize, cy: isize, radius: isize) {
        let pad = radius + 1;
        let x0 = (cx - pad).max(0) as usize;
        let x1 = ((cx + pad).max(0) as usize).min(self.width.saturating_sub(1));
        let y0 = (cy - pad).max(0) as usize;
        let y1 = ((cy + pad).max(0) as usize).min(self.height.saturating_sub(1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                if self.get(x, y) == Material::Mycelium && self.has_colonizable_neighbor(x, y) {
                    self.push_frontier(FrontierCell { x, y, reach: 0 });
                }
            }
        }
    }

    fn has_colonizable_neighbor(&self, x: usize, y: usize) -> bool {
        for (nx, ny) in self.ortho(x, y) {
            if self.material_at(nx, ny) == Material::Soil {
                return true;
            }
        }
        false
    }

    /// The 4 orthogonal neighbors as isize pairs (may be out of bounds).
    fn ortho(&self, x: usize, y: usize) -> [(isize, isize); 4] {
        let (xi, yi) = (x as isize, y as isize);
        [(xi + 1, yi), (xi - 1, yi), (xi, yi + 1), (xi, yi - 1)]
    }

    pub fn frontier_len(&self) -> usize {
        self.frontier.len()
    }
    pub fn mushroom_len(&self) -> usize {
        self.mushrooms.len()
    }
    /// Cells dropped because the frontier was already at P_MAX_FRONTIER (see `push_frontier`).
    pub fn frontier_drops(&self) -> u64 {
        self.frontier_drops
    }

    /// Enqueue a frontier cell, enforcing the P_MAX_FRONTIER hard cap. When already at the
    /// cap, the new cell is dropped (not enqueued) and counted in `frontier_drops` rather than
    /// truncating the frontier — a blind truncate would silently discard whichever cells
    /// happen to sit at the end of the vec (including cells mid-way through aging toward
    /// fruiting), and could drop cells that had already been swap-removed into new slots.
    /// Not enqueueing only ever keeps the frontier the same size or smaller, so this preserves
    /// the drain-to-empty/chunk-sleep termination guarantee.
    fn push_frontier(&mut self, fc: FrontierCell) {
        let max_frontier = self.params.values[P_MAX_FRONTIER] as usize;
        if self.frontier.len() >= max_frontier {
            self.frontier_drops += 1;
            return;
        }
        self.frontier.push(fc);
    }

    /// Budgeted growth tick. Called from step() on the P_GROWTH_INTERVAL cadence.
    /// Returns immediately when there is nothing alive to grow (chunk-sleep safe).
    pub fn grow(&mut self) {
        if self.frontier.is_empty() && self.mushrooms.is_empty() && self.caps.is_empty() {
            return;
        }
        let budget = self.params.values[P_GROWTH_BUDGET] as usize;

        // Process a budgeted slice of the frontier. Swap-remove retirees; append new growth.
        let mut processed = 0;
        let mut i = 0;
        while i < self.frontier.len() && processed < budget {
            processed += 1;
            let fc = self.frontier[i];
            // Retire if the source cell is no longer living mycelium (shot, burned, etc).
            if self.get(fc.x, fc.y) != Material::Mycelium {
                self.frontier.swap_remove(i);
                continue;
            }
            // Age the living cell toward fruiting maturity (aux is free until it burns).
            let ci = self.idx(fc.x, fc.y);
            if self.cells[ci].flags & FLAG_BURNING == 0 {
                self.cells[ci].aux = self.cells[ci].aux.saturating_add(1);
            }
            // Fruit: mature, under the global cap, on a die roll.
            let maturity = self.params.values[P_MATURITY] as u8;
            let cap = self.params.values[P_MAX_MUSHROOMS] as usize;
            if self.cells[ci].flags & FLAG_BURNING == 0
                && self.cells[ci].flags & FLAG_FRUITED == 0
                && self.cells[ci].aux >= maturity
                && self.mushrooms.len() < cap
                && self.has_fruiting_room(fc.x, fc.y)
                && self.chance(self.params.values[P_FRUIT_CHANCE])
            {
                self.try_fruit(fc.x, fc.y);
                self.cells[ci].flags |= FLAG_FRUITED;
            }
            let grew = self.colonize_from(i);
            // An exhausted cell (no more soil/empty to colonize) still loiters in the
            // frontier until it reaches fruiting maturity, so isolated interior mycelium
            // keeps aging after its immediate neighborhood is fully colonized. Once mature
            // it retires for good (it has already had its fruiting roll for this tick).
            if !self.has_colonizable_neighbor_or_bridge(fc.x, fc.y, fc.reach)
                && self.cells[ci].aux >= maturity
            {
                self.frontier.swap_remove(i); // exhausted and mature -> retire
            } else {
                i += 1;
            }
            let _ = grew;
        }
        // Mushroom growth + puffs are added in Tasks 3-5; for now a no-op if empty.
        self.grow_mushrooms();
        self.puff_caps();
    }

    /// Reveal more of each growing mushroom this tick; retire finished ones.
    /// Completed caps are recorded in `self.caps` so they can later puff spores (Task 5).
    pub fn grow_mushrooms(&mut self) {
        let reveal = self.params.values[P_MUSH_REVEAL] as u16;
        let mut i = 0;
        while i < self.mushrooms.len() {
            let done = self.reveal_mushroom(i, reveal);
            if done {
                let m = self.mushrooms[i];
                let cap_top_y = m.base_y as i32 - m.height as i32;
                if cap_top_y >= 0 {
                    let interval = (self.params.values[P_PUFF_INTERVAL] as u32).max(1);
                    self.caps.push((m.x, cap_top_y as usize, interval, 3));
                }
                self.mushrooms.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// If this spore touches soil, maybe convert that soil to a mycelium seed (and consume the spore).
    /// Returns true if the spore at (x, y) was consumed into a new mycelium seed.
    pub(crate) fn try_reseed(&mut self, x: usize, y: usize) -> bool {
        // Check soil adjacency before rolling chance() so spores with no soil neighbor
        // don't burn an RNG draw every frame for a reseed that can never happen.
        let soil_target = self
            .ortho(x, y)
            .into_iter()
            .find(|&(nx, ny)| self.material_at(nx, ny) == Material::Soil);
        let (nx, ny) = match soil_target {
            Some(t) => t,
            None => return false,
        };
        if !self.chance(self.params.values[P_RESEED_CHANCE]) {
            return false;
        }
        let (ux, uy) = (nx as usize, ny as usize);
        self.set_mycelium(ux, uy);
        self.push_frontier(FrontierCell { x: ux, y: uy, reach: 0 });
        // consume the spore
        let si = self.idx(x, y);
        self.cells[si].material = Material::Empty as u8;
        self.wake(x, y);
        true
    }

    /// Decrement each cap's puff countdown; at 0, emit a burst of SporeGas above the cap
    /// into Empty cells only, then either reset the countdown (more puffs remaining) or
    /// retire the cap (finite puffing keeps the world able to sleep once caps are spent).
    fn puff_caps(&mut self) {
        let interval = (self.params.values[P_PUFF_INTERVAL] as u32).max(1);
        let spores = self.params.values[P_PUFF_SPORES] as i32;
        let mut k = 0;
        while k < self.caps.len() {
            let (cx, cy, cd, rem) = self.caps[k];
            if cd == 0 {
                let (cxi, cyi) = (cx as i32, cy as i32);
                for s in 0..spores {
                    let px = cxi + (s - spores / 2);
                    let py = cyi - 1;
                    if self.in_bounds(px as isize, py as isize)
                        && self.material_at(px as isize, py as isize) == Material::Empty
                    {
                        let idx = self.idx(px as usize, py as usize);
                        self.cells[idx].material = Material::SporeGas as u8;
                        self.cells[idx].aux = Material::SporeGas.initial_aux();
                        self.wake(px as usize, py as usize);
                    }
                }
                if rem <= 1 {
                    self.caps.swap_remove(k); // dormant — cap is done puffing, lets the world sleep
                    continue;
                }
                self.caps[k].2 = interval;
                self.caps[k].3 = rem - 1;
            } else {
                self.caps[k].2 = cd - 1;
            }
            k += 1;
        }
    }

    /// Reveal up to `n` cells of mushroom `i`. Returns true when fully grown.
    /// Layout: cells [0, height) are the stem column going up from base_y-1;
    /// cells [height, height + cap_area) are the cap disk around the stem top.
    fn reveal_mushroom(&mut self, i: usize, n: u16) -> bool {
        let m = self.mushrooms[i];
        let stem = m.height as u16;
        let r = m.cap_r as i32;
        let cap_top_y = m.base_y as i32 - m.height as i32; // stem top; the dome's widest row
        // The dome must sit atop the stem's ACTUAL (wandered) top, not the mushroom's base x.
        let stem_top_x = m.x as i32 + stem_dx(m.sway_seed, stem.saturating_sub(1));
        // Precompute the cap dome offsets in a stable order (top-down, left-right) for determinism.
        let cap_cells = cap_dome(r); // Vec<(dx, dy)>
        let total = stem + cap_cells.len() as u16;

        let mut revealed = 0;
        while revealed < n && m.progress + revealed < total {
            let p = m.progress + revealed;
            let (cx, cy) = if p < stem {
                // stem, bottom-up, gently wandering left/right as it rises (never more than
                // 1 cell between consecutive heights, so it stays visually connected)
                (m.x as i32 + stem_dx(m.sway_seed, p), m.base_y as i32 - 1 - p as i32)
            } else {
                let (dx, dy) = cap_cells[(p - stem) as usize];
                (stem_top_x + dx, cap_top_y + dy)
            };
            if self.in_bounds(cx as isize, cy as isize) {
                let cur = self.material_at(cx as isize, cy as isize);
                // Only ever occupy Empty space -- growing into Soil (or anything else solid)
                // would let the mushroom carve straight through the ground. Skipped cells still
                // advance `progress` below, so the mushroom still completes/retires normally.
                if cur == Material::Empty {
                    let idx = self.idx(cx as usize, cy as usize);
                    self.cells[idx].material = Material::MushroomFlesh as u8;
                    self.cells[idx].aux = 0;
                    self.wake(cx as usize, cy as usize);
                }
            }
            revealed += 1;
        }
        self.mushrooms[i].progress += revealed;
        self.mushrooms[i].progress >= total
    }

    /// Whether there's enough empty headroom above (x, y) for a mushroom to occupy: the cell
    /// directly above must be Empty, and so must a few more cells of vertical clearance above
    /// that. Without this, mycelium buried inside soil/sand could fruit and immediately try to
    /// grow its stem through solid ground.
    fn has_fruiting_room(&self, x: usize, y: usize) -> bool {
        let xi = x as isize;
        for dy in 1..=4i32 {
            if self.material_at(xi, y as isize - dy as isize) != Material::Empty {
                return false;
            }
        }
        true
    }

    /// Try to colonize neighbors of frontier cell `i`. Returns true if any cell was converted.
    /// Grows into soil (reach resets to 0) and bridges empty within `P_MAX_REACH` of soil mass.
    /// Water-adjacent frontier cells get extra colonize attempts per tick (P_WATER_ACCEL).
    pub fn colonize_from(&mut self, i: usize) -> bool {
        let fc = self.frontier[i];
        let max_reach = self.params.values[P_MAX_REACH] as u8;
        // Water contact accelerates: one base attempt + extra attempts when a neighbor is water.
        let attempts = if self.water_adjacent(fc.x, fc.y) {
            1 + self.params.values[P_WATER_ACCEL] as u32
        } else {
            1
        };
        let mut grew = false;
        for _ in 0..attempts {
            let order = self.shuffled_ortho(fc.x, fc.y);
            let mut did = false;
            for (nx, ny) in order {
                let m = self.material_at(nx, ny);
                if m == Material::Soil {
                    let (ux, uy) = (nx as usize, ny as usize);
                    self.set_mycelium(ux, uy);
                    self.push_frontier(FrontierCell { x: ux, y: uy, reach: 0 });
                    did = true;
                    break;
                }
                // Bridge into empty only if this growth is still within reach of soil mass.
                if m == Material::Empty && fc.reach < max_reach && self.in_bounds(nx, ny) {
                    let (ux, uy) = (nx as usize, ny as usize);
                    self.set_mycelium(ux, uy);
                    self.push_frontier(FrontierCell { x: ux, y: uy, reach: fc.reach + 1 });
                    did = true;
                    break;
                }
            }
            grew |= did;
            if !did {
                break;
            }
        }
        grew
    }

    /// Place a fresh mycelium cell: material + reset aux age to 0 + wake its chunk.
    pub(crate) fn set_mycelium(&mut self, x: usize, y: usize) {
        let idx = self.idx(x, y);
        self.cells[idx].material = Material::Mycelium as u8;
        self.cells[idx].aux = 0; // age starts at 0 (maturity clock, Task 3)
        self.cells[idx].flags &= !FLAG_BURNING;
        self.wake(x, y);
    }

    fn has_colonizable_neighbor_or_bridge(&self, x: usize, y: usize, reach: u8) -> bool {
        let max_reach = self.params.values[P_MAX_REACH] as u8;
        for (nx, ny) in self.ortho(x, y) {
            let m = self.material_at(nx, ny);
            if m == Material::Soil {
                return true;
            }
            if m == Material::Empty && reach < max_reach && self.in_bounds(nx, ny) {
                return true;
            }
        }
        false
    }

    fn water_adjacent(&self, x: usize, y: usize) -> bool {
        self.ortho(x, y).iter().any(|&(nx, ny)| self.material_at(nx, ny) == Material::Water)
    }

    fn shuffled_ortho(&mut self, x: usize, y: usize) -> [(isize, isize); 4] {
        let mut a = self.ortho(x, y);
        // Fisher-Yates with the sim RNG (deterministic).
        for k in (1..4).rev() {
            let j = (self.next_rand() as usize) % (k + 1);
            a.swap(k, j);
        }
        a
    }

    /// Roll a parametric mushroom shape and enqueue it to grow from (x, y).
    pub fn try_fruit(&mut self, x: usize, y: usize) -> bool {
        let hmin = self.params.values[P_MUSH_HEIGHT_MIN] as i32;
        let hmax = self.params.values[P_MUSH_HEIGHT_MAX] as i32;
        let cmin = self.params.values[P_MUSH_CAP_MIN] as i32;
        let cmax = self.params.values[P_MUSH_CAP_MAX] as i32;
        let height = self.rand_range(hmin, hmax) as u8;
        let cap_r = self.rand_range(cmin, cmax) as u8;
        let sway_seed = self.next_rand();
        self.mushrooms.push(GrowingMushroom { x, base_y: y, height, cap_r, progress: 0, sway_seed });
        true
    }

    /// Inclusive random integer in [lo, hi] using the sim RNG. lo<=hi assumed; falls back to lo.
    pub(crate) fn rand_range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_rand() as i32).rem_euclid(hi - lo + 1)
    }
}

/// Deterministic, bounded horizontal wander for a mushroom stem, seeded per-mushroom so stems
/// aren't perfectly straight. Amplitude is 1 or 2 cells (picked from the seed) and the path is
/// a triangle wave: consecutive heights (`p` and `p+1`) always differ by exactly 0 or 1 cell,
/// so the stem never gaps -- it stays visually connected as it wanders.
fn stem_dx(seed: u32, p: u16) -> i32 {
    let amp = 1 + (seed % 2) as i32; // amplitude: 1 or 2 cells
    let period = 4 * amp; // full up-down-up cycle of a unit-slope triangle wave
    let shift = (seed / 2) % period as u32; // per-seed phase offset
    let phase = ((p as u32 + shift) % period as u32) as i32;
    if phase <= amp {
        phase
    } else if phase <= 3 * amp {
        2 * amp - phase
    } else {
        phase - 4 * amp
    }
}

/// Upper-hemisphere dome of radius r as (dx, dy) offsets, deterministic order (row-major
/// top-down). dy runs from -r (the apex) to 0 (the widest row), so the dome sits ON TOP of
/// the stem -- widest at the stem top, curving up to a point -- instead of a full sphere.
fn cap_dome(r: i32) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    for dy in -r..=0 {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                cells.push((dx, dy));
            }
        }
    }
    cells
}
