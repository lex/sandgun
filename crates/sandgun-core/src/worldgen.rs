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

/// A depth band of the subsurface: a soil/rock mix ratio (before caves) plus the initial cave
/// open-cell density the Task 2 CA carve will use for that depth.
struct Biome {
    top: usize,    // inclusive world row where this band starts
    soil_pct: u32, // 0..100 chance a filled cell is Soil vs Rock (before caves)
    // 0..100 initial open-cell chance for the cave CA carve (pass 3), per depth band.
    cave_seed_pct: u32,
}

/// Partition [surface .. h) into depth bands: a soil-rich crust zone near the top grading to
/// rock-dominant depths. Boundaries scale with world height so it works at any size.
fn biomes(h: usize) -> Vec<Biome> {
    vec![
        Biome { top: 0, soil_pct: 78, cave_seed_pct: 42 },        // upper: soft, soil-rich
        Biome { top: h * 2 / 5, soil_pct: 45, cave_seed_pct: 46 }, // mid: mixed
        Biome { top: h * 7 / 10, soil_pct: 20, cave_seed_pct: 40 }, // deep: rock-dominant
    ]
}

fn biome_at<'a>(bands: &'a [Biome], y: usize) -> &'a Biome {
    bands.iter().rev().find(|b| y >= b.top).unwrap_or(&bands[0])
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

/// BFS-flood Empty cells from the open sky (top row) using 4-connectivity. Returns whether the
/// bottom row was reached, plus the deepest (max-y) open cell reached (first one found at that
/// depth, if there are ties) -- the launch point for `ensure_descendable`'s connectivity carve.
fn flood_from_surface(world: &World) -> (bool, usize, usize) {
    use std::collections::VecDeque;
    let (w, h) = (world.width, world.height);
    let mut seen = vec![false; w * h];
    let mut q = VecDeque::new();
    for x in 0..w {
        if world.get(x, 0) == Material::Empty {
            seen[x] = true;
            q.push_back((x, 0usize));
        }
    }
    let mut reached_bottom = false;
    let (mut deepest_x, mut deepest_y) = (0usize, 0usize);
    while let Some((x, y)) = q.pop_front() {
        if y > deepest_y {
            deepest_y = y;
            deepest_x = x;
        }
        if y == h - 1 {
            reached_bottom = true;
        }
        for (nx, ny) in [
            (x as i32 - 1, y as i32),
            (x as i32 + 1, y as i32),
            (x as i32, y as i32 - 1),
            (x as i32, y as i32 + 1),
        ] {
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if !seen[ny * w + nx] && world.get(nx, ny) == Material::Empty {
                seen[ny * w + nx] = true;
                q.push_back((nx, ny));
            }
        }
    }
    (reached_bottom, deepest_x, deepest_y)
}

/// Guarantee a top-to-bottom Empty path exists. The cavern CA carve above is density-driven and
/// can seal pockets off from each other (especially in the rock-dominant deep band), so after
/// carving we flood Empty from the open sky; if the bottom row wasn't reached, carve a
/// meandering shaft -- a downward-biased random walk, x jittered by rng.range(-1,2) each step,
/// width 2-3 cells -- from the deepest surface-reachable point down to the bottom row. This
/// guarantees a descendable path regardless of how the CA carve happened to connect; caves then
/// read as branches off this guaranteed route.
fn ensure_descendable(world: &mut World, rng: &mut GenRng) {
    let (w, h) = (world.width, world.height);
    let (reached_bottom, x0, y0) = flood_from_surface(world);
    if reached_bottom {
        return;
    }
    // Random walk downward from the deepest surface-reachable open cell. Each row's carved span
    // always covers BOTH the previous and the new center column (plus a 1-cell margin), so
    // consecutive rows are guaranteed to share at least one column -- that's what makes the shaft
    // provably 4-connected end to end, regardless of how the jitter happens to land.
    let mut cur_x = x0 as i32;
    let mut y = y0;
    while y < h - 1 {
        y += 1;
        let next_x = (cur_x + rng.range(-1, 2)).clamp(0, w as i32 - 1);
        let lo = (cur_x.min(next_x) - 1).clamp(0, w as i32 - 1);
        let hi = (cur_x.max(next_x) + 1).clamp(0, w as i32 - 1);
        for cx in lo..=hi {
            set(world, cx as usize, y, Material::Empty, rng);
        }
        cur_x = next_x;
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

    // 2. depth-graded base fill: soil-rich near the surface grading to rock-dominant deep, via
    // per-band per-cell rolls (replaces the old uniform-rock-then-global-soil-CA passes). Caves
    // are carved into this fill below (pass 3); soil beds/liquid pools are set-pieces for Task 3.
    let bands = biomes(h);
    let mut soil_mask = vec![false; w * h];
    for x in 0..w {
        for yy in surface[x] as usize..h {
            let band = biome_at(&bands, yy);
            soil_mask[yy * w + x] = rng.chance(band.soil_pct, 100);
        }
    }
    // 2b. majority-smoothing: 2 passes of the standard "majority of 8 neighbors wins, ties keep
    // the current state" rule kill salt-and-pepper noise so each band reads as a cohesive soil or
    // rock mass rather than a checkerboard. Only touches cells at/below the surface line so the
    // open sky above is untouched.
    for _ in 0..2 {
        let prev = soil_mask.clone();
        for x in 1..w - 1 {
            for yy in 1..h - 1 {
                if (yy as i32) < surface[x] {
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
                soil_mask[yy * w + x] = if n == 4 { prev[yy * w + x] } else { n > 4 };
            }
        }
    }
    for x in 0..w {
        for yy in surface[x] as usize..h {
            let m = if soil_mask[yy * w + x] { Material::Soil } else { Material::Rock };
            set(world, x, yy, m, &mut rng);
        }
    }

    // 3. caves: cellular automata in the rock body, sparing a 4-cell crust. The initial open-cell
    // density is per-biome (cave_seed_pct) rather than a flat rate, so caverns are sparser in the
    // soft upper band and looser in the mixed/deep bands -- the CA smoothing (isotropic 8-neighbor
    // majority, below) then reads as organic pockets rather than streaks regardless of band.
    let mut open = vec![false; w * h];
    for x in 0..w {
        for yy in 0..h {
            if (yy as i32) > surface[x] + 4 {
                let band = biome_at(&bands, yy);
                open[yy * w + x] = rng.chance(band.cave_seed_pct, 100);
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

    // 3b. spore pockets: gas blobs in cave air; they rise and pool by themselves
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

    // 4b. guaranteed descendable path: the cavern carve is a density-driven CA and is not
    // guaranteed to connect the surface all the way to the bottom of the world (pockets can seal
    // off from each other), and the material-pocket blobs above (3b/4) can themselves fill in
    // pieces of a carved cave with non-Empty material. So this check runs last, after every pass
    // that can still write solid/liquid/gas material, right before the world goes live: flood-fill
    // Empty from the open sky and, if the bottom row wasn't reached, carve a meandering shaft down
    // from the deepest surface-reachable point so the world is always descendable top-to-bottom.
    // Caves then read as branches off this guaranteed route. (Task 3 will add its own set-piece
    // placement after this point and must re-run this check so its liquid pools can't reseal the
    // descent -- see the plan's Task 3 section.)
    ensure_descendable(world, &mut rng);

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
