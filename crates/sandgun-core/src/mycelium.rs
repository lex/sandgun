use crate::cell::{Cell, Material};
use crate::world::World;
use std::collections::HashSet;

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

/// Cap on cells visited by a single connected-group flood in `drop_unsupported_around` (task 5).
/// Keeps the carve/burn-triggered support check cheap and chunk-sleep-safe -- it never scans the
/// whole world, only the local mass reachable from wherever cells were just removed. If a group
/// is still not fully explored once this many cells have been visited, the check bails WITHOUT
/// touching anything: an unverified mass is treated as supported (err toward not dropping) rather
/// than risk wrongly detaching (or spending unbounded time walking) a huge structure.
const DROP_FLOOD_BUDGET: usize = 4000;

/// Downward velocity given to a cell that falls because its group lost all anchor support.
/// Small and mostly-vertical (a gentle detach, not an explosion) per the task 5 brief.
const DROP_FALL_VY: f32 = 0.6;

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
    /// Test hook: directly set a colony's nutrient pool, for deterministic fruiting-trigger
    /// tests that don't want to wait out real eating. A no-op if the id doesn't exist.
    pub fn set_colony_pool(&mut self, id: u8, pool: u32) {
        if let Some(c) = self.colonies.iter_mut().find(|c| c.id == id) {
            c.nutrient_pool = pool;
        }
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
    /// New growth entry point. Chunk-sleep safe: no-op when there are no live tips AND nothing
    /// is mid-reveal or mid-decay (a colony can fully die -- no tips, empty pool -- while a
    /// mushroom it already fruited is still growing or waiting to crumble; that must keep
    /// draining even with zero live tips).
    pub fn grow_mycelium(&mut self) {
        let has_live_tips = self.tips.iter().any(|t| t.alive);
        if !has_live_tips && self.mushrooms.is_empty() && self.decaying_mushrooms.is_empty() {
            return;
        }
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
            // Task 5 guard: a carve or burn may have removed the cell this tip sits on (directly,
            // or via drop_unsupported_around dropping its whole disconnected group). Such a tip
            // has nothing left to extend from -- kill it instead of silently growing a fresh
            // strand out of thin air at its stale (x, y).
            let (tx, ty) = (self.tips[ti].x, self.tips[ti].y);
            if self.get(tx, ty) != Material::Mycelium {
                self.tips[ti].alive = false;
                continue;
            }
            let colony_id = self.tips[ti].colony;
            if self.colony_starving(colony_id) {
                // Starvation: the colony can't sustain growth. Recede from the tip inward
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
        // Fruiting: a colony fed above threshold spends a chunk of its pool to sprout a
        // mushroom at one of its own well-fed tips. This REPLACES the old aux-maturity random
        // fruiting trigger -- fruiting now only ever happens through the nutrient economy.
        self.fruit_fed_colonies();
        // Reveal any mushrooms fruited (this tick or earlier) and age completed ones toward
        // decay. Driven from here (not the old dormant grow()) so fruiting is self-sufficient:
        // both calls are no-ops once their lists drain empty, so this stays chunk-sleep safe.
        self.grow_mushrooms();
        self.decay_mushrooms();
    }

    /// For each alive colony whose nutrient pool has crossed P_MY_FRUIT_THRESHOLD, try to
    /// sprout a mushroom at one of its live tips that has fruiting headroom. Spends
    /// P_MY_FRUIT_COST from the pool only on an actual spawn (a footprint that doesn't fit costs
    /// nothing -- the colony just tries again on a later growth tick once it or the world has
    /// moved). At most one fruiting per colony per tick, bounded by live colony/tip counts.
    fn fruit_fed_colonies(&mut self) {
        let threshold = self.params.values[crate::params::P_MY_FRUIT_THRESHOLD].max(0.0) as u32;
        let cost = self.params.values[crate::params::P_MY_FRUIT_COST].max(0.0) as u32;
        let fed: Vec<u8> = self
            .colonies
            .iter()
            .filter(|c| c.alive && c.nutrient_pool >= threshold)
            .map(|c| c.id)
            .collect();
        for colony_id in fed {
            let candidates: Vec<(usize, usize)> = self
                .tips
                .iter()
                .filter(|t| t.alive && t.colony == colony_id)
                .map(|t| (t.x, t.y))
                .collect();
            for (x, y) in candidates {
                if self.has_fruiting_room(x, y) && self.try_fruit(x, y) {
                    if let Some(c) = self.colonies.iter_mut().find(|c| c.id == colony_id) {
                        c.nutrient_pool = c.nutrient_pool.saturating_sub(cost);
                    }
                    break; // one fruiting per colony per tick keeps this paced, not constant
                }
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

    /// Task 5 support/anchor check. Call this after a carve or burn removes Mycelium/
    /// MushroomFlesh cells centered at (cx, cy): any surviving Mycelium/MushroomFlesh nearby may
    /// have just lost its only path to an anchor (Rock or Soil terrain). For every such cell
    /// within `radius` of the removal, flood-fill its connected group (8-connectivity, matching
    /// how strands actually connect to their diagonal neighbors elsewhere in this module -- see
    /// `adjacent_same_colony_mycelium`) and check whether ANY cell in the group is orthogonally
    /// adjacent to an anchor. An unsupported group detaches entirely: every one of its cells
    /// becomes a falling particle (its own material, a small downward velocity) and is cleared.
    /// A supported group is left untouched.
    ///
    /// This is a one-shot flood triggered only by an active event (carve/burn), never a per-frame
    /// scan -- it stays local and budgeted (see DROP_FLOOD_BUDGET) so it can't cost more than a
    /// bounded amount of work even on a huge, fully-anchored mycelium mass, and it never leaves
    /// anything mid-flight that would keep the world awake once the dropped cells' particles land.
    pub fn drop_unsupported_around(&mut self, cx: isize, cy: isize, radius: isize) {
        let mut visited: HashSet<(isize, isize)> = HashSet::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let (x, y) = (cx + dx, cy + dy);
                if !self.in_bounds(x, y) || visited.contains(&(x, y)) {
                    continue;
                }
                let m = self.material_at(x, y);
                if m == Material::Mycelium || m == Material::MushroomFlesh {
                    self.flood_group_and_maybe_drop(x, y, &mut visited);
                }
            }
        }
    }

    /// Flood-fill the Mycelium/MushroomFlesh group connected to (sx, sy) (8-connectivity),
    /// marking every visited cell in `visited` so the caller's seed loop never reprocesses it.
    /// Bailing out (over budget) leaves the group entirely untouched -- no cells are mutated
    /// until the WHOLE group has been confirmed both fully explored and unsupported.
    fn flood_group_and_maybe_drop(&mut self, sx: isize, sy: isize, visited: &mut HashSet<(isize, isize)>) {
        const D8: [(isize, isize); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
        const D4: [(isize, isize); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        let mut queue: Vec<(isize, isize)> = vec![(sx, sy)];
        visited.insert((sx, sy));
        let mut group: Vec<(usize, usize)> = Vec::new();
        let mut supported = false;
        let mut qi = 0;
        while qi < queue.len() {
            if group.len() >= DROP_FLOOD_BUDGET {
                // Exceeded the budget before the group was fully explored: err toward "supported"
                // and leave everything as-is, per the task 5 ruling.
                return;
            }
            let (x, y) = queue[qi];
            qi += 1;
            group.push((x as usize, y as usize));
            for (dx, dy) in D4 {
                let am = self.material_at(x + dx, y + dy);
                if am == Material::Rock || am == Material::Soil {
                    supported = true;
                }
            }
            for (dx, dy) in D8 {
                let (nx, ny) = (x + dx, y + dy);
                if !self.in_bounds(nx, ny) || visited.contains(&(nx, ny)) {
                    continue;
                }
                let m = self.material_at(nx, ny);
                if m == Material::Mycelium || m == Material::MushroomFlesh {
                    visited.insert((nx, ny));
                    queue.push((nx, ny));
                }
            }
        }
        if supported {
            return;
        }
        // Unsupported: the whole group detaches and falls, dying as inert matter on landing (the
        // existing particle system already resettles pixels-as-particles -- no re-rooting here).
        for (ux, uy) in group {
            let i = self.idx(ux, uy);
            let mat = self.cells[i].material;
            self.cells[i] = Cell::default();
            self.spawn_particle(ux as f32 + 0.5, uy as f32 + 0.5, 0.0, DROP_FALL_VY, mat);
            self.wake(ux, uy);
        }
    }
}

// --- Parametric mushroom shapes (kept from the old M1c growth model; M1e task 6) ---
//
// Fruiting itself is now driven purely by the colony nutrient economy (`fruit_fed_colonies`
// above), but the actual mushroom SHAPE -- a stem that wanders gently as it rises, topped by an
// upper-hemisphere cap dome -- and its cell-by-cell reveal/decay are unchanged from M1c. This
// section is the single source of truth for that shape (`mushroom_footprint`), the growing-list
// reveal (`grow_mushrooms`/`reveal_mushroom`), the fit-check + spawn (`try_fruit`), and simple v1
// decay (`decay_mushrooms`).

/// A mushroom being revealed cell-by-cell. Shape fields set at fruiting time (`try_fruit`).
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

/// A fully-grown mushroom counting down to decay. Stores the same shape fields as
/// `GrowingMushroom` so its footprint can be recomputed via `mushroom_footprint` once it's time
/// to crumble -- no need to remember every cell up front.
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
    pub fn mushroom_len(&self) -> usize {
        self.mushrooms.len()
    }

    /// Reveal more of each growing mushroom this tick; retire finished ones into
    /// `decaying_mushrooms` so they eventually crumble away. Driven from `grow_mycelium`'s
    /// cadence, which is the sole owner of this call now that the old dormant grow() is gone.
    pub fn grow_mushrooms(&mut self) {
        let reveal = self.params.values[crate::params::P_MUSH_REVEAL] as u16;
        let mut i = 0;
        while i < self.mushrooms.len() {
            let done = self.reveal_mushroom(i, reveal);
            if done {
                let m = self.mushrooms[i];
                let lifespan = (self.params.values[crate::params::P_MUSH_LIFESPAN] as u32).max(1);
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
                let product = if self.chance(self.params.values[crate::params::P_ASH_CHANCE]) {
                    Material::Ash
                } else {
                    Material::Empty
                };
                let shade = (self.next_rand() & 3) as u8;
                let idx = self.idx(ux, uy);
                self.cells[idx] = Cell::new(product, shade);
                self.wake(ux, uy);
            }
            self.decaying_mushrooms.swap_remove(i);
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

    /// Roll a parametric mushroom shape and, if its whole footprint has room to grow, enqueue
    /// it. Returns false (and spawns nothing) if any in-bounds footprint cell is already
    /// occupied -- by another mushroom, terrain, or anything else non-Empty -- which is what
    /// used to let adjacent mushrooms interleave/merge (Fix A, M1c).
    pub fn try_fruit(&mut self, x: usize, y: usize) -> bool {
        let hmin = self.params.values[crate::params::P_MUSH_HEIGHT_MIN] as i32;
        let hmax = self.params.values[crate::params::P_MUSH_HEIGHT_MAX] as i32;
        let cmin = self.params.values[crate::params::P_MUSH_CAP_MIN] as i32;
        let cmax = self.params.values[crate::params::P_MUSH_CAP_MAX] as i32;
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
