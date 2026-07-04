use crate::cell::{Material, FLAG_BURNING};
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
}

impl World {
    /// Scan once (at load / after worldgen) and enqueue mycelium cells that border
    /// colonizable space. O(width*height) but runs only on demand, not per frame.
    pub fn seed_frontier(&mut self) {
        self.frontier.clear();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) == Material::Mycelium && self.has_colonizable_neighbor(x, y) {
                    self.frontier.push(FrontierCell { x, y, reach: 0 });
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

    /// Budgeted growth tick. Called from step() on the P_GROWTH_INTERVAL cadence.
    /// Returns immediately when there is nothing alive to grow (chunk-sleep safe).
    pub fn grow(&mut self) {
        if self.frontier.is_empty() && self.mushrooms.is_empty() {
            return;
        }
        let budget = self.params.values[P_GROWTH_BUDGET] as usize;
        let max_frontier = self.params.values[P_MAX_FRONTIER] as usize;

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
            if self.cells[ci].aux >= maturity
                && self.mushrooms.len() < cap
                && self.chance(self.params.values[P_FRUIT_CHANCE])
            {
                self.try_fruit(fc.x, fc.y);
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
            if self.frontier.len() > max_frontier {
                self.frontier.truncate(max_frontier); // hard cap; see Task 7 note re: logging drops
            }
        }
        // Mushroom growth + puffs are added in Tasks 3-5; for now a no-op if empty.
        self.grow_mushrooms();
    }

    /// Reveal more of each growing mushroom this tick; retire finished ones.
    pub fn grow_mushrooms(&mut self) {
        let reveal = self.params.values[P_MUSH_REVEAL] as u16;
        let mut i = 0;
        while i < self.mushrooms.len() {
            let done = self.reveal_mushroom(i, reveal);
            if done {
                self.mushrooms.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Reveal up to `n` cells of mushroom `i`. Returns true when fully grown.
    /// Layout: cells [0, height) are the stem column going up from base_y-1;
    /// cells [height, height + cap_area) are the cap disk around the stem top.
    fn reveal_mushroom(&mut self, i: usize, n: u16) -> bool {
        let m = self.mushrooms[i];
        let stem = m.height as u16;
        let r = m.cap_r as i32;
        let cap_top_y = m.base_y as i32 - m.height as i32; // center of the cap
        // Precompute the cap disk offsets in a stable order (top-down, left-right) for determinism.
        let cap_cells = cap_disk(r); // Vec<(dx, dy)>
        let total = stem + cap_cells.len() as u16;

        let mut revealed = 0;
        while revealed < n && m.progress + revealed < total {
            let p = m.progress + revealed;
            let (cx, cy) = if p < stem {
                (m.x as i32, m.base_y as i32 - 1 - p as i32) // stem, bottom-up
            } else {
                let (dx, dy) = cap_cells[(p - stem) as usize];
                (m.x as i32 + dx, cap_top_y + dy)
            };
            if self.in_bounds(cx as isize, cy as isize) {
                let cur = self.material_at(cx as isize, cy as isize);
                if cur == Material::Empty || cur == Material::Soil {
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
                    self.frontier.push(FrontierCell { x: ux, y: uy, reach: 0 });
                    did = true;
                    break;
                }
                // Bridge into empty only if this growth is still within reach of soil mass.
                if m == Material::Empty && fc.reach < max_reach && self.in_bounds(nx, ny) {
                    let (ux, uy) = (nx as usize, ny as usize);
                    self.set_mycelium(ux, uy);
                    self.frontier.push(FrontierCell { x: ux, y: uy, reach: fc.reach + 1 });
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
        self.mushrooms.push(GrowingMushroom { x, base_y: y, height, cap_r, progress: 0 });
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

/// Filled disk of radius r as (dx, dy) offsets, deterministic order (row-major top-down).
fn cap_disk(r: i32) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                cells.push((dx, dy));
            }
        }
    }
    cells
}
