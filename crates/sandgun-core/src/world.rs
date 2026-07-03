use crate::cell::{Cell, Material};

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

    pub(crate) fn next_rand(&mut self) -> u32 {
        // xorshift32 — deterministic, no external deps
        let mut r = self.rng;
        r ^= r << 13;
        r ^= r >> 17;
        r ^= r << 5;
        self.rng = r;
        r
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
                self.cells[i] = Cell { material, shade, flags: 0, aux: 0 };
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
        for y in (0..self.height).rev() {
            for x_raw in 0..self.width {
                let x = if ltr { x_raw } else { self.width - 1 - x_raw };
                self.update_cell(x, y);
            }
        }
    }

    fn update_cell(&mut self, x: usize, y: usize) {
        let i = self.idx(x, y);
        if self.stamp[i] == self.frame_u8() {
            return; // already moved this frame
        }
        let mat = Material::from_u8(self.cells[i].material);
        if mat == Material::Empty || mat.is_solid() {
            return;
        }
        self.cells_processed += 1;
        if mat.is_powder() {
            self.update_powder(x, y);
        } else if mat.is_liquid() {
            self.update_liquid(x, y, mat);
        }
    }

    fn update_powder(&mut self, x: usize, y: usize) {
        let (xi, yi) = (x as isize, y as isize);

        // Try straight down
        if self.in_bounds(xi, yi + 1) {
            let below_idx = self.idx(xi as usize, (yi + 1) as usize);
            let dst = Material::from_u8(self.cells[below_idx].material);
            if dst == Material::Empty || dst.is_liquid() {
                self.swap_cells(x, y, xi as usize, (yi + 1) as usize);
                return;
            }
            // Only slide diagonally if:
            // 1. Blocked by powder (sand) AND
            // 2. There's also powder above (unstable peak - needs support from above to trigger sliding)
            if dst.is_powder() {
                let above_idx = self.idx(xi as usize, (yi - 1) as usize);
                let above_material = if yi > 0 {
                    Material::from_u8(self.cells[above_idx].material)
                } else {
                    Material::Empty
                };

                if above_material.is_powder() {
                    // Unstable configuration - try sliding diagonally
                    let first_dx = if self.next_rand() & 1 == 0 { -1 } else { 1 };
                    let candidates = [(xi + first_dx, yi + 1), (xi - first_dx, yi + 1)];
                    for (nx, ny) in candidates {
                        if !self.in_bounds(nx, ny) {
                            continue;
                        }
                        let diag_idx = self.idx(nx as usize, ny as usize);
                        let diag_dst = Material::from_u8(self.cells[diag_idx].material);
                        if diag_dst == Material::Empty || diag_dst.is_liquid() {
                            self.swap_cells(x, y, nx as usize, ny as usize);
                            return;
                        }
                    }
                }
            }
        }
    }

    fn update_liquid(&mut self, _x: usize, _y: usize, _mat: Material) {
        // Task 3
    }
}
