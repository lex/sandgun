use crate::cell::{Cell, Material};
use crate::world::World;

/// Generation-local xorshift32 so the sim's own RNG stream is untouched.
struct GenRng(u32);

impl GenRng {
    fn next(&mut self) -> u32 {
        let mut r = self.0;
        r ^= r << 13;
        r ^= r >> 17;
        r ^= r << 5;
        self.0 = r;
        r
    }
    /// true with probability num/den
    fn chance(&mut self, num: u32, den: u32) -> bool {
        self.next() % den < num
    }
    /// uniform in [lo, hi)
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo) as u32) as i32
    }
}

fn set(world: &mut World, x: usize, y: usize, m: Material, rng: &mut GenRng) {
    let i = y * world.width + x;
    world.cells[i] = Cell { material: m as u8, shade: (rng.next() & 3) as u8, flags: 0, aux: 0 };
}

fn blob(world: &mut World, rng: &mut GenRng, cx: i32, cy: i32, radius: i32, m: Material) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x as usize >= world.width || y as usize >= world.height {
                continue;
            }
            set(world, x as usize, y as usize, m, rng);
        }
    }
}

pub fn generate(world: &mut World, seed: u32) {
    let (w, h) = (world.width, world.height);
    let mut rng = GenRng(seed.wrapping_mul(0x9E37_79B9) | 1);
    world.clear();

    // 1. surface heightline: bounded random walk, box-smoothed
    let mut surface = vec![0i32; w];
    let mut y = h as i32 / 3;
    for x in 0..w {
        y += rng.range(-1, 2);
        y = y.clamp(h as i32 / 5, h as i32 / 2);
        surface[x] = y;
    }
    for _ in 0..2 {
        let prev = surface.clone();
        for x in 1..w - 1 {
            surface[x] = (prev[x - 1] + prev[x] + prev[x + 1]) / 3;
        }
    }

    // 2. rock below the surface
    for x in 0..w {
        for yy in surface[x] as usize..h {
            set(world, x, yy, Material::Rock, &mut rng);
        }
    }

    // 3. caves: cellular automata in the rock body, sparing a 4-cell crust
    let mut open = vec![false; w * h];
    for x in 0..w {
        for yy in 0..h {
            if (yy as i32) > surface[x] + 4 {
                open[yy * w + x] = rng.chance(45, 100);
            }
        }
    }
    for _ in 0..4 {
        let prev = open.clone();
        for x in 1..w - 1 {
            for yy in 1..h - 1 {
                if (yy as i32) <= surface[x] + 4 {
                    continue;
                }
                let mut n = 0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if (dx, dy) == (0, 0) {
                            continue;
                        }
                        if prev[((yy as i32 + dy) as usize) * w + (x as i32 + dx) as usize] {
                            n += 1;
                        }
                    }
                }
                open[yy * w + x] = if prev[yy * w + x] { n >= 4 } else { n >= 5 };
            }
        }
    }
    for x in 0..w {
        for yy in 0..h {
            if open[yy * w + x] {
                set(world, x, yy, Material::Empty, &mut rng);
            }
        }
    }

    // 4. material pockets
    for _ in 0..(w / 24).max(4) {
        // sand dunes straddling the surface — they slump alive on frame one
        let x = rng.range(6, w as i32 - 6);
        let sy = surface[x as usize] - 2;
        let r = rng.range(3, 7);
        blob(world, &mut rng, x, sy, r, Material::Sand);
    }
    let mid = (h as i32 * 2) / 3;
    let mut placed_water = 0;
    let mut placed_oil = 0;
    for _ in 0..4000 {
        if placed_water >= (w / 16).max(6) && placed_oil >= (w / 20).max(5) {
            break;
        }
        let x = rng.range(4, w as i32 - 4);
        let yy = rng.range(surface[x as usize] + 5, h as i32 - 4);
        if world.get(x as usize, yy as usize) != Material::Empty {
            continue;
        }
        if yy < mid && placed_water < (w / 16).max(6) {
            let r = rng.range(2, 5);
            blob(world, &mut rng, x, yy, r, Material::Water);
            placed_water += 1;
        } else if yy >= mid && placed_oil < (w / 20).max(5) {
            let r = rng.range(2, 4);
            blob(world, &mut rng, x, yy, r, Material::Oil);
            placed_oil += 1;
        }
    }

    // 5. everything settles alive
    world.wake_all();
}
