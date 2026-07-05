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

/// A fully-grown mushroom counting down to decay (M1e task 4 v1 decay). Stores the same shape
/// fields as `GrowingMushroom` so its footprint can be recomputed via `mushroom_footprint` once
/// it's time to crumble -- no need to remember every cell up front.
#[derive(Clone, Copy)]
pub struct DecayingMushroom {
    pub x: usize,
    pub base_y: usize,
    pub height: u8,
    pub cap_r: u8,
    pub sway_seed: u32,
    pub ticks_left: u32,
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
                // Bridge-aware (not just Soil-only): mycelium painted where its only neighbors
                // are Empty must still join the frontier so it can bridge, matching what normal
                // colonization can already do. reach=0 since a freshly painted cell isn't yet
                // known to be any particular distance from soil mass.
                if self.get(x, y) == Material::Mycelium
                    && self.has_colonizable_neighbor_or_bridge(x, y, 0)
                {
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
    ///
    /// Mushroom reveal/decay is owned solely by the new `grow_mycelium` path (mycelium.rs),
    /// which already calls `grow_mushrooms` + `decay_mushrooms` once per its own cadence. This
    /// old dormant frontier model must NOT also touch mushrooms/caps -- it used to, and because
    /// both cadences default to the same interval and both countdowns start at 0, that fired the
    /// reveal/puff twice per growth tick (see M1e task 4 review). The guard below depends only
    /// on the frontier now (nothing seeds it in normal play, so this stays dormant) rather than
    /// also keying off mushrooms/caps state that belongs to the other path.
    pub fn grow(&mut self) {
        if self.frontier.is_empty() {
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
                && self.try_fruit(fc.x, fc.y)
            {
                // Only mark this cell as fruited if a mushroom actually spawned -- a cell whose
                // footprint didn't fit (e.g. blocked by a neighboring mushroom) keeps its
                // fruiting eligibility and can roll again on a later tick within its bounded
                // window, instead of being silently locked out forever.
                self.cells[ci].flags |= FLAG_FRUITED;
            }
            let grew = self.colonize_from(i);
            // An exhausted cell (no more soil/empty to colonize) still loiters in the
            // frontier, aging every tick, for as long as it remains a viable fruiting
            // candidate: retiring the instant it turns mature would give it essentially ONE
            // fruiting roll (at default P_FRUIT_CHANCE = 0.02, whole colonies could and did
            // produce zero mushrooms). Instead it only retires once it can no longer usefully
            // fruit -- already fruited, permanently out of room, or its age has saturated the
            // u8 aux ceiling (255) after a long bounded window of rolls. aux is monotone
            // (saturating_add, never reset while exhausted) so this is still a hard, bounded
            // number of ticks: the frontier provably still drains to empty.
            if !self.has_colonizable_neighbor_or_bridge(fc.x, fc.y, fc.reach)
                && (self.cells[ci].flags & FLAG_FRUITED != 0
                    || !self.has_fruiting_room(fc.x, fc.y)
                    || self.cells[ci].aux == u8::MAX)
            {
                self.frontier.swap_remove(i); // exhausted and can no longer usefully fruit -> retire
            } else {
                i += 1;
            }
            let _ = grew;
        }
        // Mushroom reveal/decay is NOT called here -- grow_mycelium (mycelium.rs) is the sole
        // owner now (see the doc comment on this fn). Calling it here too was the double-reveal
        // bug fixed in M1e task 4 review.
    }

    /// Reveal more of each growing mushroom this tick; retire finished ones.
    /// Completed caps are recorded in `self.caps` so they can later puff spores, and in
    /// `self.decaying_mushrooms` so they eventually crumble away (M1e task 4 v1 decay).
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
                let lifespan = (self.params.values[P_MUSH_LIFESPAN] as u32).max(1);
                self.decaying_mushrooms.push(DecayingMushroom {
                    x: m.x,
                    base_y: m.base_y,
                    height: m.height,
                    cap_r: m.cap_r,
                    sway_seed: m.sway_seed,
                    ticks_left: lifespan,
                });
                self.mushrooms.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Age completed mushrooms toward decay; once a mushroom's lifespan runs out its flesh
    /// crumbles cell-by-cell to Ash (P_ASH_CHANCE) or Empty, same product mix as burnt-out
    /// mycelium/flesh. Bounded list that drains to empty once nothing is decaying, so it stays
    /// chunk-sleep safe (no per-frame scan; only ever as big as the completed-mushroom count).
    pub fn decay_mushrooms(&mut self) {
        let mut i = 0;
        while i < self.decaying_mushrooms.len() {
            if self.decaying_mushrooms[i].ticks_left > 0 {
                self.decaying_mushrooms[i].ticks_left -= 1;
                i += 1;
                continue;
            }
            let m = self.decaying_mushrooms[i];
            let footprint = mushroom_footprint(m.x as i32, m.base_y as i32, m.height, m.cap_r, m.sway_seed);
            for (cx, cy) in footprint {
                if !self.in_bounds(cx as isize, cy as isize) {
                    continue;
                }
                if self.material_at(cx as isize, cy as isize) != Material::MushroomFlesh {
                    continue; // already carved away, burned, etc.
                }
                let (ux, uy) = (cx as usize, cy as usize);
                let product = if self.chance(self.params.values[P_ASH_CHANCE]) {
                    Material::Ash
                } else {
                    Material::Empty
                };
                let shade = (self.next_rand() & 3) as u8;
                let idx = self.idx(ux, uy);
                self.cells[idx] = crate::cell::Cell::new(product, shade);
                self.wake(ux, uy);
            }
            self.decaying_mushrooms.swap_remove(i);
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
    /// No longer called (was old grow()'s cap-puff spore mechanic, an old-model concern being
    /// removed wholesale in Task 6) -- kept for now rather than deleted, per that task's scope.
    #[allow(dead_code)]
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
    /// cells [height, height + cap_area) are the cap dome around the stem top.
    fn reveal_mushroom(&mut self, i: usize, n: u16) -> bool {
        let m = self.mushrooms[i];
        // Same geometry the fruiting-time fit-check uses (see `mushroom_footprint`), so what
        // gets drawn here always matches what was verified clear before this mushroom spawned.
        let footprint = mushroom_footprint(m.x as i32, m.base_y as i32, m.height, m.cap_r, m.sway_seed);
        let total = footprint.len() as u16;

        let mut revealed = 0;
        while revealed < n && m.progress + revealed < total {
            let p = m.progress + revealed;
            let (cx, cy) = footprint[p as usize];
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
    pub(crate) fn has_fruiting_room(&self, x: usize, y: usize) -> bool {
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

    /// Roll a parametric mushroom shape and, if its whole footprint has room to grow, enqueue
    /// it. Returns false (and spawns nothing) if any in-bounds footprint cell is already
    /// occupied -- by another mushroom, terrain, or anything else non-Empty -- which is what
    /// used to let adjacent mushrooms interleave/merge (Fix A).
    pub fn try_fruit(&mut self, x: usize, y: usize) -> bool {
        let hmin = self.params.values[P_MUSH_HEIGHT_MIN] as i32;
        let hmax = self.params.values[P_MUSH_HEIGHT_MAX] as i32;
        let cmin = self.params.values[P_MUSH_CAP_MIN] as i32;
        let cmax = self.params.values[P_MUSH_CAP_MAX] as i32;
        let height = self.rand_range(hmin, hmax) as u8;
        let cap_r = self.rand_range(cmin, cmax) as u8;
        let sway_seed = self.next_rand();

        let footprint = mushroom_footprint(x as i32, y as i32, height, cap_r, sway_seed);
        let clear = footprint.iter().all(|&(cx, cy)| {
            !self.in_bounds(cx as isize, cy as isize)
                || self.material_at(cx as isize, cy as isize) == Material::Empty
        });
        if !clear {
            return false;
        }

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

/// All absolute (x, y) cells a mushroom with this shape occupies: the stem column (bottom-up,
/// wandering per `stem_dx`) followed by the cap dome (per `cap_dome`, already bottom-up) atop
/// the stem's ACTUAL wandered top. This is the single source of truth for a mushroom's
/// footprint -- used both to check the whole shape is clear before fruiting (`try_fruit`) and
/// to reveal it cell-by-cell as it grows (`reveal_mushroom`), so the two can never disagree.
fn mushroom_footprint(x: i32, base_y: i32, height: u8, cap_r: u8, sway_seed: u32) -> Vec<(i32, i32)> {
    let stem = height as u16;
    let r = cap_r as i32;
    let cap_top_y = base_y - height as i32; // stem top; the dome's widest row
    let stem_top_x = x + stem_dx(sway_seed, stem.saturating_sub(1));
    let cap_cells = cap_dome(r);

    let mut cells = Vec::with_capacity(stem as usize + cap_cells.len());
    for p in 0..stem {
        // bottom-up, gently wandering left/right as it rises (never more than 1 cell between
        // consecutive heights, so it stays visually connected)
        cells.push((x + stem_dx(sway_seed, p), base_y - 1 - p as i32));
    }
    for (dx, dy) in cap_cells {
        cells.push((stem_top_x + dx, cap_top_y + dy));
    }
    cells
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
/// bottom-up). dy runs from 0 (the widest row, flush against the stem top) to -r (the apex),
/// so the dome reveals grounded-first: the widest row fills in before the rows curving up
/// above it, instead of the apex appearing to float in open air before the rest connects.
fn cap_dome(r: i32) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    for dy in (-r..=0).rev() {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                cells.push((dx, dy));
            }
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::cap_dome;

    #[test]
    fn cap_reveals_from_bottom_up() {
        // Regression: cap_dome used to order dy from -r (apex) to 0 (widest row), so the very
        // first cells revealed were the apex, floating above the stem top before the rest of
        // the dome filled in. The dome must reveal bottom-up (grounded on the stem top first).
        let r = 5;
        let cells = cap_dome(r);
        assert!(!cells.is_empty());
        let first = cells.first().unwrap();
        let last = cells.last().unwrap();
        assert_eq!(first.1, 0, "first cap cell revealed should be on the widest row (dy == 0), atop the stem");
        assert_eq!(last.1, -r, "last cap cell revealed should be the apex (dy == -r)");
    }
}
