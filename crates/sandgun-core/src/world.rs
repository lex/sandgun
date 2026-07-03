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
}
