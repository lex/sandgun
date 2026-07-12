use crate::avatar::Avatar;
use crate::cell::{Cell, Material, FLAG_BURNING};
use crate::mycelium::{Colony, DecayingMushroom, GrowingMushroom, Tip};
use crate::params::{
    Params, P_ACID_BLOB_RADIUS, P_ACID_ETCH, P_ACID_ETCH_ROCK, P_ASH_CHANCE, P_FIRE_FLICKER,
    P_FIRE_LIFETIME, P_GUNFIRE_SPORE_CHANCE, P_INCENDIARY_RADIUS, P_KINETIC_EJECTA,
    P_KINETIC_RADIUS, P_SMOKE_EMIT, P_SMOKE_LIFETIME, P_SPORE_BLOB_RADIUS,
};
use crate::particle::Particle;
use crate::projectile::{Ammo, Projectile};

pub const CHUNK: usize = 64;
pub const DISPERSION: isize = 4;
pub const PARTICLE_GRAVITY: f32 = 0.35;
const PARTICLE_MAX_SPEED: f32 = 8.0;
const PROJECTILE_MAX_SPEED: f32 = 24.0;
const AVATAR_GRAVITY: f32 = 0.3;
const AVATAR_WALK: f32 = 1.4;
const AVATAR_JUMP: f32 = 4.2;
const AVATAR_MAX_FALL: f32 = 6.0;
/// Search radius (in cells) `find_spore_landing_site` scans around a Spore round's impact point
/// for a Soil/Soil-adjacent landing site, so a round fired into open air still plants on
/// substrate instead of as a floating stub. See M1e cleanup task 2.
const SPORE_LANDING_SEARCH_RADIUS: isize = 3;

pub struct World {
    pub width: usize,
    pub height: usize,
    pub(crate) cells: Vec<Cell>,
    /// Per-cell "moved at frame (frame & 0xFF)" stamp; prevents double-updates within a step.
    pub(crate) stamp: Vec<u8>,
    pub(crate) chunks_x: usize,
    pub(crate) chunks_y: usize,
    /// Chunks to simulate this step (swapped in from active_next at step start).
    pub(crate) active: Vec<u8>,
    /// Chunks woken for the NEXT step (by movement or painting).
    pub(crate) active_next: Vec<u8>,
    pub(crate) frame: u64,
    pub(crate) rng: u32,
    pub(crate) rgba: Vec<u8>,
    /// Per-chunk "needs re-upload to the GPU" bitmap (M1d task 3). Set whenever `wake()` fires
    /// (the single choke-point every cell mutation already calls) so it piggybacks the exact
    /// same set of chunks that changed, without a second sweep. `render_rgba` only rewrites the
    /// RGBA bytes for chunks with `render_dirty[chunk] == 1`; the renderer only re-uploads those
    /// chunks' sub-rects to the GPU texture, then clears the bitmap. A settled, offscreen world
    /// wakes no chunks, so this stays all-zero and costs ~nothing to render or upload.
    pub(crate) render_dirty: Vec<u8>,
    /// Cells visited by the last step(); test + debug hook for chunk skipping.
    pub cells_processed: u64,
    pub params: Params,
    pub(crate) particles: Vec<Particle>,
    pub(crate) projectiles: Vec<Projectile>,
    pub(crate) avatar: Option<Avatar>,
    pub(crate) mushrooms: Vec<GrowingMushroom>,
    /// Completed mushrooms counting down to decay; bounded by how many mushrooms have ever
    /// finished growing and not yet crumbled.
    pub(crate) decaying_mushrooms: Vec<DecayingMushroom>,
    /// Cadence gate for grow_mycelium() (P_MY_GROWTH_INTERVAL) -- the sole growth model.
    pub(crate) my_grow_countdown: u32,
    /// M1e living-mycelium organism model: colonies and their growing tips.
    pub(crate) colonies: Vec<Colony>,
    pub(crate) tips: Vec<Tip>,
    /// Ids freed by reaped colonies (LIFO), recycled by `alloc_colony_id` before minting a new one.
    pub(crate) free_colony_ids: Vec<u8>,
    /// Sites (x, y, search_radius) queued this step where a Mycelium/MushroomFlesh (or its
    /// anchoring Soil) cell was just removed -- burnout, acid dissolve, or a kinetic/acid ammo
    /// impact. Drained once per step by `drop_unsupported_pending()` instead of each site flooding
    /// inline, so overlapping removals from a single big burn/acid event share one coalesced,
    /// deduped support check (M1e cleanup task 3). The radius travels with the site (rather than
    /// a single step-wide constant) because carve_crater/inject_blob need a search radius scaled
    /// to the blast (crater/blob radius + 2), while the per-cell sweep sites (burnout, acid
    /// dissolve) only ever need the small fixed radius that removing one cell warrants; sharing
    /// one global radius across a step would either under-search the big-blast sites (missing the
    /// surviving edge of a large crater -- wrong) or over-search the small sweep sites (harmless
    /// but wasteful), so each entry keeps the radius its own removal actually needs.
    pub(crate) pending_drop_checks: Vec<(isize, isize, isize)>,
}

impl World {
    pub fn new(width: usize, height: usize) -> World {
        assert!(width % CHUNK == 0 && height % CHUNK == 0, "world dims must be multiples of {CHUNK}");
        let (chunks_x, chunks_y) = (width / CHUNK, height / CHUNK);
        World {
            width,
            height,
            cells: vec![Cell::default(); width * height],
            stamp: vec![0; width * height],
            chunks_x,
            chunks_y,
            active: vec![0; chunks_x * chunks_y],
            active_next: vec![0; chunks_x * chunks_y],
            frame: 0,
            rng: 0x9E37_79B9,
            rgba: vec![0; width * height * 4],
            // All-dirty on construction: the very first frame has no prior GPU texture content,
            // so everything must upload once.
            render_dirty: vec![1; chunks_x * chunks_y],
            cells_processed: 0,
            params: Params::default(),
            particles: Vec::new(),
            projectiles: Vec::new(),
            avatar: None,
            mushrooms: Vec::new(),
            decaying_mushrooms: Vec::new(),
            my_grow_countdown: 0,
            colonies: Vec::new(),
            tips: Vec::new(),
            free_colony_ids: Vec::new(),
            pending_drop_checks: Vec::new(),
        }
    }

    #[inline]
    pub(crate) fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    #[inline]
    pub(crate) fn in_bounds(&self, x: isize, y: isize) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }

    pub fn get(&self, x: usize, y: usize) -> Material {
        Material::from_u8(self.cells[self.idx(x, y)].material)
    }

    /// Read the flags of a cell (test helper).
    pub fn cell_flags(&self, x: usize, y: usize) -> u8 {
        self.cells[self.idx(x, y)].flags
    }

    /// Read the aux value of a cell (test helper).
    pub fn cell_aux(&self, x: usize, y: usize) -> u8 {
        self.cells[self.idx(x, y)].aux
    }

    /// Reset every cell to Empty and clear movement stamps (used by worldgen). Also resets
    /// in-flight growth state (mushrooms/decay/cadence/colonies/tips) so a regenerated world
    /// doesn't carry growth records that point at terrain positions from the world it replaced.
    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
        self.stamp.fill(0);
        self.mushrooms.clear();
        self.decaying_mushrooms.clear();
        self.my_grow_countdown = 0;
        self.colonies.clear();
        self.tips.clear();
        self.free_colony_ids.clear();
        self.pending_drop_checks.clear();
        // The whole grid just changed under the GPU texture's feet -- force a full re-upload.
        self.mark_all_render_dirty();
    }

    /// Wake every chunk for the next step (used after worldgen).
    pub fn wake_all(&mut self) {
        self.active_next.iter_mut().for_each(|b| *b = 1);
    }

    pub(crate) fn next_rand(&mut self) -> u32 {
        // xorshift32 — deterministic, no external deps
        let mut r = self.rng;
        r ^= r << 13;
        r ^= r >> 17;
        r ^= r << 5;
        self.rng = r;
        r
    }

    /// true with probability p (0..1); deterministic via the sim RNG.
    pub(crate) fn chance(&mut self, p: f32) -> bool {
        if p <= 0.0 {
            return false;
        }
        (self.next_rand() >> 8) as f32 / 16_777_216.0 < p
    }

    /// Material at (x, y); out of bounds reads as Rock (the border blocks everything).
    pub(crate) fn material_at(&self, x: isize, y: isize) -> Material {
        if !self.in_bounds(x, y) {
            return Material::Rock;
        }
        Material::from_u8(self.cells[self.idx(x as usize, y as usize)].material)
    }

    /// Wake the chunks containing (x, y) and its 8 neighbors for the next step.
    pub(crate) fn wake(&mut self, x: usize, y: usize) {
        let (w, h) = (self.width as isize, self.height as isize);
        for dy in -1isize..=1 {
            for dx in -1isize..=1 {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let ci = (ny as usize / CHUNK) * self.chunks_x + (nx as usize / CHUNK);
                self.active_next[ci] = 1;
            }
        }
        // Render-dirty only needs the mutated cell's OWN chunk (unlike sim wake above, which
        // also wakes neighbors so their next step sees a fresh neighborhood) -- the pixel that
        // actually changed lives in exactly one chunk's sub-rect of the RGBA buffer/texture.
        let ci = (y / CHUNK) * self.chunks_x + (x / CHUNK);
        self.render_dirty[ci] = 1;
    }

    /// Set a tunable param by index (used by tests and the wasm bridge). Out-of-range indices are ignored.
    pub fn set_param(&mut self, index: u32, value: f32) {
        if (index as usize) < crate::params::P_COUNT {
            self.params.values[index as usize] = value;
        }
    }

    pub fn paint(&mut self, cx: i32, cy: i32, radius: i32, material: u8) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let (x, y) = ((cx + dx) as isize, (cy + dy) as isize);
                if !self.in_bounds(x, y) {
                    continue;
                }
                let shade = (self.next_rand() & 3) as u8;
                let i = self.idx(x as usize, y as usize);
                let mat = Material::from_u8(material);
                let mut cell = Cell::new(mat, shade);
                if mat == Material::Fire {
                    cell.aux = self.params.values[P_FIRE_LIFETIME].clamp(0.0, 255.0) as u8;
                }
                // The brush is material-agnostic and unconditionally overwrites whatever was
                // there (erase, paint over, etc.) -- same aux caveat as carve_crater/inject_blob:
                // only decrement if this Mycelium cell was never lit (aux still the colony id,
                // not a stale fuel countdown; a burning cell was already accounted at ignition).
                if Material::from_u8(self.cells[i].material) == Material::Mycelium
                    && self.cells[i].flags & FLAG_BURNING == 0
                {
                    let cid = self.cells[i].aux;
                    self.colony_cell_removed(cid);
                }
                self.cells[i] = cell;
                self.wake(x as usize, y as usize);
            }
        }
        // Painted Mycelium is inert terrain -- it only becomes a living, growing organism if a
        // colony's tip later happens to reach it, or via spawn_colony (see Ammo::Spore below).
    }

    pub fn spawn_avatar(&mut self, x: f32, y: f32) {
        self.avatar = Some(Avatar {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            w: 3,
            h: 6,
            on_ground: false,
            want_left: false,
            want_right: false,
            want_jump: false,
        });
    }

    pub fn set_avatar_input(&mut self, left: bool, right: bool, jump: bool) {
        if let Some(a) = self.avatar.as_mut() {
            a.want_left = left;
            a.want_right = right;
            a.want_jump = jump;
        }
    }

    pub fn avatar_xywh(&self) -> Option<[f32; 4]> {
        self.avatar.map(|a| [a.x, a.y, a.w as f32, a.h as f32])
    }

    pub fn avatar_center(&self) -> Option<[f32; 2]> {
        self.avatar.map(|a| [a.x + a.w as f32 / 2.0, a.y + a.h as f32 / 2.0])
    }

    /// Would the AABB with top-left (ax, ay) overlap any blocking cell?
    ///
    /// The avatar's position is fractional almost every frame (walk speed is ±1.4px,
    /// gravity accumulates in 0.3px steps), so the continuous AABB [ax, ax+w) x [ay, ay+h)
    /// can extend into the cell just past `floor(ax) + w` (and `floor(ay) + h`). The cell
    /// range must cover every cell the AABB overlaps, i.e. up to `ceil(ax + w)` exclusive,
    /// not just `floor(ax) + w` exclusive — otherwise the trailing fractional cell is never
    /// tested and the avatar can embed into terrain on its leading/trailing edge.
    fn avatar_blocked(&self, ax: f32, ay: f32, w: i32, h: i32) -> bool {
        let x0 = ax.floor() as isize;
        let y0 = ay.floor() as isize;
        let x1 = (ax + w as f32).ceil() as isize;
        let y1 = (ay + h as f32).ceil() as isize;
        for y in y0..y1 {
            for x in x0..x1 {
                let m = self.material_at(x, y);
                if m.is_solid() || m.is_powder() {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn update_avatar(&mut self) {
        let Some(mut a) = self.avatar.take() else { return };
        // horizontal intent
        a.vx = if a.want_left == a.want_right {
            0.0
        } else if a.want_right {
            AVATAR_WALK
        } else {
            -AVATAR_WALK
        };
        // jump
        if a.want_jump && a.on_ground {
            a.vy = -AVATAR_JUMP;
            a.on_ground = false;
        }
        // gravity
        a.vy = (a.vy + AVATAR_GRAVITY).min(AVATAR_MAX_FALL);

        // move X one pixel at a time
        let mut moved = a.vx;
        while moved.abs() >= 0.001 {
            let step = moved.clamp(-1.0, 1.0);
            if self.avatar_blocked(a.x + step, a.y, a.w, a.h) {
                a.vx = 0.0;
                break;
            }
            a.x += step;
            moved -= step;
        }

        // move Y one pixel at a time
        a.on_ground = false;
        let mut moved = a.vy;
        while moved.abs() >= 0.001 {
            let step = moved.clamp(-1.0, 1.0);
            if self.avatar_blocked(a.x, a.y + step, a.w, a.h) {
                if step > 0.0 {
                    a.on_ground = true;
                }
                a.vy = 0.0;
                break;
            }
            a.y += step;
            moved -= step;
        }

        // keep in-world horizontally; if it falls out the bottom, leave it (dead-ish) at the edge
        let maxx = (self.width as i32 - a.w) as f32;
        a.x = a.x.clamp(0.0, maxx.max(0.0));
        self.avatar = Some(a);
    }

    #[inline]
    pub(crate) fn frame_u8(&self) -> u8 {
        (self.frame & 0xFF) as u8
    }

    /// Swap two cells (either may be Empty), stamp both as moved this frame, wake both chunks.
    pub(crate) fn swap_cells(&mut self, ax: usize, ay: usize, bx: usize, by: usize) {
        let (ai, bi) = (self.idx(ax, ay), self.idx(bx, by));
        self.cells.swap(ai, bi);
        let f = self.frame_u8();
        self.stamp[ai] = f;
        self.stamp[bi] = f;
        self.wake(ax, ay);
        self.wake(bx, by);
    }

    pub fn step(&mut self) {
        self.frame += 1;
        std::mem::swap(&mut self.active, &mut self.active_next);
        self.active_next.iter_mut().for_each(|b| *b = 0);
        self.cells_processed = 0;
        let ltr = self.frame % 2 == 0; // alternate sweep direction to avoid horizontal bias

        // Bottom-up: destination rows are processed before their sources.
        // Only sweep x-spans of chunks that were woken last step.
        for y in (0..self.height).rev() {
            let crow = (y / CHUNK) * self.chunks_x;
            for c_raw in 0..self.chunks_x {
                let cx = if ltr { c_raw } else { self.chunks_x - 1 - c_raw };
                if self.active[crow + cx] == 0 {
                    continue;
                }
                let x0 = cx * CHUNK;
                for i in 0..CHUNK {
                    let x = if ltr { x0 + i } else { x0 + CHUNK - 1 - i };
                    self.update_cell(x, y);
                }
            }
        }
        // Budgeted mycelium growth on the P_MY_GROWTH_INTERVAL cadence -- the sole growth model
        // (chunk-sleep safe: grow_mycelium() no-ops when there's nothing live to grow/decay).
        if self.my_grow_countdown == 0 {
            self.grow_mycelium();
            self.my_grow_countdown = (self.params.values[crate::params::P_MY_GROWTH_INTERVAL] as u32).max(1);
        }
        self.my_grow_countdown = self.my_grow_countdown.saturating_sub(1);
        self.update_projectiles(); // carve_crater/inject_blob (ammo impacts) enqueue drop checks here
        // Task 3 (M1e cleanup): one coalesced, deduped support-check pass per step, after every
        // site that can enqueue one this frame has run (the cell sweep's burnout/acid-dissolve
        // above, and ammo impacts via update_projectiles just above). No-ops when nothing was
        // queued, so it costs nothing on a settled/sleeping world (chunk-sleep safe).
        self.drop_unsupported_pending();
        self.update_particles();
        self.update_avatar();
    }

    pub fn fire(&mut self, x: f32, y: f32, vx: f32, vy: f32, ammo: u8) {
        // Validate velocity: reject if non-finite or both zero
        if !vx.is_finite() || !vy.is_finite() || (vx == 0.0 && vy == 0.0) {
            return; // silently ignore invalid projectiles
        }

        // Clamp velocity components to prevent infinite substep loops
        let vx_clamped = vx.clamp(-PROJECTILE_MAX_SPEED, PROJECTILE_MAX_SPEED);
        let vy_clamped = vy.clamp(-PROJECTILE_MAX_SPEED, PROJECTILE_MAX_SPEED);

        self.projectiles.push(Projectile {
            x,
            y,
            vx: vx_clamped,
            vy: vy_clamped,
            ammo: Ammo::from_u8(ammo),
            alive: true,
        });
    }

    pub fn projectile_count(&self) -> usize {
        self.projectiles.len()
    }

    /// Flat [x0,y0,x1,y1,...] world coords of live projectiles, for rendering.
    pub fn projectiles_xy(&self) -> Vec<f32> {
        let mut v = Vec::with_capacity(self.projectiles.len() * 2);
        for p in &self.projectiles {
            v.push(p.x);
            v.push(p.y);
        }
        v
    }

    pub(crate) fn update_projectiles(&mut self) {
        if self.projectiles.is_empty() {
            return;
        }
        let existing = std::mem::take(&mut self.projectiles);
        let mut survivors = Vec::with_capacity(existing.len());
        for mut p in existing {
            // Clamp velocity to keep the ray-march bounded and prevent NaN propagation
            p.vx = p.vx.clamp(-PROJECTILE_MAX_SPEED, PROJECTILE_MAX_SPEED);
            p.vy = p.vy.clamp(-PROJECTILE_MAX_SPEED, PROJECTILE_MAX_SPEED);
            let steps = p.vx.abs().max(p.vy.abs()).ceil().max(1.0) as i32;
            let (sx, sy) = (p.vx / steps as f32, p.vy / steps as f32);
            for _ in 0..steps {
                let nx = p.x + sx;
                let ny = p.y + sy;
                let (cx, cy) = (nx.floor() as isize, ny.floor() as isize);
                if !self.in_bounds(cx, cy) {
                    p.alive = false;
                    break;
                }
                let m = self.material_at(cx, cy);
                if m.is_projectile_passable() {
                    p.x = nx;
                    p.y = ny;
                } else {
                    self.on_impact(cx, cy, p.ammo);
                    p.alive = false;
                    break;
                }
            }
            if p.alive {
                survivors.push(p);
            }
        }
        self.projectiles = survivors;
    }

    fn on_impact(&mut self, cx: isize, cy: isize, ammo: Ammo) {
        match ammo {
            Ammo::Kinetic => {
                let r = self.params.values[P_KINETIC_RADIUS] as isize;
                let ej = self.params.values[P_KINETIC_EJECTA];
                self.carve_crater(cx, cy, r, ej);
            }
            Ammo::Incendiary => {
                let r = self.params.values[P_INCENDIARY_RADIUS] as isize;
                self.carve_crater(cx, cy, (r - 1).max(1), 0.15);
                self.ignite_blast(cx, cy, r);
            }
            Ammo::Acid => {
                let r = self.params.values[P_ACID_BLOB_RADIUS] as isize;
                self.inject_blob(cx, cy, r, Material::Acid);
            }
            Ammo::Spore => {
                // Spore ammo is "the builder": it plants a living colony (spawn_colony lays the
                // origin mycelium cell + one tip), rather than just painting inert mycelium.
                //
                // M1e cleanup task 2: two things can make a bare `spawn_colony(cx, cy)` wrong
                // here. (1) The concurrent-colony cap (P_MY_MAX_COLONIES / the 255 aux-id hard
                // cap) means spawn_colony can return 0 ("no colony") -- that must be a quiet
                // no-op, never a tip/cell referencing nonexistent colony 0. (2) A round fired
                // into open air would otherwise plant a floating stub with no substrate under
                // it, so search the impact's neighborhood for a Soil cell (or an Empty cell
                // adjacent to Soil) and plant there instead; if nothing qualifies within the
                // search radius, skip entirely (no colony, no puff) rather than float one in air.
                let r = self.params.values[P_SPORE_BLOB_RADIUS] as isize;
                if let Some((sx, sy)) = self.find_spore_landing_site(cx, cy, SPORE_LANDING_SEARCH_RADIUS) {
                    if self.spawn_colony(sx, sy) != 0 {
                        self.inject_blob(cx, cy - r, (r / 2).max(1), Material::SporeGas); // a puff above
                    }
                }
            }
        }
    }

    /// Find a landing site for Spore ammo near an impact point: a Soil cell (plant directly on
    /// substrate) or an Empty cell adjacent to Soil (plant right next to it), searched in
    /// concentric square rings out to `radius` so the closest qualifying cell wins. None if
    /// nothing qualifies anywhere in the neighborhood (e.g. deep open air, no soil nearby) --
    /// callers should skip planting entirely in that case rather than float a colony in air.
    /// Deterministic fixed scan order (row-major within each ring); no RNG.
    fn find_spore_landing_site(&self, cx: isize, cy: isize, radius: isize) -> Option<(usize, usize)> {
        for r in 0..=radius {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r { continue; } // only this ring's perimeter
                    let (x, y) = (cx + dx, cy + dy);
                    if !self.in_bounds(x, y) { continue; }
                    let m = self.material_at(x, y);
                    if m == Material::Soil || (m == Material::Empty && self.adjacent_to_soil(x, y)) {
                        return Some((x as usize, y as usize));
                    }
                }
            }
        }
        None
    }

    /// True if any of the 8 neighbors of (x,y) is Soil.
    fn adjacent_to_soil(&self, x: isize, y: isize) -> bool {
        const D: [(isize, isize); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
        D.iter().any(|&(dx, dy)| self.material_at(x + dx, y + dy) == Material::Soil)
    }

    fn carve_crater(&mut self, cx: isize, cy: isize, radius: isize, ejecta_frac: f32) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if !self.in_bounds(x, y) {
                    continue;
                }
                let m = self.material_at(x, y);
                if m == Material::Empty || m == Material::Rock {
                    continue; // rock resists kinetic rounds (keeps caves stable)
                }
                let (ux, uy) = (x as usize, y as usize);
                let i = self.idx(ux, uy);
                // throw a fraction of powders/solids outward as debris; clear the rest
                if (m.is_powder() || m == Material::Mycelium || m == Material::MushroomFlesh)
                    && self.chance(ejecta_frac)
                {
                    let ang = (self.next_rand() & 255) as f32 / 255.0 * std::f32::consts::TAU;
                    let spd = 2.0 + (self.next_rand() & 63) as f32 / 32.0;
                    let mat = self.cells[i].material;
                    self.spawn_particle(x as f32 + 0.5, y as f32 + 0.5, ang.cos() * spd, ang.sin() * spd - 1.0, mat);
                }
                if m == Material::MushroomFlesh && self.chance(self.params.values[P_GUNFIRE_SPORE_CHANCE])
                {
                    let shade = (self.next_rand() & 3) as u8;
                    self.cells[i] = Cell::new(Material::SporeGas, shade);
                    self.wake(ux, uy);
                    continue; // leave spore gas instead of empty this cell; flags cleared so a
                              // carved cell that was mid-burn (flesh ignited by a spreading fire)
                              // doesn't inherit FLAG_BURNING with aux=0 and phantom-detonate next tick
                }
                // A carved Mycelium cell's aux is only trustworthy as a colony id if it was never
                // lit -- once burning, aux is the fuel countdown (see ignite_blast/
                // ignite_neighbors) and colony_cell_removed was already called at ignition time.
                if m == Material::Mycelium && self.cells[i].flags & FLAG_BURNING == 0 {
                    let cid = self.cells[i].aux;
                    self.colony_cell_removed(cid);
                }
                self.cells[i] = Cell::default();
                self.wake(ux, uy);
            }
        }
        // Task 5: cells just carved away may have been the only link some Mycelium/MushroomFlesh
        // nearby had to an anchor. Check a bit past the crater's own radius so the flood's seed
        // search reaches the surviving cells right at the edge of what was just removed.
        // Task 3 (M1e cleanup): enqueue rather than flood inline -- drop_unsupported_pending()
        // runs once after update_projectiles() later this same step, so the outcome (and its
        // single-step timing, per the tests) is unchanged; only the raw call is deferred/deduped.
        self.pending_drop_checks.push((cx, cy, radius + 2));
    }

    fn ignite_blast(&mut self, cx: isize, cy: isize, radius: isize) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if !self.in_bounds(x, y) {
                    continue;
                }
                let (ux, uy) = (x as usize, y as usize);
                let i = self.idx(ux, uy);
                let m = Material::from_u8(self.cells[i].material);
                if self.params.flammability(m) > 0.0 {
                    // Ignition is the last moment a Mycelium cell's aux is still its colony id --
                    // once FLAG_BURNING is set, aux becomes the fuel countdown for the rest of
                    // this cell's life (burnout, or any carve/acid removal while still burning),
                    // so the colony's cell_count must be decremented HERE, not at burnout. Only
                    // do this the first time a cell catches fire (an already-burning cell hit by
                    // another blast has already been accounted for; its current aux is fuel, not
                    // a colony id, and must not be misread as one).
                    if m == Material::Mycelium && self.cells[i].flags & FLAG_BURNING == 0 {
                        let cid = self.cells[i].aux;
                        self.colony_cell_removed(cid);
                    }
                    self.cells[i].flags |= FLAG_BURNING;
                    // Deliberately refuel already-burning cells (owner ruling 2026-07-04):
                    // incendiary blasts refresh fires — this differs from ignite_neighbors,
                    // which skips already-burning cells to spread fire one cell per frame.
                    self.cells[i].aux = self.params.fuel(m);
                    self.stamp[i] = self.frame_u8();
                    self.wake(ux, uy);
                } else if m == Material::Empty && self.chance(0.3) {
                    self.cells[i] = Cell::new(Material::Fire, (self.next_rand() & 3) as u8);
                    self.stamp[i] = self.frame_u8();
                    self.wake(ux, uy);
                }
            }
        }
    }

    fn inject_blob(&mut self, cx: isize, cy: isize, radius: isize, material: Material) {
        let mut overwrote_mycelium_or_flesh = false;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if !self.in_bounds(x, y) {
                    continue;
                }
                let (ux, uy) = (x as usize, y as usize);
                let i = self.idx(ux, uy);
                let dst = Material::from_u8(self.cells[i].material);
                // fill empty; acid/spore also eat into soft organics/soil, not rock
                let soft = matches!(dst, Material::Soil | Material::Sand | Material::Mycelium);
                if dst == Material::Empty || soft {
                    if matches!(dst, Material::Mycelium | Material::MushroomFlesh) {
                        overwrote_mycelium_or_flesh = true;
                    }
                    // Same aux caveat as carve_crater: only decrement if this Mycelium cell was
                    // never lit (aux still the colony id, not a stale fuel countdown).
                    if dst == Material::Mycelium && self.cells[i].flags & FLAG_BURNING == 0 {
                        let cid = self.cells[i].aux;
                        self.colony_cell_removed(cid);
                    }
                    self.cells[i] = Cell::new(material, (self.next_rand() & 3) as u8);
                    self.wake(ux, uy);
                }
            }
        }
        // Task 5 (M1e final review): a blob (Acid ammo) can overwrite Mycelium/MushroomFlesh just
        // like carve_crater carves it away -- run the same support check, same radius convention
        // (blob radius + 2 so the flood's seed search reaches the surviving cells at the edge).
        // Task 3 (M1e cleanup): enqueue rather than flood inline; see carve_crater's comment.
        if overwrote_mycelium_or_flesh {
            self.pending_drop_checks.push((cx, cy, radius + 2));
        }
    }

    pub fn spawn_particle(&mut self, x: f32, y: f32, vx: f32, vy: f32, material: u8) {
        self.particles.push(Particle { x, y, vx, vy, material });
    }

    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    /// A cell a particle can fly through without settling.
    fn particle_passable(&self, x: isize, y: isize) -> bool {
        let m = self.material_at(x, y);
        m == Material::Empty || m.is_gas()
    }

    pub(crate) fn update_particles(&mut self) {
        if self.particles.is_empty() {
            return;
        }
        let mut survivors: Vec<Particle> = Vec::with_capacity(self.particles.len());
        let existing = std::mem::take(&mut self.particles);
        for mut p in existing {
            p.vy += PARTICLE_GRAVITY;
            // clamp speed to keep the ray-march bounded and avoid tunneling
            p.vx = p.vx.clamp(-PARTICLE_MAX_SPEED, PARTICLE_MAX_SPEED);
            p.vy = p.vy.clamp(-PARTICLE_MAX_SPEED, PARTICLE_MAX_SPEED);
            let steps = p.vx.abs().max(p.vy.abs()).ceil().max(1.0) as i32;
            let (sx, sy) = (p.vx / steps as f32, p.vy / steps as f32);
            let mut settled = false;
            let mut last_x = p.x;
            let mut last_y = p.y;
            for _ in 0..steps {
                let nx = p.x + sx;
                let ny = p.y + sy;
                let (cx, cy) = (nx.floor() as isize, ny.floor() as isize);
                if !self.in_bounds(cx, cy) {
                    // left the world: if it went out the sides/top/bottom, drop it
                    settled = true; // "settled" here means "remove from list"
                    last_x = f32::NAN; // sentinel: don't write to grid
                    break;
                }
                if self.particle_passable(cx, cy) {
                    p.x = nx;
                    p.y = ny;
                    last_x = nx;
                    last_y = ny;
                } else {
                    // blocked: resettle into the last passable cell we occupied
                    settled = true;
                    break;
                }
            }
            if settled {
                if last_x.is_finite() {
                    let (cx, cy) = (last_x.floor() as isize, last_y.floor() as isize);
                    if self.in_bounds(cx, cy) && self.material_at(cx, cy) == Material::Empty {
                        let (ux, uy) = (cx as usize, cy as usize);
                        let shade = (self.next_rand() & 3) as u8;
                        let i = self.idx(ux, uy);
                        self.cells[i] = Cell::new(Material::from_u8(p.material), shade);
                        self.wake(ux, uy);
                    }
                    // if the last cell isn't empty (rare — landed on a gas that filled), drop silently
                }
            } else {
                survivors.push(p);
            }
        }
        self.particles = survivors;
    }

    fn update_cell(&mut self, x: usize, y: usize) {
        let i = self.idx(x, y);
        if self.stamp[i] == self.frame_u8() {
            return; // already moved this frame
        }
        let cell = self.cells[i];
        let mat = Material::from_u8(cell.material);
        if mat == Material::Empty {
            return;
        }
        if cell.flags & FLAG_BURNING != 0 {
            self.cells_processed += 1;
            self.update_burning(x, y);
            return;
        }
        match mat {
            Material::Fire => {
                self.cells_processed += 1;
                self.update_fire(x, y);
            }
            Material::Acid => {
                self.cells_processed += 1;
                self.update_acid(x, y);
            }
            m if m.is_gas() => {
                self.cells_processed += 1;
                self.update_gas(x, y, m);
            }
            m if m.is_powder() => {
                self.cells_processed += 1;
                self.update_powder(x, y);
            }
            m if m.is_liquid() => {
                self.cells_processed += 1;
                self.update_liquid(x, y, m);
            }
            _ => {} // static solids (Rock, Mycelium, MushroomFlesh) cost nothing
        }
    }

    fn update_powder(&mut self, x: usize, y: usize) {
        let (xi, yi) = (x as isize, y as isize);
        let first_dx = if self.next_rand() & 1 == 0 { -1 } else { 1 };
        let candidates = [(xi, yi + 1), (xi + first_dx, yi + 1), (xi - first_dx, yi + 1)];
        for (nx, ny) in candidates {
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let ni = self.idx(nx as usize, ny as usize);
            let dst = Material::from_u8(self.cells[ni].material);
            // Sink into empty (cascades freely — empty isn't a conserved particle) or displace
            // a liquid, but never a liquid that already moved this frame: that would ride the
            // bottom-up sweep and teleport one liquid cell up the whole column in a single tick.
            if dst == Material::Empty || (dst.is_liquid() && self.stamp[ni] != self.frame_u8()) {
                self.swap_cells(x, y, nx as usize, ny as usize);
                return;
            }
        }
    }

    fn update_liquid(&mut self, x: usize, y: usize, mat: Material) {
        let (xi, yi) = (x as isize, y as isize);
        let my_density = mat.density();

        // Fall / diagonal-fall into empty or a lighter liquid (displacement swap).
        let first_dx = if self.next_rand() & 1 == 0 { -1 } else { 1 };
        let falls = [(xi, yi + 1), (xi + first_dx, yi + 1), (xi - first_dx, yi + 1)];
        for (nx, ny) in falls {
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let ni = self.idx(nx as usize, ny as usize);
            let dst = Material::from_u8(self.cells[ni].material);
            // As in update_powder: fall into empty freely, but only displace a lighter liquid
            // that hasn't already moved this frame, so displacement propagates one cell per
            // frame instead of teleporting a liquid cell up the sweep.
            if dst == Material::Empty
                || (dst.is_liquid() && dst.density() < my_density && self.stamp[ni] != self.frame_u8())
            {
                self.swap_cells(x, y, nx as usize, ny as usize);
                return;
            }
        }

        // Rest-seeking dispersion: slide only toward an actual drop within DISPERSION —
        // the FIRST scanned empty cell whose below-neighbor is Empty or a strictly
        // lighter liquid. No drop in either direction means the liquid RESTS (no swap,
        // no wake, its chunk sleeps). The world floor and solids are not drops.
        let first_dir: isize = if self.next_rand() & 1 == 0 { 1 } else { -1 };
        for dir in [first_dir, -first_dir] {
            let mut nx = xi;
            for _ in 0..DISPERSION {
                nx += dir;
                if !self.in_bounds(nx, yi) {
                    break;
                }
                let scanned = Material::from_u8(self.cells[self.idx(nx as usize, y)].material);
                if scanned != Material::Empty {
                    break; // anything non-empty blocks the slide
                }
                let is_drop = if self.in_bounds(nx, yi + 1) {
                    let b = Material::from_u8(
                        self.cells[self.idx(nx as usize, (yi + 1) as usize)].material,
                    );
                    b == Material::Empty || (b.is_liquid() && b.density() < my_density)
                } else {
                    false
                };
                if is_drop {
                    self.swap_cells(x, y, nx as usize, y);
                    return;
                }
            }
        }
    }

    fn update_burning(&mut self, x: usize, y: usize) {
        let i = self.idx(x, y);
        let mat = Material::from_u8(self.cells[i].material);
        let (xi, yi) = (x as isize, y as isize);
        // water above or beside extinguishes; water BELOW does not, so an oil
        // slick floating on a pool keeps burning
        for (nx, ny) in [(xi, yi - 1), (xi + 1, yi), (xi - 1, yi)] {
            if self.material_at(nx, ny) == Material::Water {
                // Note: this doesn't remove the cell (material stays Mycelium) so it's not a
                // colony_cell_removed site by definition -- and aux here is already the fuel
                // countdown, not the colony id (already accounted for at ignition), so there is
                // nothing left to read anyway. A cell extinguished this way keeps occupying the
                // grid as an orphan Mycelium husk (aux=0) until some later removal path clears it.
                self.cells[i].flags &= !FLAG_BURNING;
                self.cells[i].aux = 0;
                self.wake(x, y);
                return;
            }
        }
        if self.cells[i].aux == 0 {
            // fuel spent: burn to the product material
            // Note: no colony_cell_removed call here for a Mycelium cell -- once a cell starts
            // burning, aux becomes the fuel countdown (see ignite_blast/ignite_neighbors below),
            // never the colony id, so its colony's cell_count was already decremented back at
            // ignition time, not here (aux is always 0 by the time this branch runs anyway).
            let product = match mat {
                // Only sometimes leaves Ash behind -- a burnt mushroom colony should mostly
                // disappear, not blanket the ground 1:1 in ash.
                Material::Mycelium | Material::MushroomFlesh => {
                    if self.chance(self.params.values[P_ASH_CHANCE]) {
                        Material::Ash
                    } else {
                        Material::Empty
                    }
                }
                Material::SporeGas => Material::Fire, // the detonation flash
                _ => Material::Empty,
            };
            let shade = (self.next_rand() & 3) as u8;
            let mut product_cell = Cell::new(product, shade);
            if product == Material::Fire {
                product_cell.aux = self.params.values[P_FIRE_LIFETIME].clamp(0.0, 255.0) as u8;
            }
            self.cells[i] = product_cell;
            self.stamp[i] = self.frame_u8();
            self.wake(x, y);
            // Task 5: a Mycelium/MushroomFlesh cell just burned away -- its neighbors may have
            // lost their only link to an anchor. Radius is small (a single cell was removed);
            // it only needs to reach the 8 surrounding cells to seed the flood.
            // Task 3 (M1e cleanup): this fires once per burned cell during the sweep, so a big
            // fire eating a big mycelium mass would otherwise re-flood overlapping regions many
            // times a frame -- enqueue instead and let drop_unsupported_pending() coalesce all of
            // this step's removals (burn + acid + ammo) into one deduped pass after the sweep.
            if matches!(mat, Material::Mycelium | Material::MushroomFlesh) {
                self.pending_drop_checks.push((x as isize, y as isize, 2));
            }
            return;
        }
        self.cells[i].aux -= 1;
        self.wake(x, y); // burning cells stay hot until spent
        self.ignite_neighbors(x, y);
        self.emit_smoke_above(x, y);
        if mat.is_liquid() {
            self.update_liquid(x, y, mat); // burning oil keeps flowing
        }
    }

    fn update_fire(&mut self, x: usize, y: usize) {
        let i = self.idx(x, y);
        if self.cells[i].aux == 0 {
            self.cells[i] = Cell::default();
            self.wake(x, y);
            return;
        }
        self.cells[i].aux -= 1;
        self.wake(x, y);
        self.ignite_neighbors(x, y);
        self.emit_smoke_above(x, y);
        // flicker upward into empty space
        if self.chance(self.params.values[P_FIRE_FLICKER]) {
            let (xi, yi) = (x as isize, y as isize);
            if self.material_at(xi, yi - 1) == Material::Empty {
                self.swap_cells(x, y, x, y - 1);
            }
        }
    }

    fn ignite_neighbors(&mut self, x: usize, y: usize) {
        let (xi, yi) = (x as isize, y as isize);
        for (nx, ny) in [(xi, yi - 1), (xi + 1, yi), (xi, yi + 1), (xi - 1, yi)] {
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let ni = self.idx(nx as usize, ny as usize);
            if self.cells[ni].flags & FLAG_BURNING != 0 {
                continue;
            }
            let nmat = Material::from_u8(self.cells[ni].material);
            let p = self.params.flammability(nmat);
            if p > 0.0 && self.chance(p) {
                // See ignite_blast's comment: this is the last point aux is still the colony id
                // for a Mycelium cell (the `continue` above already guarantees it wasn't burning).
                if nmat == Material::Mycelium {
                    let cid = self.cells[ni].aux;
                    self.colony_cell_removed(cid);
                }
                self.cells[ni].flags |= FLAG_BURNING;
                self.cells[ni].aux = self.params.fuel(nmat);
                // Stamp as acted-this-frame so the bottom-up sweep doesn't process this
                // freshly-lit cell again this tick — otherwise fire chains across a whole
                // connected region in one frame instead of spreading one cell per frame.
                self.stamp[ni] = self.frame_u8();
                self.wake(nx as usize, ny as usize);
            }
        }
    }

    fn emit_smoke_above(&mut self, x: usize, y: usize) {
        if y == 0 {
            return;
        }
        let above = self.idx(x, y - 1);
        if Material::from_u8(self.cells[above].material) == Material::Empty
            && self.chance(self.params.values[P_SMOKE_EMIT])
        {
            let shade = (self.next_rand() & 3) as u8;
            let mut c = Cell::new(Material::Smoke, shade);
            c.aux = self.params.values[P_SMOKE_LIFETIME].clamp(0.0, 255.0) as u8;
            self.cells[above] = c;
            self.stamp[above] = self.frame_u8();
            self.wake(x, y - 1);
        }
    }

    fn acid_etch_chance(&self, m: Material) -> f32 {
        match m {
            Material::Empty
            | Material::Acid
            | Material::Fire
            | Material::Smoke
            | Material::SporeGas
            | Material::Water => 0.0,
            Material::Rock => self.params.values[P_ACID_ETCH_ROCK],
            _ => self.params.values[P_ACID_ETCH],
        }
    }

    fn update_acid(&mut self, x: usize, y: usize) {
        let (xi, yi) = (x as isize, y as isize);
        let dirs = [(xi, yi + 1), (xi - 1, yi), (xi + 1, yi), (xi, yi - 1)];
        // stay awake while anything etchable is adjacent — otherwise a run of missed
        // rolls could let the chunk sleep mid-meal. Charges still bound total lifetime.
        if dirs
            .iter()
            .any(|&(nx, ny)| self.in_bounds(nx, ny) && self.acid_etch_chance(self.material_at(nx, ny)) > 0.0)
        {
            self.wake(x, y);
        }
        // try to dissolve one random 4-neighbor
        let (nx, ny) = dirs[(self.next_rand() % 4) as usize];
        if self.in_bounds(nx, ny) {
            let target = self.material_at(nx, ny);
            let p = self.acid_etch_chance(target);
            if p > 0.0 && self.chance(p) {
                let ni = self.idx(nx as usize, ny as usize);
                // Same aux caveat as carve_crater/inject_blob: only decrement if this Mycelium
                // cell was never lit (aux still the colony id, not a stale fuel countdown).
                if target == Material::Mycelium && self.cells[ni].flags & FLAG_BURNING == 0 {
                    let cid = self.cells[ni].aux;
                    self.colony_cell_removed(cid);
                }
                self.cells[ni] = Cell::default();
                self.stamp[ni] = self.frame_u8();
                self.wake(nx as usize, ny as usize);
                // Task 5 (M1e final review): dissolving a Mycelium/MushroomFlesh cell directly,
                // or a Soil cell that was anchoring nearby mycelium, can strand a connected mass
                // just like a burned-away cell does -- same check, same small radius since only
                // one cell was removed.
                // Task 3 (M1e cleanup): enqueue rather than flood inline; see update_burning's
                // comment -- a big acid pool dissolves many cells per frame, so this must not
                // flood inline per cell.
                if matches!(target, Material::Mycelium | Material::MushroomFlesh | Material::Soil) {
                    self.pending_drop_checks.push((nx, ny, 2));
                }
                let i = self.idx(x, y);
                if self.cells[i].aux <= 1 {
                    self.cells[i] = Cell::default(); // spent
                } else {
                    self.cells[i].aux -= 1;
                }
                self.wake(x, y);
                return;
            }
        }
        self.update_liquid(x, y, Material::Acid);
    }

    fn update_gas(&mut self, x: usize, y: usize, mat: Material) {
        let i = self.idx(x, y);
        if mat == Material::Smoke {
            if self.cells[i].aux == 0 {
                self.cells[i] = Cell::default();
                self.wake(x, y);
                return;
            }
            self.cells[i].aux -= 1;
            self.wake(x, y); // fading smoke keeps its chunk lit until it dies
        }
        let (xi, yi) = (x as isize, y as isize);
        // rise straight, then random diagonal, into empty
        let first_dx = if self.next_rand() & 1 == 0 { -1 } else { 1 };
        for (nx, ny) in [(xi, yi - 1), (xi + first_dx, yi - 1), (xi - first_dx, yi - 1)] {
            if self.in_bounds(nx, ny) && self.material_at(nx, ny) == Material::Empty {
                self.swap_cells(x, y, nx as usize, ny as usize);
                return;
            }
        }
        // rest-seeking sideways: slide only toward a cell it could rise from
        let first_dir: isize = if self.next_rand() & 1 == 0 { 1 } else { -1 };
        for dir in [first_dir, -first_dir] {
            let mut nx = xi;
            for _ in 0..DISPERSION {
                nx += dir;
                if !self.in_bounds(nx, yi) || self.material_at(nx, yi) != Material::Empty {
                    break;
                }
                if self.material_at(nx, yi - 1) == Material::Empty {
                    self.swap_cells(x, y, nx as usize, y);
                    return;
                }
            }
        }
    }

    /// Number of cells currently flagged as burning (test/debug instrument).
    pub fn burning_count(&self) -> usize {
        self.cells.iter().filter(|c| c.flags & FLAG_BURNING != 0).count()
    }

    pub fn active_ptr(&self) -> *const u8 {
        self.active.as_ptr()
    }
    pub fn active_len(&self) -> usize {
        self.active.len()
    }
    pub fn chunks_x(&self) -> usize {
        self.chunks_x
    }
    pub fn chunks_y(&self) -> usize {
        self.chunks_y
    }

    /// Rewrite the RGBA buffer, but ONLY for chunks flagged in `render_dirty` (M1d task 3) --
    /// a settled, un-woken world touches nothing here. The buffer is persistent across frames
    /// (untouched chunks keep last frame's bytes), which is exactly why entities can no longer
    /// be stamped in here: a moving particle/projectile/avatar would leave a trail in whatever
    /// chunk it vacates, since that chunk is never marked dirty by the entity's movement alone.
    /// Entities are drawn fresh every frame instead, as a JS-side overlay pass over this texture.
    pub fn render_rgba(&mut self) {
        for cy in 0..self.chunks_y {
            for cx in 0..self.chunks_x {
                if self.render_dirty[cy * self.chunks_x + cx] == 0 {
                    continue;
                }
                let (x0, y0) = (cx * CHUNK, cy * CHUNK);
                for y in y0..(y0 + CHUNK) {
                    for x in x0..(x0 + CHUNK) {
                        let i = self.idx(x, y);
                        let cell = self.cells[i];
                        let burning = cell.flags & FLAG_BURNING != 0;
                        let mat = Material::from_u8(cell.material);
                        let (base, j) = if burning || mat == Material::Fire {
                            // animate between deep orange and yellow using the countdown for flicker
                            let hot = (cell.aux & 7) as i16 * 10;
                            ([255u8, (150 + hot.min(70)) as u8, 40u8], 0i16)
                        } else {
                            let j = if mat == Material::Empty { 0 } else { (cell.shade & 3) as i16 * 6 - 9 };
                            (mat.base_color(), j)
                        };
                        let o = i * 4;
                        self.rgba[o] = (base[0] as i16 + j).clamp(0, 255) as u8;
                        self.rgba[o + 1] = (base[1] as i16 + j).clamp(0, 255) as u8;
                        self.rgba[o + 2] = (base[2] as i16 + j).clamp(0, 255) as u8;
                        self.rgba[o + 3] = 255;
                    }
                }
            }
        }
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn rgba_ptr(&self) -> *const u8 {
        self.rgba.as_ptr()
    }

    /// Per-chunk render-dirty bitmap (test accessor; the wasm bridge uses render_dirty_ptr/len
    /// for a zero-copy view instead).
    pub fn render_dirty(&self) -> &[u8] {
        &self.render_dirty
    }

    pub fn render_dirty_ptr(&self) -> *const u8 {
        self.render_dirty.as_ptr()
    }

    pub fn render_dirty_len(&self) -> usize {
        self.render_dirty.len()
    }

    /// Mark every chunk render-dirty (first frame, or after generate()/clear() replace the
    /// whole grid out from under the persistent GPU texture).
    pub fn mark_all_render_dirty(&mut self) {
        self.render_dirty.fill(1);
    }

    /// Clear the render-dirty bitmap; called by the renderer once it has uploaded every chunk
    /// this bitmap flagged.
    pub fn clear_render_dirty(&mut self) {
        self.render_dirty.fill(0);
    }

    /// Flat [x0,y0,m0, x1,y1,m1, ...] world coords + material (as f32) of live particles, for
    /// the JS overlay pass (particles are no longer stamped into the persistent RGBA texture --
    /// see render_rgba's doc comment).
    pub fn particles_xy(&self) -> Vec<f32> {
        let mut v = Vec::with_capacity(self.particles.len() * 3);
        for p in &self.particles {
            v.push(p.x);
            v.push(p.y);
            v.push(p.material as f32);
        }
        v
    }

    /// Test hook: overwrite a cell's material directly, bypassing `wake()` -- and therefore
    /// `render_dirty`. Every real mutation path goes through `wake()`, so this is the only way
    /// to construct "the material changed but its chunk was never marked render-dirty",
    /// which is what `render_rgba_only_touches_dirty_chunks` needs to prove the skip actually
    /// skips (a real paint would just re-dirty the very chunk we're trying to hold stale).
    #[doc(hidden)]
    pub fn test_set_material_no_wake(&mut self, x: usize, y: usize, material: u8) {
        let i = self.idx(x, y);
        self.cells[i] = Cell::new(Material::from_u8(material), 0);
    }
}
