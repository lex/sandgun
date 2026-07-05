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

    // 3c. mycelium veins: random walks through the soil
    for _ in 0..(w / 12).max(6) {
        let mut vx = rng.range(2, w as i32 - 2);
        let mut vy = surface[vx as usize] + rng.range(1, 6);
        let len = rng.range(15, 50);
        for _ in 0..len {
            if vx < 1 || vy < 1 || vx as usize >= w - 1 || vy as usize >= h - 1 {
                break;
            }
            if world.get(vx as usize, vy as usize) == Material::Soil {
                set(world, vx as usize, vy as usize, Material::Mycelium, &mut rng);
            }
            // wander, biased sideways so veins read as veins
            vx += rng.range(-1, 2);
            vy += if rng.chance(1, 3) { rng.range(-1, 2) } else { 0 };
        }
    }

    // 3d. mushroom groves: stems + caps grown up from cave floors
    let mut floors: Vec<(usize, usize)> = Vec::new();
    for x in (2..w - 2).step_by(3) {
        for yy in (surface[x] as usize + 2)..h - 2 {
            let here = world.get(x, yy);
            let below = world.get(x, yy + 1);
            if here == Material::Empty && (below == Material::Rock || below == Material::Soil) {
                floors.push((x, yy));
                break; // one candidate per column
            }
        }
    }
    if !floors.is_empty() {
        let grove_count = (w / 28).max(3);
        for g in 0..grove_count {
            let (fx, fy) = floors[(rng.next() as usize) % floors.len()];
            let giant = g < 2; // the first two groves are giants
            let height = if giant { rng.range(14, 24) } else { rng.range(4, 10) } as usize;
            let cap_rx = if giant { rng.range(8, 14) } else { rng.range(3, 7) };
            let cap_ry = (cap_rx / 2).max(2);
            // stem
            for dy in 0..height {
                if fy > dy {
                    let sy = fy - dy;
                    if world.get(fx, sy) == Material::Empty {
                        set(world, fx, sy, Material::MushroomFlesh, &mut rng);
                    }
                    if giant && fx + 1 < w && world.get(fx + 1, sy) == Material::Empty {
                        set(world, fx + 1, sy, Material::MushroomFlesh, &mut rng);
                    }
                }
            }
            // elliptical cap, only into open air
            let top = fy as i32 - height as i32;
            for dy in -cap_ry..=0 {
                for dx in -cap_rx..=cap_rx {
                    let f = (dx * dx) as f32 / (cap_rx * cap_rx).max(1) as f32
                        + (dy * dy) as f32 / (cap_ry * cap_ry).max(1) as f32;
                    if f > 1.0 {
                        continue;
                    }
                    let (cx2, cy2) = (fx as i32 + dx, top + dy);
                    if cx2 < 0 || cy2 < 0 || cx2 as usize >= w || cy2 as usize >= h {
                        continue;
                    }
                    if world.get(cx2 as usize, cy2 as usize) == Material::Empty {
                        set(world, cx2 as usize, cy2 as usize, Material::MushroomFlesh, &mut rng);
                    }
                }
            }
        }
    }

    // 3e. spore pockets: gas blobs in cave air; they rise and pool by themselves
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

    // 6. M1e: bake substrate richness into every Soil cell's aux so mycelium tips have
    // something to eat once the new organism model starts growing (Task 2+). The old
    // frontier/colonize model is dormant — worldgen no longer calls seed_frontier().
    let richness_min = world.params.values[crate::params::P_SOIL_RICHNESS_MIN] as i32;
    let richness_max = world.params.values[crate::params::P_SOIL_RICHNESS_MAX] as i32;
    for x in 0..w {
        for yy in 0..h {
            if world.get(x, yy) == Material::Soil {
                let richness = rng.range(richness_min, richness_max + 1).clamp(0, 255) as u8;
                world.set_soil_richness(x, yy, richness);
            }
        }
    }
}
