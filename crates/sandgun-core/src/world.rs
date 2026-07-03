use crate::cell::{Cell, Material, FLAG_BURNING};
use crate::params::{Params, P_ACID_ETCH, P_ACID_ETCH_ROCK, P_FIRE_FLICKER, P_SMOKE_EMIT, P_SMOKE_LIFETIME};

pub const CHUNK: usize = 64;
pub const DISPERSION: isize = 4;

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
    /// Cells visited by the last step(); test + debug hook for chunk skipping.
    pub cells_processed: u64,
    pub params: Params,
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
            cells_processed: 0,
            params: Params::default(),
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

    /// Reset every cell to Empty and clear movement stamps (used by worldgen).
    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
        self.stamp.fill(0);
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
                self.cells[i] = Cell::new(Material::from_u8(material), shade);
                self.wake(x as usize, y as usize);
            }
        }
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
            let dst = Material::from_u8(self.cells[self.idx(nx as usize, ny as usize)].material);
            if dst == Material::Empty || dst.is_liquid() {
                self.swap_cells(x, y, nx as usize, ny as usize); // sinks through liquid by displacement
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
            let dst = Material::from_u8(self.cells[self.idx(nx as usize, ny as usize)].material);
            if dst == Material::Empty || (dst.is_liquid() && dst.density() < my_density) {
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
                self.cells[i].flags &= !FLAG_BURNING;
                self.cells[i].aux = 0;
                self.wake(x, y);
                return;
            }
        }
        if self.cells[i].aux == 0 {
            // fuel spent: burn to the product material
            let product = match mat {
                Material::Mycelium | Material::MushroomFlesh => Material::Ash,
                Material::SporeGas => Material::Fire, // the detonation flash
                _ => Material::Empty,
            };
            let shade = (self.next_rand() & 3) as u8;
            self.cells[i] = Cell::new(product, shade);
            self.stamp[i] = self.frame_u8();
            self.wake(x, y);
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
                self.cells[ni].flags |= FLAG_BURNING;
                self.cells[ni].aux = self.params.fuel(nmat);
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
            let p = self.acid_etch_chance(self.material_at(nx, ny));
            if p > 0.0 && self.chance(p) {
                let ni = self.idx(nx as usize, ny as usize);
                self.cells[ni] = Cell::default();
                self.stamp[ni] = self.frame_u8();
                self.wake(nx as usize, ny as usize);
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

    pub fn render_rgba(&mut self) {
        for (i, cell) in self.cells.iter().enumerate() {
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

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn rgba_ptr(&self) -> *const u8 {
        self.rgba.as_ptr()
    }
}
