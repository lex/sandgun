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
    world.cells[i] = Cell::new(m, (rng.next() & 3) as u8);
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

    // 3b. soil crust: the top skin of remaining rock becomes colonizable soil
    for x in 0..w {
        let depth = 6 + (rng.next() % 5) as usize;
        for d in 0..depth {
            let yy = surface[x] as usize + d;
            if yy < h && world.get(x, yy) == Material::Rock {
                set(world, x, yy, Material::Soil, &mut rng);
            }
        }
    }

    // 3c. deep soil: playtest feedback was that the subsurface was too rock-heavy for mycelium
    // to have anywhere to grow. Below the crust, most of the remaining Rock becomes Soil too --
    // rock stays only as a structural MINORITY (smoothed chunky regions/veins, not a solid wall).
    // A biased cellular automaton (same technique as the cave carve above, but seeded soil-heavy
    // and biased to keep growing) turns salt-and-pepper noise into coherent soil masses with rock
    // remnants, rather than a checkerboard. Caves (already carved to Empty above) are untouched --
    // this pass only ever touches cells that are still Rock, so it can't fill in or block a cave.
    let mut soil_mask = vec![false; w * h];
    for x in 0..w {
        for yy in 0..h {
            if world.get(x, yy) == Material::Rock {
                soil_mask[yy * w + x] = rng.chance(72, 100);
            }
        }
    }
    for _ in 0..3 {
        let prev = soil_mask.clone();
        for x in 1..w - 1 {
            for yy in 1..h - 1 {
                if world.get(x, yy) != Material::Rock {
                    continue; // only rock cells are candidates; soil/empty/etc are untouched
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
                // Biased toward soil (stays soil on a bare majority, converts on a supermajority)
                // so rock survives only in the thicker/more isolated pockets -- a minority,
                // structural material rather than the default fill.
                soil_mask[yy * w + x] = if prev[yy * w + x] { n >= 3 } else { n >= 5 };
            }
        }
    }
    for x in 0..w {
        for yy in 0..h {
            if soil_mask[yy * w + x] && world.get(x, yy) == Material::Rock {
                set(world, x, yy, Material::Soil, &mut rng);
            }
        }
    }

    // 3d. spore pockets: gas blobs in cave air; they rise and pool by themselves
    let mut placed_spores = 0;
    for _ in 0..3000 {
        if placed_spores >= (w / 20).max(5) {
            break;
        }
        let x = rng.range(4, w as i32 - 4);
        let yy = rng.range(surface[x as usize] + 4, h as i32 - 4);
        if world.get(x as usize, yy as usize) != Material::Empty {
            continue;
        }
        let r = rng.range(2, 4);
        blob(world, &mut rng, x, yy, r, Material::SporeGas);
        placed_spores += 1;
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

    // 6. bake substrate richness into every Soil cell's aux so mycelium tips have something to
    // eat as soon as colonies start growing.
    let richness_min = world.params.values[crate::params::P_SOIL_RICHNESS_MIN] as i32;
    let richness_max = world.params.values[crate::params::P_SOIL_RICHNESS_MAX] as i32;
    let mut soil_sites: Vec<(usize, usize)> = Vec::new();
    for x in 0..w {
        for yy in 0..h {
            if world.get(x, yy) == Material::Soil {
                let richness = rng.range(richness_min, richness_max + 1).clamp(0, 255) as u8;
                world.set_soil_richness(x, yy, richness);
                soil_sites.push((x, yy));
            }
        }
    }

    // 7. seed colony origins: each colony starts as one mycelium cell + one tip on a Soil cell,
    // so it has substrate to eat immediately. Replaces the old pre-filled mycelium veins/mushroom
    // groves -- the world now grows its own mycelium (and, eventually, mushrooms) outward from
    // these origins via the living organism model, rather than starting pre-grown.
    let colony_count = world.params.values[crate::params::P_MY_WORLDGEN_COLONIES] as usize;
    if !soil_sites.is_empty() {
        for _ in 0..colony_count {
            let (sx, sy) = soil_sites[(rng.next() as usize) % soil_sites.len()];
            world.spawn_colony(sx, sy);
        }
    }
}
