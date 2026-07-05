use crate::cell::Material;
use crate::world::World;

#[derive(Clone, Copy)]
pub struct Colony {
    pub id: u8,
    pub nutrient_pool: u32,
    pub tip_count: u16,
    pub alive: bool,
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
        self.colonies.push(Colony { id, nutrient_pool: 0, tip_count: 1, alive: true });
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
    /// New growth entry point. Chunk-sleep safe: no-op when no live tips.
    pub fn grow_mycelium(&mut self) {
        if self.tips.iter().all(|t| !t.alive) { return; }
        let eat = self.params.values[crate::params::P_MY_EAT];
        for ti in 0..self.tips.len() {
            if !self.tips[ti].alive { continue; }
            self.extend_tip(ti, eat);
        }
        self.tips.retain(|t| t.alive); // drop dead tips so the loop stays cheap
        // Keep Colony.tip_count in sync with reality: it's set to 1 at spawn but never adjusted
        // as tips die (or, later, branch), so recompute it from the live tips whenever the set
        // of live tips changes. Cheap: bounded by colony/tip counts, and only runs on a growth
        // tick (already gated by P_MY_GROWTH_INTERVAL).
        for c in self.colonies.iter_mut() { c.tip_count = 0; }
        for t in self.tips.iter() {
            if let Some(c) = self.colonies.iter_mut().find(|c| c.id == t.colony) {
                c.tip_count += 1;
            }
        }
    }

    fn extend_tip(&mut self, ti: usize, eat: f32) {
        let t = self.tips[ti];
        let Some((nx, ny)) = self.pick_step(t) else { self.tips[ti].alive = false; return; };
        let dst = self.material_at(nx, ny);
        let (ux, uy) = (nx as usize, ny as usize);
        // eat if stepping into soil
        if dst == Material::Soil {
            let r = self.cell_aux(ux, uy) as f32;
            if let Some(c) = self.colonies.iter_mut().find(|c| c.id == t.colony) {
                c.nutrient_pool = c.nutrient_pool.saturating_add((r * eat) as u32);
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
