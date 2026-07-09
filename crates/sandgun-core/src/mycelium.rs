use crate::cell::{Cell, Material, FLAG_BURNING};
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
    /// Live Mycelium cells in the grid owned by this colony (aux == id). A colony is only reaped
    /// (and its id recycled) once this reaches 0, so a recycled id can never mislabel orphan cells.
    pub cell_count: u32,
}

#[derive(Clone, Copy)]
pub struct Tip {
    pub x: usize,
    pub y: usize,
    pub colony: u8,
    pub last_dx: i8,
    pub last_dy: i8,
    pub alive: bool,
    /// Consecutive Empty-cell steps since this tip last touched Soil (reset to 0 on stepping into
    /// Soil, incremented on stepping into Empty). Caps how far a strand can fly through open air
    /// before it must reach substrate again -- see P_MY_MAX_AIR_REACH in pick_step.
    pub air_run: u8,
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
    /// Create a colony rooted at (x,y): lay a mycelium cell (aux=colony id) and one tip. Returns
    /// 0 (no colony created) if the concurrent-colony cap has been reached -- see
    /// `alloc_colony_id`; callers must tolerate 0.
    pub fn spawn_colony(&mut self, x: usize, y: usize) -> u8 {
        let Some(id) = self.alloc_colony_id() else { return 0; };
        self.colonies.push(Colony { id, nutrient_pool: 0, tip_count: 1, alive: true, age_ticks: 0, cell_count: 1 });
        let i = self.idx(x, y);
        // Root cell overwrite: Spore ammo can land on an existing colony's live Mycelium cell,
        // transferring that grid cell to this new colony. Same aux caveat as every other removal
        // site -- only decrement the OLD owner if the cell was never lit (aux still its colony
        // id, not a stale fuel countdown; a burning cell was already accounted at ignition).
        if Material::from_u8(self.cells[i].material) == Material::Mycelium
            && self.cells[i].flags & crate::cell::FLAG_BURNING == 0
        {
            let old_id = self.cells[i].aux;
            self.colony_cell_removed(old_id);
        }
        self.cells[i].material = Material::Mycelium as u8;
        self.cells[i].aux = id;
        self.cells[i].flags &= !crate::cell::FLAG_BURNING;
        self.wake(x, y);
        self.tips.push(Tip { x, y, colony: id, last_dx: 0, last_dy: -1, alive: true, air_run: 0 });
        id
    }

    /// Allocate a colony id: reuse a freed one, else the next unused id (1..=255). None at the
    /// cap -- either the 255 hard cap (aux is u8, 0 reserved for "none") or the lower
    /// `P_MY_MAX_COLONIES` soft cap (task 2), whichever is smaller. `colonies.len()` is exactly
    /// the count of currently-live colonies (reaped ones are removed from the Vec, not just
    /// marked dead -- see Task 1), so this caps CONCURRENT colonies, not cumulative spawns.
    fn alloc_colony_id(&mut self) -> Option<u8> {
        let cap = (self.params.values[crate::params::P_MY_MAX_COLONIES].max(0.0) as usize).min(255);
        if self.free_colony_ids.is_empty() && self.colonies.len() >= cap { return None; }
        if let Some(id) = self.free_colony_ids.pop() { return Some(id); }
        let next = self.colonies.len() + 1;
        if next <= 255 { Some(next as u8) } else { None }
    }

    /// Account a Mycelium cell newly laid for `id`.
    pub(crate) fn colony_cell_laid(&mut self, id: u8) {
        if let Some(c) = self.colonies.iter_mut().find(|c| c.id == id) { c.cell_count = c.cell_count.saturating_add(1); }
    }
    /// Account a Mycelium cell of `id` removed from the grid (reverted/carved/burned/dissolved/dropped).
    pub(crate) fn colony_cell_removed(&mut self, id: u8) {
        if let Some(c) = self.colonies.iter_mut().find(|c| c.id == id) { c.cell_count = c.cell_count.saturating_sub(1); }
    }
    pub fn colony_count(&self) -> usize { self.colonies.iter().filter(|c| c.alive).count() }
    pub fn tip_count(&self) -> usize { self.tips.iter().filter(|t| t.alive).count() }
    pub fn colony_pool(&self, id: u8) -> u32 {
        self.colonies.iter().find(|c| c.id == id).map(|c| c.nutrient_pool).unwrap_or(0)
    }
    /// HUD passthrough: the largest nutrient pool among currently-alive colonies (0 if none are
    /// alive). Gives a rough sense of feeding progress toward P_MY_FRUIT_THRESHOLD without the
    /// HUD needing to know colony ids or iterate the colony list itself.
    pub fn max_colony_pool(&self) -> u32 {
        self.colonies.iter().filter(|c| c.alive).map(|c| c.nutrient_pool).max().unwrap_or(0)
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
    /// Live Mycelium cells in the grid currently owned by this colony (test hook; 0 if the id
    /// doesn't exist -- either never allocated, or already reaped).
    pub fn colony_cell_count(&self, id: u8) -> u32 {
        self.colonies.iter().find(|c| c.id == id).map(|c| c.cell_count).unwrap_or(0)
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
            let air_run = self.tips[ti].air_run;
            // Branch mechanism 1: stepping into rich soil spawns 1-2 new tips toward the food.
            if let Some(r) = rich_hit {
                if r > BRANCH_RICH_FLOOR {
                    self.try_branch(colony_id, x, y, cap, air_run);
                    self.try_branch(colony_id, x, y, cap, air_run);
                }
            }
            // Branch mechanism 2: rare periodic branching, independent of richness.
            if self.chance(branch_chance) {
                self.try_branch(colony_id, x, y, cap, air_run);
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
        // A colony with no live tips is functionally dead (nothing left to grow or fruit from).
        // Zero any leftover pool so it can't linger as a "zombie", then reap it once none of its
        // cells remain in the grid -- freeing its id for reuse (an id with live cells is NOT
        // recycled, so a new colony can never inherit old cells).
        for c in self.colonies.iter_mut() {
            if c.tip_count == 0 { c.alive = false; c.nutrient_pool = 0; }
        }
        let mut freed: Vec<u8> = Vec::new();
        self.colonies.retain(|c| {
            let reap = !c.alive && c.cell_count == 0;
            if reap { freed.push(c.id); }
            !reap
        });
        self.free_colony_ids.extend(freed);
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
    ///
    /// Global cap (P_MAX_MUSHROOMS, review fix): without this, enough well-fed colonies could
    /// keep fruiting forever with no ceiling on how many mushrooms exist at once. "Simultaneous"
    /// means every mushroom still visible on the map -- both still growing (`self.mushrooms`) and
    /// fully grown but not yet crumbled (`self.decaying_mushrooms`) occupy real MushroomFlesh
    /// cells on screen, so both count toward the cap. Checked once per colony (not just once per
    /// tick) so a cap reached mid-loop stops the rest of this tick's fruiting immediately, rather
    /// than letting later colonies squeeze in one more before the count is rechecked.
    fn fruit_fed_colonies(&mut self) {
        let threshold = self.params.values[crate::params::P_MY_FRUIT_THRESHOLD].max(0.0) as u32;
        let cost = self.params.values[crate::params::P_MY_FRUIT_COST].max(0.0) as u32;
        let cap = self.params.values[crate::params::P_MAX_MUSHROOMS].max(0.0) as usize;
        let fed: Vec<u8> = self
            .colonies
            .iter()
            .filter(|c| c.alive && c.nutrient_pool >= threshold)
            .map(|c| c.id)
            .collect();
        for colony_id in fed {
            if self.mushrooms.len() + self.decaying_mushrooms.len() >= cap {
                break; // at the global simultaneous-mushroom cap; no more fruiting this tick
            }
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
        self.colony_cell_laid(t.colony);
        let (dx, dy) = (nx - t.x as isize, ny - t.y as isize);
        self.tips[ti].x = ux;
        self.tips[ti].y = uy;
        self.tips[ti].last_dx = dx as i8;
        self.tips[ti].last_dy = dy as i8;
        // FIX 2: track how far this tip has flown through open air since it last touched soil;
        // pick_step refuses to extend further into Empty once this exceeds P_MY_MAX_AIR_REACH.
        self.tips[ti].air_run = if dst == Material::Soil { 0 } else { t.air_run.saturating_add(1) };
        // FIX 4: thicken the strand -- lay P_MY_STRAND_WIDTH-1 extra cells perpendicular to the
        // movement direction, so strands are orthogonally (not just diagonally/corner) connected
        // and read as solid rather than hairline-thin.
        self.thicken_strand(t.colony, nx, ny, dx, dy, eat);
        eaten
    }

    /// Lay extra Mycelium cells alongside a tip's just-taken step (dx, dy) so the strand is
    /// orthogonally (4-)connected, not just corner-touching. Only ever overwrites Soil or Empty
    /// (never carves through Rock/other terrain); a Soil cell eaten this way still feeds the
    /// colony's pool. Bounded by P_MY_STRAND_WIDTH (meant to stay small, 2-3).
    fn thicken_strand(&mut self, colony_id: u8, nx: isize, ny: isize, dx: isize, dy: isize, eat: f32) {
        let width = (self.params.values[crate::params::P_MY_STRAND_WIDTH].max(1.0) as i32).min(3);
        if width <= 1 { return; }
        // Candidate extra cells, nearest first. dx/dy are each in {-1,0,1} and never both 0 (a
        // real step was taken).
        let extra: [(isize, isize); 2] = if dx != 0 && dy != 0 {
            // Diagonal move: a diagonal step's two endpoints are only corner-connected to each
            // other (8-connectivity) -- exactly the "corner-only" straight-45-degree-line problem
            // fix 3 calls out. The two cells that ORTHOGONALLY bridge the source (nx-dx, ny-dy)
            // and destination (nx, ny) into true 4-connectivity are (nx, ny-dy) and (nx-dx, ny)
            // -- each shares an edge with both endpoints. (A naive 90-degree rotation of (dx, dy)
            // would instead give a cell only diagonally touching the destination, which would NOT
            // fix the corner-connectivity problem this thickening exists for.)
            [(nx, ny - dy), (nx - dx, ny)]
        } else {
            // Orthogonal move: thicken sideways, one cell on each side of the direction of
            // travel (a 90-degree rotation of (dx, dy) is itself orthogonal here, so it directly
            // neighbors the destination).
            let (perp_dx, perp_dy) = (-dy, dx);
            [(nx + perp_dx, ny + perp_dy), (nx - perp_dx, ny - perp_dy)]
        };
        for &(wx, wy) in extra.iter().take((width - 1) as usize) {
            if !self.in_bounds(wx, wy) { continue; }
            let wm = self.material_at(wx, wy);
            if wm != Material::Empty && wm != Material::Soil { continue; } // never overwrite terrain
            let (wux, wuy) = (wx as usize, wy as usize);
            if wm == Material::Soil {
                let r = self.cell_aux(wux, wuy);
                if let Some(c) = self.colonies.iter_mut().find(|c| c.id == colony_id) {
                    c.nutrient_pool = c.nutrient_pool.saturating_add((r as f32 * eat) as u32);
                }
            }
            let wi = self.idx(wux, wuy);
            self.cells[wi].material = Material::Mycelium as u8;
            self.cells[wi].aux = colony_id;
            self.cells[wi].flags &= !crate::cell::FLAG_BURNING;
            self.wake(wux, wuy);
            self.colony_cell_laid(colony_id);
        }
    }

    /// Spawn one new tip for `colony_id` at (x,y) if the colony's live tip count is under the
    /// hard cap. The new tip starts with no momentum (last_dx/last_dy = 0); pick_step's own
    /// richness-sampling and shuffle naturally diverge it from its sibling on the next tick
    /// (its parent's cell is now occupied Mycelium, so it can't just retrace the same step).
    /// Inherits `air_run` from the parent tip it branched off of -- a branch spawned deep in open
    /// air shouldn't reset its air budget for free.
    fn try_branch(&mut self, colony_id: u8, x: usize, y: usize, cap: u16, air_run: u8) {
        if self.colony_tip_count(colony_id) >= cap { return; }
        self.tips.push(Tip { x, y, colony: colony_id, last_dx: 0, last_dy: 0, alive: true, air_run });
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
                // Only trust aux as the colony id if this cell was never lit: once a Mycelium
                // cell ignites, its aux becomes the fuel countdown (see ignite_blast/
                // ignite_neighbors in world.rs), and colony_cell_removed was already called
                // there -- reading aux here for a burning cell would misattribute a fuel number
                // as some other colony's id.
                if self.cells[i].flags & crate::cell::FLAG_BURNING == 0 {
                    let cid = self.cells[i].aux;
                    self.colony_cell_removed(cid);
                }
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
    ///
    /// A neighbor also counts as same-colony when it's Mycelium and FLAG_BURNING (task 4, M1e
    /// cleanup): once a cell ignites, its aux becomes the fuel countdown rather than the colony
    /// id (see ignite_blast/ignite_neighbors), so a plain aux == colony_id check would treat a
    /// burning same-strand neighbor as foreign and die at it, stranding everything beyond it as
    /// a permanent stub. A burning neighbor here is *usually* part of THIS strand and will
    /// self-remove via burnout regardless, so traversing it is nearly always right and cheap.
    /// ACCEPTED BOUNDED TRADEOFF (not an excluded case): pick_step does NOT forbid growing
    /// adjacent to a different colony, so two concurrent colonies' strands can end up 8-adjacent.
    /// A burning cell can't be attributed by colony (its aux is fuel), so at such a boundary a
    /// receding walk may step onto the other colony's burning cell. This can't corrupt cell_count
    /// (each colony released that cell at its own ignition) or break termination (the cell is
    /// reverted to Empty, never revisited); worst case it relocates a cosmetic stub. Fully fixing
    /// it needs per-cell colony id preserved through burning, which the 4-byte cell can't hold.
    fn adjacent_same_colony_mycelium(&self, x: usize, y: usize, colony_id: u8) -> Option<(usize, usize)> {
        const D: [(isize, isize); 8] = [(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)];
        let (tx, ty) = (x as isize, y as isize);
        let mut best: Option<(usize, usize)> = None;
        let mut best_degree = i32::MAX;
        for (dx, dy) in D {
            let (nx, ny) = (tx + dx, ty + dy);
            if !self.in_bounds(nx, ny) { continue; }
            let (ux, uy) = (nx as usize, ny as usize);
            if self.get(ux, uy) != Material::Mycelium
                || (self.cell_aux(ux, uy) != colony_id && self.cell_flags(ux, uy) & FLAG_BURNING == 0)
            {
                continue;
            }
            let degree = self.same_colony_mycelium_degree(ux, uy, colony_id);
            if degree < best_degree {
                best_degree = degree;
                best = Some((ux, uy));
            }
        }
        best
    }

    /// Count `colony_id`'s Mycelium cells among the 8 neighbors of (x, y). See
    /// `adjacent_same_colony_mycelium` for why a burning Mycelium neighbor also counts.
    fn same_colony_mycelium_degree(&self, x: usize, y: usize, colony_id: u8) -> i32 {
        const D: [(isize, isize); 8] = [(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)];
        let (tx, ty) = (x as isize, y as isize);
        let mut n = 0;
        for (dx, dy) in D {
            let (nx, ny) = (tx + dx, ty + dy);
            if !self.in_bounds(nx, ny) { continue; }
            let (ux, uy) = (nx as usize, ny as usize);
            if self.get(ux, uy) == Material::Mycelium
                && (self.cell_aux(ux, uy) == colony_id || self.cell_flags(ux, uy) & FLAG_BURNING != 0)
            {
                n += 1;
            }
        }
        n
    }

    /// Choose the next cell for a tip: the passable (Empty/Soil) neighbor with the highest
    /// substrate richness, momentum-biased and RNG-tie-broken. None if boxed in.
    ///
    /// M1e playtest fixes:
    /// - Soil gets a large flat bonus over Empty so a tip essentially always prefers substrate
    ///   when it's available, instead of drifting into open air (FIX 2).
    /// - A tip that has already flown P_MY_MAX_AIR_REACH consecutive Empty cells without
    ///   touching Soil may not step into Empty again -- only Soil candidates remain for it. If
    ///   none exist, this returns None and the tip dies rather than sailing further into air
    ///   (FIX 2).
    /// - Momentum is a much smaller bonus than before, and the 4 orthogonal neighbors get a
    ///   small bonus over the 4 diagonals, so strands wiggle and curve instead of locking onto
    ///   straight 45-degree, corner-only-connected rays. The random wander term's range is also
    ///   widened (FIX 3).
    fn pick_step(&mut self, t: Tip) -> Option<(isize, isize)> {
        let (tx, ty) = (t.x as isize, t.y as isize);
        let max_air_reach = self.params.values[crate::params::P_MY_MAX_AIR_REACH].max(0.0) as u8;
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
            if m == Material::Empty && t.air_run >= max_air_reach { continue; } // out of air budget
            let richness = if m == Material::Soil { self.cell_aux(nx as usize, ny as usize) as i32 } else { 0 };
            let soil_bonus = if m == Material::Soil { 40 } else { 0 }; // strongly prefer substrate
            // momentum bias: mild preference for continuing roughly the same direction
            let momentum = if dx == t.last_dx as isize && dy == t.last_dy as isize { 2 } else { 0 };
            // orthogonal bias: de-bias pure 45-degree diagonals (corner-only connected) in favor
            // of the 4 orthogonal neighbors, so strands stay wiggly and 4-connected
            let orthogonal = if dx == 0 || dy == 0 { 2 } else { 0 };
            let score = richness + soil_bonus + momentum + orthogonal + (self.next_rand() % 5) as i32;
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
        self.seed_drop_scan(cx, cy, radius, &mut visited);
    }

    /// Batched, deduped counterpart to `drop_unsupported_around` (M1e cleanup task 3). Every site
    /// that removed a Mycelium/MushroomFlesh (or its anchoring Soil) cell this step -- burnout,
    /// acid dissolve, or a kinetic/acid ammo impact -- pushed `(x, y, radius)` onto
    /// `pending_drop_checks` instead of flooding inline. This drains that list once, sharing ONE
    /// `visited` set across every entry, so overlapping removal sites (a big fire or acid pool
    /// eating a large mycelium mass triggers this once per cell) check each connected group at
    /// most once per step instead of re-flooding the same territory repeatedly.
    ///
    /// No-ops when nothing was queued (leaves `visited` unallocated) so a settled/sleeping world
    /// does zero work here, same as `drop_unsupported_around`'s existing chunk-sleep-safe budget.
    /// Determinism: `visited` is membership-only (never iterated), and the drop order follows the
    /// fixed push order of `pending_drop_checks` plus each flood's own deterministic queue order.
    pub fn drop_unsupported_pending(&mut self) {
        if self.pending_drop_checks.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_drop_checks);
        let mut visited: HashSet<(isize, isize)> = HashSet::new();
        for (cx, cy, radius) in pending {
            self.seed_drop_scan(cx, cy, radius, &mut visited);
        }
    }

    /// Shared seed-search core of `drop_unsupported_around`/`drop_unsupported_pending`: scans the
    /// `radius`-square around (cx, cy) for Mycelium/MushroomFlesh cells not already in `visited`
    /// and floods each one found (`flood_group_and_maybe_drop`), which extends `visited` with
    /// every cell of that connected group so later seeds (from this call or, when `visited` is
    /// shared across a whole pending batch, from a different queued site) never reprocess it.
    fn seed_drop_scan(&mut self, cx: isize, cy: isize, radius: isize, visited: &mut HashSet<(isize, isize)>) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let (x, y) = (cx + dx, cy + dy);
                if !self.in_bounds(x, y) || visited.contains(&(x, y)) {
                    continue;
                }
                let m = self.material_at(x, y);
                if m == Material::Mycelium || m == Material::MushroomFlesh {
                    self.flood_group_and_maybe_drop(x, y, visited);
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
            // Mushroom flesh has aux=0 (not colony-tracked); a burning Mycelium cell's aux is
            // already the fuel countdown (see recede_tip's comment) and was already accounted
            // for at ignition -- only decrement for a non-burning Mycelium cell.
            if mat == Material::Mycelium as u8 && self.cells[i].flags & crate::cell::FLAG_BURNING == 0 {
                let cid = self.cells[i].aux;
                self.colony_cell_removed(cid);
            }
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

    /// Count of mushrooms that have finished growing and are now counting down to crumble.
    /// Still visible (still occupying MushroomFlesh cells) so it counts toward the global
    /// simultaneous-mushroom cap (P_MAX_MUSHROOMS) alongside `mushroom_len`.
    pub fn decaying_mushroom_len(&self) -> usize {
        self.decaying_mushrooms.len()
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
