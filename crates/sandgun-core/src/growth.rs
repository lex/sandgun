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
            let grew = self.colonize_from(i);
            if !self.has_colonizable_neighbor_or_bridge(fc.x, fc.y, fc.reach) {
                self.frontier.swap_remove(i); // exhausted -> retire
            } else {
                i += 1;
            }
            let _ = grew;
            if self.frontier.len() > max_frontier {
                self.frontier.truncate(max_frontier); // hard cap; see Task 7 note re: logging drops
            }
        }
        // Mushroom growth + puffs are added in Tasks 3-5; for now a no-op if empty.
    }

    /// Try to colonize one neighbor of frontier cell `i`. Returns true if a cell was converted.
    /// Task 1: soil only. Task 2 extends this to bridge empty within reach.
    pub fn colonize_from(&mut self, i: usize) -> bool {
        let fc = self.frontier[i];
        // Randomize neighbor order so growth isn't directionally biased.
        let order = self.shuffled_ortho(fc.x, fc.y);
        for (nx, ny) in order {
            if self.material_at(nx, ny) == Material::Soil {
                let (ux, uy) = (nx as usize, ny as usize);
                self.set_mycelium(ux, uy);
                self.frontier.push(FrontierCell { x: ux, y: uy, reach: 0 });
                return true;
            }
        }
        false
    }

    /// Place a fresh mycelium cell: material + reset aux age to 0 + wake its chunk.
    pub(crate) fn set_mycelium(&mut self, x: usize, y: usize) {
        let idx = self.idx(x, y);
        self.cells[idx].material = Material::Mycelium as u8;
        self.cells[idx].aux = 0; // age starts at 0 (maturity clock, Task 3)
        self.cells[idx].flags &= !FLAG_BURNING;
        self.wake(x, y);
    }

    fn has_colonizable_neighbor_or_bridge(&self, x: usize, y: usize, _reach: u8) -> bool {
        // Task 1: soil only. Task 2 adds the in-reach empty check.
        self.has_colonizable_neighbor(x, y)
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
}
