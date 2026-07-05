use crate::cell::Material;
use crate::world::World;

/// Soil richness above this floor counts as a "rich hit" worth branching toward (task 3's
/// gradient-biased branching). A small floor above 0 so ordinary poor dirt doesn't trigger it.
const BRANCH_RICH_FLOOR: u8 = 30;

/// Germination grace, in growth ticks: a freshly spawned colony starts with an empty pool
/// (nutrient_pool == 0) same as one that has truly starved, so pool == 0 alone can't
/// distinguish "hasn't found food yet" from "never will". A colony gets this many growth ticks
/// to find its first food (like a spore's stored reserve) before pool == 0 is treated as real
/// starvation. Comfortably above the ~76-tick walk `tip_grows_toward_richer_substrate` needs to
/// reach food. `starving_colony_recedes_and_world_sleeps` budgets far more growth ticks than
/// this on top of the grace period, since receding a strand this long back to nothing costs one
/// growth tick per P_MY_DIEBACK cells (see recede_tip).
const STARVE_GRACE_TICKS: u32 = 90;

#[derive(Clone, Copy)]
pub struct Colony {
    pub id: u8,
    pub nutrient_pool: u32,
    pub tip_count: u16,
    pub alive: bool,
    /// Growth ticks since this colony was spawned; see STARVE_GRACE_TICKS.
    pub age_ticks: u32,
}

#[derive(Clone, Copy)]
pub struct Tip {
    pub x: usize,
    pub y: usize,
    pub colony: u8,
    pub last_dx: i8,
    pub last_dy: i8,
    pub alive: bool,
}

impl World {
    pub fn soil_richness(&self, x: usize, y: usize) -> u8 {
        if self.get(x, y) == Material::Soil { self.cell_aux(x, y) } else { 0 }
    }
    pub fn set_soil_richness(&mut self, x: usize, y: usize, v: u8) {
        let i = self.idx(x, y);
        if Material::from_u8(self.cells[i].material) == Material::Soil {
            self.cells[i].aux = v;
        }
    }
    /// Create a colony rooted at (x,y): lay a mycelium cell (aux=colony id) and one tip.
    pub fn spawn_colony(&mut self, x: usize, y: usize) -> u8 {
        let id = (self.colonies.len() as u8).wrapping_add(1); // 1-based; v1 assumes < 255 colonies
        self.colonies.push(Colony { id, nutrient_pool: 0, tip_count: 1, alive: true, age_ticks: 0 });
        let i = self.idx(x, y);
        self.cells[i].material = Material::Mycelium as u8;
        self.cells[i].aux = id;
        self.cells[i].flags &= !crate::cell::FLAG_BURNING;
        self.wake(x, y);
        self.tips.push(Tip { x, y, colony: id, last_dx: 0, last_dy: -1, alive: true });
        id
    }
    pub fn colony_count(&self) -> usize { self.colonies.iter().filter(|c| c.alive).count() }
    pub fn tip_count(&self) -> usize { self.tips.iter().filter(|t| t.alive).count() }
    pub fn colony_pool(&self, id: u8) -> u32 {
        self.colonies.iter().find(|c| c.id == id).map(|c| c.nutrient_pool).unwrap_or(0)
    }
    /// Live tip count recorded on a colony (test hook; kept in sync by grow_mycelium as tips die).
    pub fn colony_tip_count(&self, id: u8) -> u16 {
        self.colonies.iter().find(|c| c.id == id).map(|c| c.tip_count).unwrap_or(0)
    }
    /// True once a colony has had its germination grace period and still has an empty pool --
    /// i.e. it's genuinely starving, not just newly spawned and yet to find its first food.
    /// See STARVE_GRACE_TICKS.
    fn colony_starving(&self, id: u8) -> bool {
        self.colonies.iter().find(|c| c.id == id)
            .map(|c| c.nutrient_pool == 0 && c.age_ticks > STARVE_GRACE_TICKS)
            .unwrap_or(false)
    }
    /// New growth entry point. Chunk-sleep safe: no-op when no live tips.
    pub fn grow_mycelium(&mut self) {
        if self.tips.iter().all(|t| !t.alive) { return; }
        let eat = self.params.values[crate::params::P_MY_EAT];
        let cap = self.params.values[crate::params::P_MY_TIP_CAP].max(1.0) as u16;
        let branch_chance = self.params.values[crate::params::P_MY_BRANCH_CHANCE];
        let dieback = (self.params.values[crate::params::P_MY_DIEBACK].max(1.0)) as usize;
        // Age every living colony by one growth tick (bounded by colony count; see
        // STARVE_GRACE_TICKS for why age matters).
        for c in self.colonies.iter_mut() {
            if c.alive { c.age_ticks = c.age_ticks.saturating_add(1); }
        }
        for ti in 0..self.tips.len() {
            if !self.tips[ti].alive { continue; }
            let colony_id = self.tips[ti].colony;
            if self.colony_starving(colony_id) {
                // Starvation: the colony can't sustain growth. Recede from the frontier inward
                // instead of extending.
                self.recede_tip(ti, dieback);
                continue;
            }
            let rich_hit = self.extend_tip(ti, eat);
            if !self.tips[ti].alive { continue; } // boxed in; pick_step found nowhere to go
            let (x, y) = (self.tips[ti].x, self.tips[ti].y);
            // Branch mechanism 1: stepping into rich soil spawns 1-2 new tips toward the food.
            if let Some(r) = rich_hit {
                if r > BRANCH_RICH_FLOOR {
                    self.try_branch(colony_id, x, y, cap);
                    self.try_branch(colony_id, x, y, cap);
                }
            }
            // Branch mechanism 2: rare periodic branching, independent of richness.
            if self.chance(branch_chance) {
                self.try_branch(colony_id, x, y, cap);
            }
        }
        self.tips.retain(|t| t.alive); // drop dead tips so the loop stays cheap
        // Keep Colony.tip_count in sync with reality: incremented as tips branch and (mostly)
        // decremented as tips die, but recompute from the live tips here as a cheap safety net.
        // Bounded by colony/tip counts, and only runs on a growth tick (already gated by
        // P_MY_GROWTH_INTERVAL).
        for c in self.colonies.iter_mut() { c.tip_count = 0; }
        for t in self.tips.iter() {
            if let Some(c) = self.colonies.iter_mut().find(|c| c.id == t.colony) {
                c.tip_count += 1;
            }
        }
        // A colony that has fully receded (no live tips) and has nothing left to grow with
        // (empty pool) is done: mark it dead so it's no longer reported as a living colony.
        for c in self.colonies.iter_mut() {
            if c.tip_count == 0 && c.nutrient_pool == 0 {
                c.alive = false;
            }
        }
    }

    /// Extend a live tip by one cell. Returns the richness of the soil it ate into, if any
    /// (used by the caller to decide whether this was a "rich hit" worth branching on).
    fn extend_tip(&mut self, ti: usize, eat: f32) -> Option<u8> {
        let t = self.tips[ti];
        let Some((nx, ny)) = self.pick_step(t) else { self.tips[ti].alive = false; return None; };
        let dst = self.material_at(nx, ny);
        let (ux, uy) = (nx as usize, ny as usize);
        // eat if stepping into soil
        let mut eaten = None;
        if dst == Material::Soil {
            let r = self.cell_aux(ux, uy);
            eaten = Some(r);
            if let Some(c) = self.colonies.iter_mut().find(|c| c.id == t.colony) {
                c.nutrient_pool = c.nutrient_pool.saturating_add((r as f32 * eat) as u32);
            }
        }
        let i = self.idx(ux, uy);
        self.cells[i].material = Material::Mycelium as u8;
        self.cells[i].aux = t.colony; // colony id; overwrites soil richness (now spent)
        self.cells[i].flags &= !crate::cell::FLAG_BURNING;
        self.wake(ux, uy);
        self.tips[ti].x = ux;
        self.tips[ti].y = uy;
        self.tips[ti].last_dx = (nx - t.x as isize) as i8;
        self.tips[ti].last_dy = (ny - t.y as isize) as i8;
        eaten
    }

    /// Spawn one new tip for `colony_id` at (x,y) if the colony's live tip count is under the
    /// hard cap. The new tip starts with no momentum (last_dx/last_dy = 0); pick_step's own
    /// richness-sampling and shuffle naturally diverge it from its sibling on the next tick
    /// (its parent's cell is now occupied Mycelium, so it can't just retrace the same step).
    fn try_branch(&mut self, colony_id: u8, x: usize, y: usize, cap: u16) {
        if self.colony_tip_count(colony_id) >= cap { return; }
        self.tips.push(Tip { x, y, colony: colony_id, last_dx: 0, last_dy: 0, alive: true });
        if let Some(c) = self.colonies.iter_mut().find(|c| c.id == colony_id) {
            c.tip_count += 1;
        }
    }

    /// Starvation dieback: revert up to `dieback` cells to Empty this tick, one at a time,
    /// following the strand backward through adjacent same-colony mycelium (not a remembered
    /// direction vector -- that can't track a strand that turns). Each iteration reverts the
    /// tip's current cell, then hops to an adjacent Mycelium cell belonging to the same colony.
    /// If no such neighbor exists (reached the root, a branch point already claimed by another
    /// tip's recede, or the strand is fully gone), the tip has nothing left to recede into and
    /// dies.
    fn recede_tip(&mut self, ti: usize, dieback: usize) {
        let colony_id = self.tips[ti].colony;
        for _ in 0..dieback.max(1) {
            let (x, y) = (self.tips[ti].x, self.tips[ti].y);
            let i = self.idx(x, y);
            if Material::from_u8(self.cells[i].material) == Material::Mycelium {
                self.cells[i].material = Material::Empty as u8;
                self.cells[i].aux = 0;
                self.cells[i].flags = 0;
                self.wake(x, y);
            }
            match self.adjacent_same_colony_mycelium(x, y, colony_id) {
                Some((nx, ny)) => {
                    self.tips[ti].x = nx;
                    self.tips[ti].y = ny;
                }
                None => {
                    self.tips[ti].alive = false;
                    return;
                }
            }
        }
    }

    /// Find an orthogonally- or diagonally-adjacent cell that is Mycelium and belongs to
    /// `colony_id`, preferring the candidate with the FEWEST same-colony-mycelium neighbors of
    /// its own (ties broken by fixed scan order).
    ///
    /// A grown strand is a simple path (growth never steps onto existing Mycelium), but a
    /// winding one can still pass close to itself without touching, leaving a "chord": two
    /// path cells that are 8-adjacent despite being far apart along the strand. A naive
    /// first-match scan can hop across such a chord, and once it does, both cells behind the
    /// jump get reverted while walking away from them -- stranding the skipped segment with no
    /// remaining connection to anywhere the tip will ever visit again (a permanent stub, the
    /// exact bug this rewrite is fixing). A plain path cell has at most 2 same-colony neighbors
    /// (its predecessor and successor); a chord endpoint has 3+. Preferring the lower-degree
    /// neighbor keeps the walk on the actual strand instead of jumping the chord.
    fn adjacent_same_colony_mycelium(&self, x: usize, y: usize, colony_id: u8) -> Option<(usize, usize)> {
        const D: [(isize, isize); 8] = [(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)];
        let (tx, ty) = (x as isize, y as isize);
        let mut best: Option<(usize, usize)> = None;
        let mut best_degree = i32::MAX;
        for (dx, dy) in D {
            let (nx, ny) = (tx + dx, ty + dy);
            if !self.in_bounds(nx, ny) { continue; }
            let (ux, uy) = (nx as usize, ny as usize);
            if self.get(ux, uy) != Material::Mycelium || self.cell_aux(ux, uy) != colony_id { continue; }
            let degree = self.same_colony_mycelium_degree(ux, uy, colony_id);
            if degree < best_degree {
                best_degree = degree;
                best = Some((ux, uy));
            }
        }
        best
    }

    /// Count `colony_id`'s Mycelium cells among the 8 neighbors of (x, y).
    fn same_colony_mycelium_degree(&self, x: usize, y: usize, colony_id: u8) -> i32 {
        const D: [(isize, isize); 8] = [(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)];
        let (tx, ty) = (x as isize, y as isize);
        let mut n = 0;
        for (dx, dy) in D {
            let (nx, ny) = (tx + dx, ty + dy);
            if !self.in_bounds(nx, ny) { continue; }
            let (ux, uy) = (nx as usize, ny as usize);
            if self.get(ux, uy) == Material::Mycelium && self.cell_aux(ux, uy) == colony_id {
                n += 1;
            }
        }
        n
    }

    /// Choose the next cell for a tip: the passable (Empty/Soil) neighbor with the highest
    /// substrate richness, momentum-biased and RNG-tie-broken. None if boxed in.
    fn pick_step(&mut self, t: Tip) -> Option<(isize, isize)> {
        let (tx, ty) = (t.x as isize, t.y as isize);
        let mut best: Option<(isize, isize)> = None;
        let mut best_score = i32::MIN;
        // evaluate the 8 neighbors in a shuffled order for unbiased ties
        let mut order = [0u8, 1, 2, 3, 4, 5, 6, 7];
        for k in (1..8).rev() { let j = (self.next_rand() as usize) % (k + 1); order.swap(k, j); }
        const D: [(isize, isize); 8] = [(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)];
        for &oi in order.iter() {
            let (dx, dy) = D[oi as usize];
            let (nx, ny) = (tx + dx, ty + dy);
            if !self.in_bounds(nx, ny) { continue; }
            let m = self.material_at(nx, ny);
            if m != Material::Empty && m != Material::Soil { continue; } // only grow into empty/soil
            let richness = if m == Material::Soil { self.cell_aux(nx as usize, ny as usize) as i32 } else { 0 };
            // momentum bias: prefer continuing roughly the same direction (thin strands, not blobs)
            let momentum = if dx == t.last_dx as isize && dy == t.last_dy as isize { 6 } else { 0 };
            let score = richness + momentum + (self.next_rand() % 3) as i32;
            if score > best_score { best_score = score; best = Some((nx, ny)); }
        }
        best
    }
}
